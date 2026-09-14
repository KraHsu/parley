use super::*;

#[derive(Default)]
struct Slot {
    client: Mutex<Option<Arc<Client>>>,
    gate: AsyncMutex<()>,
    generation: AtomicU64,
}

#[derive(Default)]
pub struct CodexState {
    slots: Mutex<HashMap<String, Arc<Slot>>>,
    closing: AtomicBool,
}

impl CodexState {
    fn slot(&self, id: &str) -> Arc<Slot> {
        self.slots
            .lock()
            .unwrap()
            .entry(id.into())
            .or_default()
            .clone()
    }
    pub(super) fn get(&self, id: &str) -> Result<Arc<Client>, String> {
        let slot = self
            .slots
            .lock()
            .unwrap()
            .get(id)
            .cloned()
            .ok_or("请先连接此 Codex 服务。")?;
        slot.client
            .lock()
            .unwrap()
            .as_ref()
            .filter(|c| c.alive.load(Ordering::SeqCst))
            .cloned()
            .ok_or_else(|| "请先连接此 Codex 服务。".into())
    }
    pub fn shutdown(&self) {
        self.closing.store(true, Ordering::SeqCst);
        let slots: Vec<_> = self.slots.lock().unwrap().values().cloned().collect();
        for slot in slots {
            slot.generation.fetch_add(1, Ordering::SeqCst);
            if let Some(client) = slot.client.lock().unwrap().as_ref() {
                client.close("Parley 已关闭。");
            }
        }
    }
    pub(super) async fn disconnect(&self, id: Option<&str>) -> Result<(), String> {
        let slots: Vec<_> = {
            let slots = self.slots.lock().unwrap();
            slots
                .iter()
                .filter(|(key, _)| id.is_none_or(|id| key.as_str() == id))
                .map(|(_, slot)| slot.clone())
                .collect()
        };
        // Signal every process before waiting, so a slow process does not delay
        // stopping the others. In-progress launches also observe cancellation.
        let mut stops = Vec::new();
        for slot in slots {
            let generation = slot.generation.fetch_add(1, Ordering::SeqCst) + 1;
            if let Some(client) = slot.client.lock().unwrap().as_ref() {
                client.close("已断开 Codex，本机账号登录状态保留。");
            }
            stops.push(async move {
                let _gate = slot.gate.lock().await;
                if slot.generation.load(Ordering::SeqCst) != generation {
                    return Ok(());
                }
                let client = slot.client.lock().unwrap().clone();
                if let Some(client) = client {
                    client.stop().await?;
                    slot.client.lock().unwrap().take();
                }
                Ok::<(), String>(())
            });
        }
        for result in futures_util::future::join_all(stops).await {
            result?;
        }
        Ok(())
    }
}

pub(super) async fn connect(
    state: &CodexState,
    storage: StorageState,
    root: PathBuf,
    events: Channel<Value>,
    codex_path: String,
    profile_id: String,
    expected_revision: Option<i64>,
) -> Reply {
    if profile_id.is_empty() || profile_id.len() > 200 {
        return Err("无效的 Codex 配置标识。".into());
    }
    let slot = state.slot(&profile_id);
    let generation = slot.generation.fetch_add(1, Ordering::SeqCst) + 1;
    let _gate = slot.gate.lock().await;
    let cancelled = || {
        state.closing.load(Ordering::SeqCst) || slot.generation.load(Ordering::SeqCst) != generation
    };
    if cancelled() {
        return Err("Codex 连接已取消。".into());
    }
    let (store, profile, binary, cwd) = storage
        .run(move |store| {
            let profile = store.backend_profile(&profile_id)?;
            if !profile.config.enabled || profile.config.kind != BackendKind::Codex {
                return Err("所选 Codex 服务已停用或类型不匹配。".into());
            }
            if expected_revision.is_some_and(|revision| revision != profile.revision) {
                return Err("Codex 配置已更新，请重新读取设置后连接。".into());
            }
            if profile.config.binary_path != codex_path.trim() {
                return Err("Codex 路径与保存的配置不同，请先保存设置再连接。".into());
            }
            let binary = configured_binary(&codex_path)?;
            let cwd = if profile_id == DEFAULT_CODEX_PROFILE {
                root.join("conversation-workspace")
            } else {
                root.join("codex-workspaces")
                    .join(crate::vocabulary::digest(&profile_id))
            };
            std::fs::create_dir_all(&cwd).map_err(|e| e.to_string())?;
            Ok((store, profile, binary, cwd))
        })
        .await?;
    if cancelled() {
        return Err("Codex 连接已取消。".into());
    }
    let old = slot.client.lock().unwrap().clone();
    if let Some(old) = old {
        old.stop().await?;
        slot.client.lock().unwrap().take();
    }
    if cancelled() {
        return Err("Codex 连接已取消。".into());
    }
    let id = profile.id.clone();
    storage
        .run(move |store| store.interrupt_backend(&id))
        .await?;
    let client = launch_profile(
        binary,
        cwd,
        events,
        store,
        ConversationBackend {
            profile_id: profile.id,
            profile_revision: profile.revision,
            kind: BackendKind::Codex,
        },
    )
    .await?;
    let published = {
        let mut current = slot.client.lock().unwrap();
        if cancelled() {
            false
        } else {
            *current = Some(client.clone());
            true
        }
    };
    if !published {
        client.stop().await?;
        return Err("Codex 连接已取消。".into());
    }
    // Initialization is cancellable through the published client; do not hold
    // the launch gate while waiting for protocol replies.
    drop(_gate);
    let mut result = initialize(&client).await?;
    result["profileId"] = json!(client.profile.profile_id);
    result["profileRevision"] = json!(client.profile.profile_revision);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backends::types::{BackendProfile, SaveProfile};
    use crate::storage::ConversationConfig;

    fn profile(storage: &Storage, name: &str) -> BackendProfile {
        let mut config = storage
            .backend_profile(DEFAULT_CODEX_PROFILE)
            .unwrap()
            .config;
        config.name = name.into();
        storage
            .save_backend_profile(SaveProfile {
                id: None,
                expected_revision: None,
                config,
            })
            .unwrap()
    }
    fn client(
        storage: &Storage,
        profile: &BackendProfile,
        pane: &str,
    ) -> (Arc<Client>, Arc<Mutex<Vec<Value>>>) {
        let (mut client, _peer, events) = super::super::tests::test_client();
        let c = Arc::get_mut(&mut client).unwrap();
        c.storage = storage.clone();
        c.profile = ConversationBackend {
            profile_id: profile.id.clone(),
            profile_revision: profile.revision,
            kind: BackendKind::Codex,
        };
        c.exited.store(true, Ordering::SeqCst);
        c.reader_done.store(true, Ordering::SeqCst);
        storage.create_for_backend(pane, pane, &profile.id).unwrap();
        storage
            .begin(
                pane,
                "user",
                "hello",
                ConversationConfig {
                    model: "test",
                    target: "en",
                    native: "zh-CN",
                    mode: "conversation",
                },
            )
            .unwrap();
        *c.lanes[pane_index(pane).unwrap()].lock().unwrap() = Lane {
            active: true,
            conversation: Some(pane.into()),
            thread: Some("same-upstream-thread".into()),
            ..Default::default()
        };
        (client, events)
    }
    fn delta(text: &str) -> Value {
        json!({"method":"item/agentMessage/delta","params":{"threadId":"same-upstream-thread","turnId":"same-turn","itemId":"same-item","delta":text}})
    }

    #[tokio::test]
    async fn independent_profiles_route_colliding_ids_and_ignore_events_after_disconnect() {
        let storage = Storage::memory();
        let a = profile(&storage, "A");
        let b = profile(&storage, "B");
        let (ca, ea) = client(&storage, &a, "main");
        let (cb, eb) = client(&storage, &b, "tutor");
        let state = CodexState::default();
        *state.slot(&a.id).client.lock().unwrap() = Some(ca.clone());
        *state.slot(&b.id).client.lock().unwrap() = Some(cb.clone());
        ca.incoming(delta("first")).await;
        cb.incoming(delta("second")).await;
        state.disconnect(Some(&a.id)).await.unwrap();
        assert!(state.get(&a.id).is_err());
        assert!(state.get(&b.id).is_ok());
        let before = ea.lock().unwrap().len();
        ca.incoming(delta("late")).await;
        ca.incoming(json!({"method":"item/completed","params":{"threadId":"same-upstream-thread","item":{"id":"same-item","type":"agentMessage","text":"late overwrite"}}})).await;
        ca.incoming(json!({"method":"turn/completed","params":{"threadId":"same-upstream-thread","turn":{"status":"completed"}}})).await;
        assert_eq!(ea.lock().unwrap().len(), before);
        let main = storage.read("main").unwrap();
        assert_eq!(main.status, "interrupted");
        assert_eq!(main.messages[1].text, "first");
        assert_eq!(main.messages[1].status, "interrupted");
        cb.incoming(delta(" continues")).await;
        let tutor = storage.read("tutor").unwrap();
        assert_eq!(tutor.status, "running");
        assert_eq!(tutor.messages[1].text, "second continues");
        assert_eq!(eb.lock().unwrap()[0]["profileId"], b.id);
        assert_eq!(eb.lock().unwrap()[0]["profileRevision"], b.revision);
        assert_eq!(ea.lock().unwrap()[0]["params"]["pane"], "main");
        state.disconnect(None).await.unwrap();
        assert!(state.get(&b.id).is_err());
        assert_eq!(storage.read("tutor").unwrap().status, "interrupted");
    }

    #[tokio::test]
    async fn disconnect_cancels_a_pending_initialization_without_waiting_for_rpc_timeout() {
        let (c, peer, _) = super::super::tests::test_client();
        c.exited.store(true, Ordering::SeqCst);
        c.reader_done.store(true, Ordering::SeqCst);
        let state = CodexState::default();
        *state.slot(DEFAULT_CODEX_PROFILE).client.lock().unwrap() = Some(c.clone());
        let pending = c.clone();
        let task = tokio::spawn(async move { initialize(&pending).await });
        BufReader::new(peer)
            .lines()
            .next_line()
            .await
            .unwrap()
            .unwrap();
        state.disconnect(Some(DEFAULT_CODEX_PROFILE)).await.unwrap();
        assert!(
            tokio::time::timeout(Duration::from_secs(1), task)
                .await
                .unwrap()
                .unwrap()
                .is_err()
        );
        assert!(c.pending.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn disconnect_during_send_keeps_the_saved_interruption() {
        let (c, peer, _) = super::super::tests::test_client();
        let sender = c.clone();
        let task = tokio::spawn(async move {
            send_message(
                &sender,
                serde_json::from_value(json!({
                    "pane":"main", "conversationId":"main", "messageId":"user",
                    "text":"hello", "model":"test", "targetLanguage":"en",
                    "nativeLanguage":"zh-CN", "mode":"conversation"
                }))
                .unwrap(),
            )
            .await
        });
        BufReader::new(peer)
            .lines()
            .next_line()
            .await
            .unwrap()
            .unwrap();
        c.close("disconnected while reading account");
        assert!(task.await.unwrap().is_err());
        let saved = c.storage.read("main").unwrap();
        assert_eq!(saved.status, "interrupted");
        assert_eq!(saved.messages[0].status, "interrupted");
    }

    #[tokio::test]
    async fn disconnect_invalidates_a_queued_launch_before_it_opens_storage() {
        let state = CodexState::default();
        let slot = state.slot(DEFAULT_CODEX_PROFILE);
        let gate = slot.gate.lock().await;
        let pending = connect(
            &state,
            StorageState::new(Err("must not open storage".into())),
            PathBuf::new(),
            Channel::new(|_| Ok(())),
            String::new(),
            DEFAULT_CODEX_PROFILE.into(),
            None,
        );
        tokio::pin!(pending);
        tokio::select! {
            biased;
            result = &mut pending => panic!("launch skipped its gate: {result:?}"),
            _ = tokio::task::yield_now() => {}
        }
        let disconnect = state.disconnect(Some(DEFAULT_CODEX_PROFILE));
        tokio::pin!(disconnect);
        tokio::select! {
            biased;
            result = &mut disconnect => panic!("disconnect skipped its gate: {result:?}"),
            _ = tokio::task::yield_now() => {}
        }
        drop(gate);
        let (launch, stopped) = tokio::join!(pending, disconnect);
        assert!(launch.unwrap_err().contains("连接已取消"));
        stopped.unwrap();
        assert!(state.get(DEFAULT_CODEX_PROFILE).is_err());
    }

    #[tokio::test]
    async fn profiles_and_revisions_are_checked_before_starting_a_turn() {
        let storage = Storage::memory();
        let a = profile(&storage, "A");
        let b = profile(&storage, "B");
        let (ca, _) = client(&storage, &a, "main");
        storage.interrupt_backend(&a.id).unwrap();
        storage
            .create_for_backend("wrong-profile", "main", &b.id)
            .unwrap();
        let request = |id: &str| {
            serde_json::from_value::<MessageRequest>(json!({"pane":"main","conversationId":id,"messageId":"next-user","text":"hello","model":"test","targetLanguage":"en","nativeLanguage":"zh-CN","mode":"conversation"})).unwrap()
        };
        assert!(
            send_message(&ca, request("wrong-profile"))
                .await
                .unwrap_err()
                .contains("其他模型服务")
        );
        let mut config = a.config.clone();
        config.name = "Updated A".into();
        storage
            .save_backend_profile(SaveProfile {
                id: Some(a.id.clone()),
                expected_revision: Some(a.revision),
                config,
            })
            .unwrap();
        assert!(
            send_message(&ca, request("main"))
                .await
                .unwrap_err()
                .contains("重新连接")
        );
        assert_eq!(storage.read("main").unwrap().messages.len(), 1);
        assert!(storage.read("wrong-profile").unwrap().messages.is_empty());
    }

    #[tokio::test]
    #[ignore = "requires PARLEY_TEST_CODEX_BIN and local ChatGPT login; two profiles, no model calls"]
    async fn live_two_profiles_share_native_login_and_disconnect_independently() {
        let binary = std::env::var("PARLEY_TEST_CODEX_BIN").unwrap();
        let root = tempfile::tempdir().unwrap();
        let storage = StorageState::new(Ok(root.path().join("test.sqlite3")));
        let store = storage.get().unwrap();
        let make_profile = |name: &str| {
            let initial = profile(&store, name);
            let mut config = initial.config.clone();
            config.binary_path = binary.clone();
            store
                .save_backend_profile(SaveProfile {
                    id: Some(initial.id),
                    expected_revision: Some(initial.revision),
                    config,
                })
                .unwrap()
        };
        let a = make_profile("A");
        let b = make_profile("B");
        let state = CodexState::default();
        let start = |p: BackendProfile| {
            connect(
                &state,
                storage.clone(),
                root.path().to_owned(),
                Channel::new(|_| Ok(())),
                binary.clone(),
                p.id,
                Some(p.revision),
            )
        };
        let (ra, rb) = tokio::join!(start(a.clone()), start(b.clone()));
        let verification = async {
            ra?;
            rb?;
            let ca = state.get(&a.id)?;
            let cb = state.get(&b.id)?;
            assert_ne!(ca.cwd, cb.cwd);
            let (sa, sb) = tokio::join!(read_status(&ca, "account"), read_status(&cb, "account"));
            assert_eq!(sa?["account"]["type"], "chatgpt");
            assert_eq!(sb?["account"]["type"], "chatgpt");
            state.disconnect(Some(&a.id)).await?;
            assert!(state.get(&a.id).is_err());
            assert!(
                !read_status(&cb, "models").await?["models"]
                    .as_array()
                    .unwrap()
                    .is_empty()
            );
            Ok::<(), String>(())
        }
        .await;
        state.disconnect(None).await.unwrap();
        verification.unwrap();
    }
}
