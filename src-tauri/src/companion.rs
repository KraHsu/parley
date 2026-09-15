//! Reopen a companion GUI without replacing or restarting its native terminal.
use crate::launcher::TerminalBackend;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Companion {
    pub id: String,
    pub backend: TerminalBackend,
    pub binary: PathBuf,
    pub cwd: PathBuf,
    pub started_at: u64,
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
    pub fn select(id: Option<&str>) -> Result<Self, String> {
        let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
        Self::list()?.into_iter().find(|s| id.map_or(s.cwd == cwd, |id| s.id == id))
            .ok_or_else(|| "未找到可重连的伴随会话。用 --list-companions 查看记录，或先通过 parley-cli 启动终端。".into())
    }
}
