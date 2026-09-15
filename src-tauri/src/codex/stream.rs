//! Bounded notification delivery; control RPC replies never wait for a disk write.
use super::*;
use std::sync::{atomic::AtomicUsize, mpsc};
use std::time::Instant;
use tokio::io::AsyncBufRead;

const MAX_FRAME: usize = 4 * 1024 * 1024;
const MAX_QUEUED_BYTES: usize = 8 * 1024 * 1024;
const MAX_QUEUED_EVENTS: usize = 256;
const FLUSH_INTERVAL: Duration = Duration::from_millis(50);

pub(super) struct Queued {
    value: Value,
    bytes: usize,
    budget: Arc<AtomicUsize>,
}
impl Drop for Queued {
    fn drop(&mut self) {
        self.budget.fetch_sub(self.bytes, Ordering::SeqCst);
    }
}
pub(super) struct Queue {
    sender: mpsc::SyncSender<Queued>,
    budget: Arc<AtomicUsize>,
}
pub(super) fn queue() -> (Queue, mpsc::Receiver<Queued>) {
    let (sender, receiver) = mpsc::sync_channel(MAX_QUEUED_EVENTS);
    (
        Queue {
            sender,
            budget: Arc::new(AtomicUsize::new(0)),
        },
        receiver,
    )
}
impl Queue {
    pub fn push(&self, value: Value, bytes: usize) -> Result<(), String> {
        self.budget
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |current| {
                current
                    .checked_add(bytes)
                    .filter(|next| *next <= MAX_QUEUED_BYTES)
            })
            .map_err(|_| "本地回复保存队列已满，请检查磁盘后重新连接。".to_owned())?;
        let event = Queued {
            value,
            bytes,
            budget: self.budget.clone(),
        };
        self.sender
            .try_send(event)
            .map_err(|_| "本地回复处理未能及时跟上，已停止连接。".to_owned())
    }
}
pub(super) fn run(client: Arc<Client>, receiver: mpsc::Receiver<Queued>) {
    struct Done(Arc<Client>);
    impl Drop for Done {
        fn drop(&mut self) {
            self.0.notifications_done.store(true, Ordering::SeqCst);
            self.0.exit_notify.notify_waiters();
        }
    }
    let _done = Done(client.clone());
    let result = (|| -> Result<(), String> {
        let mut deadline: Option<Instant> = None;
        loop {
            if !client.alive.load(Ordering::SeqCst) {
                return Ok(());
            }
            let event = match deadline {
                Some(at) => receiver.recv_timeout(at.saturating_duration_since(Instant::now())),
                None => receiver
                    .recv()
                    .map_err(|_| mpsc::RecvTimeoutError::Disconnected),
            };
            match event {
                Ok(event) => {
                    let method = event.value["method"]
                        .as_str()
                        .ok_or("Codex 通知缺少方法名。")?;
                    client.notification(method, &event.value["params"], true)?;
                    if matches!(
                        method,
                        "item/agentMessage/delta" | "item/completed" | "thread/tokenUsage/updated"
                    ) {
                        deadline.get_or_insert_with(|| Instant::now() + FLUSH_INTERVAL);
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    client.flush_notifications()?;
                    return Ok(());
                }
            }
            if deadline.is_some_and(|at| Instant::now() >= at) {
                client.flush_notifications()?;
                deadline = None;
            }
        }
    })();
    if let Err(error) = result {
        client.emit("connection/notice", json!({"message":error}));
        client.close("Codex 回复处理失败，已停止连接。");
    }
}

pub(super) async fn read_frame(
    reader: &mut (impl AsyncBufRead + Unpin),
) -> Result<Option<String>, String> {
    let mut line = Vec::new();
    loop {
        let buffer = reader
            .fill_buf()
            .await
            .map_err(|e| format!("Codex 输出读取失败：{e}"))?;
        if buffer.is_empty() {
            return if line.is_empty() {
                Ok(None)
            } else {
                String::from_utf8(line)
                    .map(Some)
                    .map_err(|_| "Codex 返回了无效 UTF-8 数据。".into())
            };
        }
        let end = buffer.iter().position(|b| *b == b'\n');
        let consumed = end.map_or(buffer.len(), |n| n + 1);
        if line.len() + consumed > MAX_FRAME {
            return Err("Codex 单条协议消息超过本地大小限制。".into());
        }
        line.extend_from_slice(&buffer[..consumed]);
        reader.consume(consumed);
        if end.is_some() {
            return String::from_utf8(line)
                .map(Some)
                .map_err(|_| "Codex 返回了无效 UTF-8 数据。".into());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn setup() -> (
        Arc<Client>,
        tokio::io::DuplexStream,
        tokio::sync::mpsc::UnboundedReceiver<crate::chat::TurnEvent>,
    ) {
        let (c, peer, _) = super::super::tests::test_client();
        let (sender, events) = tokio::sync::mpsc::unbounded_channel();
        let publication = turns::fixture_publication(
            c.storage.clone(),
            c.storage.backend_profile(DEFAULT_CODEX_PROFILE).unwrap(),
            "main",
            Channel::new(move |body| {
                if let tauri::ipc::InvokeResponseBody::Json(text) = body {
                    sender.send(serde_json::from_str(&text).unwrap()).unwrap();
                }
                Ok(())
            }),
        );
        {
            let mut lane = c.lanes[0].lock().unwrap();
            lane.active = true;
            lane.thread = Some("thread".into());
            lane.request_id = Some("user".into());
            lane.publication = Some(publication);
        }
        c.notifications_done.store(false, Ordering::SeqCst);
        (c, peer, events)
    }
    fn started() -> Value {
        json!({"method":"turn/started","params":{"threadId":"thread","turn":{"id":"turn"}}})
    }
    fn delta(text: &str) -> Value {
        json!({"method":"item/agentMessage/delta","params":{"threadId":"thread","turnId":"turn","itemId":"item","delta":text}})
    }
    fn push(queue: &Queue, value: Value) {
        let bytes = value.to_string().len();
        queue.push(value, bytes).unwrap();
    }

    #[test]
    fn queue_limits_both_payload_bytes_and_event_count_without_leaking_reservations() {
        let (q, rx) = queue();
        q.push(json!({}), MAX_QUEUED_BYTES).unwrap();
        assert!(q.push(json!({}), 1).is_err());
        drop(rx.recv().unwrap());
        assert_eq!(q.budget.load(Ordering::SeqCst), 0);
        for _ in 0..MAX_QUEUED_EVENTS {
            q.push(json!({}), 1).unwrap();
        }
        assert!(q.push(json!({}), 1).is_err());
        assert_eq!(q.budget.load(Ordering::SeqCst), MAX_QUEUED_EVENTS);
        drop(rx);
        assert_eq!(q.budget.load(Ordering::SeqCst), 0);
        assert!(q.push(json!({}), 1).is_err());
        assert_eq!(q.budget.load(Ordering::SeqCst), 0);
    }
    #[tokio::test]
    async fn frames_preserve_chunked_utf8_and_reject_overlong_or_invalid_data() {
        for capacity in [1, 2, 7, 64] {
            let mut reader = BufReader::with_capacity(capacity, "你好 café\nnext".as_bytes());
            assert_eq!(
                read_frame(&mut reader).await.unwrap().as_deref(),
                Some("你好 café\n")
            );
            assert_eq!(
                read_frame(&mut reader).await.unwrap().as_deref(),
                Some("next")
            );
            assert!(read_frame(&mut reader).await.unwrap().is_none());
        }
        let oversized = vec![b'x'; MAX_FRAME + 1];
        assert!(
            read_frame(&mut BufReader::new(oversized.as_slice()))
                .await
                .unwrap_err()
                .contains("大小限制")
        );
        assert!(
            read_frame(&mut BufReader::new(&[0xff, b'\n'][..]))
                .await
                .unwrap_err()
                .contains("UTF-8")
        );
    }
    #[tokio::test]
    async fn burst_deltas_are_batched_and_eof_flushes_the_latest_partial_reply() {
        let (c, _, mut events) = setup();
        let (q, rx) = queue();
        let client = c.clone();
        let worker = tauri::async_runtime::spawn_blocking(move || run(client, rx));
        push(&q, started());
        for _ in 0..100 {
            push(&q, delta("é"));
        }
        drop(q);
        worker.await.unwrap();
        let saved = c.storage.read("main").unwrap();
        assert_eq!(saved.messages[1].text, "é".repeat(100));
        assert_eq!(saved.status, "running");
        let mut publications = 0;
        while events.try_recv().is_ok() {
            publications += 1;
        }
        assert!(publications < 20, "unbatched publications: {publications}");
        c.exited.store(true, Ordering::SeqCst);
        c.reader_done.store(true, Ordering::SeqCst);
        c.stop().await.unwrap();
        assert_eq!(c.storage.read("main").unwrap().status, "interrupted");
        assert_eq!(
            c.storage.read("main").unwrap().messages[1].text,
            "é".repeat(100)
        );
    }
    #[tokio::test]
    async fn idle_stream_flushes_without_waiting_for_another_notification() {
        let (c, _, mut events) = setup();
        events.recv().await.unwrap();
        let (q, rx) = queue();
        let client = c.clone();
        let worker = tauri::async_runtime::spawn_blocking(move || run(client, rx));
        push(&q, started());
        push(&q, delta("pending fragment"));
        let event = tokio::time::timeout(Duration::from_secs(1), events.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(event.text, "pending fragment");
        assert_eq!(event.status, "streaming");
        assert_eq!(c.storage.read("main").unwrap().messages[1].text, event.text);
        drop(q);
        worker.await.unwrap();
    }
    #[tokio::test]
    async fn terminal_notification_flushes_complete_text_once_without_waiting_for_eof() {
        let (c, _, mut events) = setup();
        let (q, rx) = queue();
        let client = c.clone();
        let worker = tauri::async_runtime::spawn_blocking(move || run(client, rx));
        push(&q, started());
        push(&q, delta("Bon"));
        push(
            &q,
            json!({"method":"item/completed","params":{"threadId":"thread","turnId":"turn","item":{"id":"item","type":"agentMessage","text":"Bonjour"}}}),
        );
        let completed = json!({"method":"turn/completed","params":{"threadId":"thread","turn":{"id":"turn","status":"completed"}}});
        push(&q, completed.clone());
        let terminal = tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                let event = events.recv().await.unwrap();
                if event.status != "streaming" {
                    break event;
                }
            }
        })
        .await
        .unwrap();
        assert_eq!(terminal.status, "complete");
        assert_eq!(terminal.text, "Bonjour");
        assert_eq!(c.storage.read("main").unwrap().status, "idle");
        push(&q, completed);
        push(&q, delta("late content"));
        drop(q);
        worker.await.unwrap();
        assert!(events.try_recv().is_err());
        assert_eq!(c.storage.read("main").unwrap().messages[1].text, "Bonjour");
    }
    #[tokio::test]
    async fn control_replies_and_cancel_do_not_wait_for_the_notification_writer() {
        let (c, peer, _events) = setup();
        c.incoming(started()).await;
        let gate = c.notifications.clone();
        let (locked, ready) = std::sync::mpsc::channel();
        let (release, released) = std::sync::mpsc::channel();
        let holder = std::thread::spawn(move || {
            let _gate = gate.lock().unwrap();
            locked.send(()).unwrap();
            let _ = released.recv_timeout(Duration::from_secs(3));
        });
        ready.recv().unwrap();
        let (q, rx) = queue();
        let client = c.clone();
        let worker = tauri::async_runtime::spawn_blocking(move || run(client, rx));
        push(&q, delta("queued while saving"));
        let client = c.clone();
        let pending = tokio::spawn(async move { client.rpc("account/read", json!({})).await });
        let mut lines = BufReader::new(peer).lines();
        let request: Value =
            serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
        c.incoming(json!({"id":request["id"],"result":{"account":null}}))
            .await;
        tokio::time::timeout(Duration::from_millis(500), pending)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let client = c.clone();
        let stop = tokio::spawn(async move { stop_request(&client, "main", Some("user")).await });
        let frame = tokio::time::timeout(Duration::from_millis(500), lines.next_line())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let request: Value = serde_json::from_str(&frame).unwrap();
        assert_eq!(request["method"], "turn/interrupt");
        assert_eq!(request["params"]["turnId"], "turn");
        c.incoming(json!({"id":request["id"],"result":{}})).await;
        stop.await.unwrap().unwrap();
        release.send(()).unwrap();
        holder.join().unwrap();
        drop(q);
        worker.await.unwrap();
    }
}
