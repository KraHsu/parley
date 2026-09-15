//! Reopen a companion GUI without replacing or restarting its native terminal.
use crate::launcher::TerminalBackend;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Companion {
    pub id: String,
    pub backend: TerminalBackend,
    pub binary: PathBuf,
    pub cwd: PathBuf,
    pub started_at: u64,
    #[serde(default)]
    process: Option<ProcessIdentity>,
}
fn root() -> Result<PathBuf, String> {
    dirs::cache_dir()
        .map(|path| path.join("org.parley.desktop/companions"))
        .ok_or_else(|| "无法确定终端伴随缓存目录。".into())
}
impl Companion {
    pub fn create(backend: TerminalBackend, binary: &Path, cwd: &Path) -> Result<Self, String> {
        let session = Self {
            id: uuid::Uuid::new_v4().to_string(),
            backend,
            binary: binary.to_owned(),
            cwd: cwd.to_owned(),
            process: ProcessIdentity::read(std::process::id()),
            started_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        };
        let directory = session.directory()?;
        std::fs::create_dir_all(directory.parent().ok_or("缓存路径无效")?)
            .map_err(|e| e.to_string())?;
        let mut builder = std::fs::DirBuilder::new();
        builder.recursive(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(&directory).map_err(|e| e.to_string())?;
        crate::exchange::write_atomic(
            &directory.join("companion.json"),
            &serde_json::to_vec(&session).map_err(|e| e.to_string())?,
        )?;
        Ok(session)
    }
    pub fn directory(&self) -> Result<PathBuf, String> {
        uuid::Uuid::parse_str(&self.id).map_err(|_| "伴随会话 ID 无效。")?;
        Ok(root()?.join(&self.id))
    }
    pub fn list() -> Result<Vec<Self>, String> {
        let root = root()?;
        if !root.exists() {
            return Ok(vec![]);
        }
        let mut sessions = Vec::new();
        for directory in std::fs::read_dir(root)
            .map_err(|e| e.to_string())?
            .flatten()
        {
            let path = directory.path().join("companion.json");
            if !path.metadata().is_ok_and(|m| m.len() <= 16384) {
                continue;
            }
            if let Ok(bytes) = std::fs::read(path)
                && let Ok(session) = serde_json::from_slice::<Self>(&bytes)
                && session.directory().is_ok_and(|p| p == directory.path())
            {
                sessions.push(session);
            }
        }
        sessions.sort_by(|a, b| b.started_at.cmp(&a.started_at).then(a.id.cmp(&b.id)));
        Ok(sessions)
    }
    pub fn status(&self) -> CompanionStatus {
        let Some(identity) = &self.process else {
            return CompanionStatus::Unknown;
        };
        let mut system = System::new();
        let pid = Pid::from_u32(identity.pid);
        system.refresh_processes_specifics(
            ProcessesToUpdate::Some(&[pid]),
            true,
            ProcessRefreshKind::nothing(),
        );
        match system.process(pid) {
            Some(process)
                if process.start_time() == identity.started_at
                    && System::boot_time() == identity.boot_time =>
            {
                if matches!(
                    process.status(),
                    ProcessStatus::Zombie | ProcessStatus::Dead
                ) {
                    CompanionStatus::Ended
                } else {
                    CompanionStatus::Running
                }
            }
            _ => CompanionStatus::Ended,
        }
    }
    #[cfg(not(unix))]
    pub fn track(&mut self, pid: u32) -> Result<(), String> {
        self.process = ProcessIdentity::read(pid);
        crate::exchange::write_atomic(
            &self.directory()?.join("companion.json"),
            &serde_json::to_vec(self).map_err(|e| e.to_string())?,
        )
    }
    pub fn remove(&self) -> Result<(), String> {
        if self.status() == CompanionStatus::Running {
            return Err("终端仍在运行，请退出原生 CLI 后再清理缓存。".into());
        }
        std::fs::remove_dir_all(self.directory()?).map_err(|e| e.to_string())
    }
    pub fn select(id: Option<&str>) -> Result<Self, String> {
        let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
        Self::list()?.into_iter().find(|s| id.map_or(s.cwd == cwd && s.status() == CompanionStatus::Running, |id| s.id == id))
            .ok_or_else(|| "未找到正在运行的伴随会话。用 --list-companions 查看记录；查看历史缓存请显式指定 --session ID。".into())
    }
}

use sysinfo::{Pid, ProcessRefreshKind, ProcessStatus, ProcessesToUpdate, System};
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProcessIdentity {
    pid: u32,
    started_at: u64,
    boot_time: u64,
}
impl ProcessIdentity {
    fn read(pid: u32) -> Option<Self> {
        let mut system = System::new();
        system.refresh_processes_specifics(
            ProcessesToUpdate::Some(&[Pid::from_u32(pid)]),
            true,
            ProcessRefreshKind::nothing(),
        );
        system.process(Pid::from_u32(pid)).map(|p| Self {
            pid,
            started_at: p.start_time(),
            boot_time: System::boot_time(),
        })
    }
}
#[derive(Clone, Copy, Serialize, PartialEq, Eq, Debug)]
#[serde(rename_all = "kebab-case")]
pub enum CompanionStatus {
    Running,
    Ended,
    Unknown,
}
impl CompanionStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Running => "运行中",
            Self::Ended => "已结束",
            Self::Unknown => "状态未知",
        }
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanionView {
    #[serde(flatten)]
    session: Companion,
    status: CompanionStatus,
}
impl From<Companion> for CompanionView {
    fn from(session: Companion) -> Self {
        Self {
            status: session.status(),
            session,
        }
    }
}
#[derive(Default)]
pub struct CompanionSelection(pub std::sync::Mutex<Option<Companion>>);
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanionList {
    sessions: Vec<CompanionView>,
    selected_id: Option<String>,
}
#[tauri::command]
pub fn companion_list(
    state: tauri::State<'_, CompanionSelection>,
) -> Result<CompanionList, String> {
    Ok(CompanionList {
        sessions: Companion::list()?.into_iter().map(Into::into).collect(),
        selected_id: state
            .0
            .lock()
            .map_err(|e| e.to_string())?
            .as_ref()
            .map(|s| s.id.clone()),
    })
}
#[tauri::command]
pub fn companion_select(
    state: tauri::State<'_, CompanionSelection>,
    terminal: tauri::State<'_, crate::terminal::TerminalState>,
    id: Option<String>,
) -> Result<Option<CompanionView>, String> {
    let session = id
        .as_deref()
        .map(|id| Companion::select(Some(id)))
        .transpose()?;
    let directory = session.as_ref().map(Companion::directory).transpose()?;
    let mut selected = state.0.lock().map_err(|e| e.to_string())?;
    terminal.select(directory)?;
    *selected = session.clone();
    Ok(session.map(Into::into))
}
#[tauri::command]
pub fn companion_remove(
    state: tauri::State<'_, CompanionSelection>,
    id: String,
) -> Result<(), String> {
    let selected = state.0.lock().map_err(|e| e.to_string())?;
    if selected.as_ref().is_some_and(|s| s.id == id) {
        return Err("请先取消关联此会话，再清理缓存。".into());
    }
    Companion::select(Some(&id))?.remove()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn identity_does_not_confuse_reused_pid_or_previous_boot_with_live_process() {
        let mut session = Companion {
            id: uuid::Uuid::new_v4().to_string(),
            backend: TerminalBackend::Codex,
            binary: "codex".into(),
            cwd: ".".into(),
            started_at: 0,
            process: ProcessIdentity::read(std::process::id()),
        };
        assert_eq!(session.status(), CompanionStatus::Running);
        session.process.as_mut().unwrap().started_at += 1;
        assert_eq!(session.status(), CompanionStatus::Ended);
        session.process = ProcessIdentity::read(std::process::id());
        session.process.as_mut().unwrap().boot_time += 1;
        assert_eq!(session.status(), CompanionStatus::Ended);
        session.process = None;
        assert_eq!(session.status(), CompanionStatus::Unknown);
    }
}
