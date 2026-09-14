use serde::{Deserialize, Serialize};
use std::path::Path;
use url::{Host, Url};

pub const DEFAULT_CODEX_PROFILE: &str = "codex-default";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackendKind {
    Codex,
    ClaudeCode,
    OpenaiResponses,
    AnthropicMessages,
    GeminiInteractions,
    OpenaiCompatible,
}

impl BackendKind {
    pub fn is_cli(self) -> bool {
        matches!(self, Self::Codex | Self::ClaudeCode)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    Openai,
    Anthropic,
    Google,
    Deepseek,
    Qwen,
    Kimi,
    Zai,
    Custom,
}

/// Deliberately no arbitrary JSON or headers: secrets cannot hide in public settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProfileConfig {
    pub name: String,
    pub kind: BackendKind,
    pub provider: Provider,
    pub endpoint: String,
    pub binary_path: String,
    pub enabled: bool,
}

impl ProfileConfig {
    pub fn validate(&mut self) -> Result<(), String> {
        self.name = self.name.trim().to_owned();
        self.endpoint = self.endpoint.trim().to_owned();
        self.binary_path = self.binary_path.trim().to_owned();
        if self.name.is_empty() || self.name.len() > 160 {
            return Err("服务名称不能为空，且不能超过 160 字节。".into());
        }
        if !matches!(
            (self.kind, self.provider),
            (
                BackendKind::Codex | BackendKind::OpenaiResponses,
                Provider::Openai
            ) | (
                BackendKind::ClaudeCode | BackendKind::AnthropicMessages,
                Provider::Anthropic
            ) | (BackendKind::GeminiInteractions, Provider::Google)
                | (BackendKind::OpenaiCompatible, _)
        ) {
            return Err("厂商与接入协议不匹配。".into());
        }
        if self.kind.is_cli() {
            if !self.endpoint.is_empty() {
                return Err("本机 CLI 配置不接受 API 地址。".into());
            }
            if self.binary_path.len() > 4096
                || self.binary_path.contains(['\0', '\n', '\r'])
                || (!self.binary_path.is_empty() && !Path::new(&self.binary_path).is_absolute())
            {
                return Err("请填写本机 CLI 的绝对路径。".into());
            }
        } else {
            if !self.binary_path.is_empty() {
                return Err("API 配置不接受可执行文件路径。".into());
            }
            if self.endpoint.len() > 2048 {
                return Err("API 地址过长。".into());
            }
            let url = Url::parse(&self.endpoint).map_err(|_| "API 地址无效。".to_owned())?;
            let loopback = match url.host() {
                Some(Host::Ipv4(ip)) => ip.is_loopback(),
                Some(Host::Ipv6(ip)) => ip.is_loopback(),
                Some(Host::Domain(name)) => name == "localhost",
                None => false,
            };
            if url.host().is_none()
                || !url.username().is_empty()
                || url.password().is_some()
                || url.query().is_some()
                || url.fragment().is_some()
                || !(url.scheme() == "https" || (url.scheme() == "http" && loopback))
            {
                return Err("API 地址需使用 HTTPS（本机回环地址可用 HTTP），且不能包含凭据、查询参数或片段。".into());
            }
            self.endpoint = url.as_str().trim_end_matches('/').to_owned();
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackendProfile {
    pub id: String,
    pub revision: i64,
    pub config: ProfileConfig,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveProfile {
    pub id: Option<String>,
    pub expected_revision: Option<i64>,
    pub config: ProfileConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationBackend {
    pub profile_id: String,
    pub profile_revision: i64,
    pub kind: BackendKind,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn api(endpoint: &str) -> ProfileConfig {
        ProfileConfig {
            name: "API".into(),
            kind: BackendKind::OpenaiCompatible,
            provider: Provider::Custom,
            endpoint: endpoint.into(),
            binary_path: String::new(),
            enabled: true,
        }
    }

    #[test]
    fn public_config_rejects_embedded_credentials_and_insecure_remote_hosts() {
        for endpoint in [
            "http://example.com/v1",
            "https://user:secret@example.com",
            "https://example.com?api_key=secret",
            "https://example.com/#secret",
            "file:///tmp/model",
        ] {
            let error = api(endpoint).validate().unwrap_err();
            assert!(!error.contains("secret"));
        }
        for endpoint in [
            "https://example.com/v1/",
            "http://127.0.0.1:8080/v1",
            "http://[::1]:8080/v1",
            "http://localhost:8080/v1",
        ] {
            api(endpoint).validate().unwrap();
        }
        let mut value = serde_json::to_value(api("https://example.com")).unwrap();
        value["apiKey"] = "secret".into();
        assert!(serde_json::from_value::<ProfileConfig>(value).is_err());
    }
}
