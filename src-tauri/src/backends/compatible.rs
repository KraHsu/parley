//! Chat Completions provider differences. See docs/BACKEND_COMPATIBILITY.md.
use super::{http::Output, types::Provider};
use crate::storage::api::ApiTurn;
use serde_json::{Value, json};

pub(super) fn request(turn: &ApiTurn, messages: Vec<Value>) -> Value {
    let provider = turn.profile.config.provider;
    let mut body = json!({"model":turn.model,"messages":messages,"stream":true});
    // Z.AI reports usage on the final chunk without this undocumented parameter.
    if provider != Provider::Zai {
        body["stream_options"] = json!({"include_usage":true});
    }
    body[if provider == Provider::Openai {
        "max_completion_tokens"
    } else {
        "max_tokens"
    }] = json!(4096);
    // Keep each model's defaults for thinking and sampling: some models reject
    // a universal thinking toggle or explicitly supplied sampling parameters.
    body
}

impl Output {
    pub(super) fn finish_compatible(&mut self) -> Result<(), String> {
        if self.finish.as_deref() != Some("stop") {
            return Err("回复未正常完成，已保留部分内容。".into());
        }
        if self.text.trim().is_empty() {
            return Err("服务没有返回可显示的正文，请检查所选模型或重新提问。".into());
        }
        self.complete = true;
        if !self.reasoning.is_empty() {
            self.continuation = Some(json!({"reasoning_content":self.reasoning}));
        }
        Ok(())
    }

    pub(super) fn compatible(&mut self, value: &Value) -> Result<(), String> {
        if value.get("choices").is_none()
            && value.get("message").is_some_and(Value::is_string)
            && value
                .get("code")
                .is_some_and(|code| !code.is_null() && code != 0 && code != "0")
        {
            return Err(
                "服务在生成过程中返回错误，已保留部分回复；请检查额度和服务状态后重试。".into(),
            );
        }
        if let Some(usage) = value.get("usage").filter(|v| v.is_object()) {
            self.usage = Some(usage.clone());
        }
        let Some(choices) = value.get("choices").and_then(Value::as_array) else {
            return Ok(());
        };
        for choice in choices {
            if choice["index"].as_u64().unwrap_or(0) != 0 {
                continue;
            }
            let delta = &choice["delta"];
            if delta
                .get("tool_calls")
                .is_some_and(|v| !v.is_null() && v.as_array().is_none_or(|a| !a.is_empty()))
                || delta.get("function_call").is_some_and(|v| !v.is_null())
            {
                return Err("纯聊天后端收到工具调用，已停止。".into());
            }
            for (key, destination) in [
                ("content", &mut self.text),
                ("reasoning_content", &mut self.reasoning),
            ] {
                if let Some(value) = delta.get(key).filter(|v| !v.is_null()) {
                    let text = value.as_str().ok_or("服务返回了无效的文本增量。")?;
                    if self.finish.is_some() && !text.is_empty() {
                        return Err("服务在完成标识后继续返回正文，已保留部分内容。".into());
                    }
                    destination.push_str(text);
                }
            }
            if let Some(reason) = choice.get("finish_reason").filter(|v| !v.is_null()) {
                let reason = reason.as_str().ok_or("服务返回了无效的完成标识。")?;
                self.finish = Some(reason.into());
                match reason {
                    "stop" => {}
                    "length" => return Err(
                        "回复达到模型输出或上下文限制，已保留部分内容。请缩短问题或选择其他模型。"
                            .into(),
                    ),
                    "content_filter" | "sensitive" => {
                        return Err("服务未能提供完整回复，已保留收到的内容。".into());
                    }
                    "tool_calls" | "function_call" => {
                        return Err("纯聊天后端收到工具调用，已停止。".into());
                    }
                    _ => return Err("服务返回了未成功完成的状态，已保留部分内容。".into()),
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
pub(super) mod fixtures {
    use super::*;
    #[derive(serde::Deserialize)]
    pub struct Case {
        pub provider: Provider,
        pub model: String,
        pub stream_options: bool,
        pub events: Vec<Value>,
        pub error_event: Value,
    }
    pub fn cases() -> Vec<Case> {
        serde_json::from_str(include_str!("fixtures/compatible.json")).unwrap()
    }
    impl Case {
        pub fn wire(&self) -> String {
            let mut wire = String::from(": keep-alive\n\n");
            for event in &self.events {
                wire.push_str(&format!("data: {event}\n\n"));
            }
            wire.push_str("data: [DONE]\n\n");
            wire
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backends::{sse::Decoder, types::BackendKind};

    #[test]
    fn vendor_chunks_keep_body_usage_and_reasoning_separate_at_every_byte_boundary() {
        for case in fixtures::cases() {
            let wire = case.wire();
            for boundary in 0..=wire.len() {
                let mut decoder = Decoder::default();
                let mut output = Output::default();
                for chunk in [&wire.as_bytes()[..boundary], &wire.as_bytes()[boundary..]] {
                    for event in decoder.push(chunk).unwrap() {
                        output
                            .accept(BackendKind::OpenaiCompatible, &event)
                            .unwrap();
                    }
                }
                assert!(output.complete, "{:?}", case.provider);
                assert_eq!(output.text, "Bonjour 🌍");
                assert_eq!(output.usage.unwrap()["total_tokens"], 28);
                assert_eq!(
                    output.continuation.unwrap()["reasoning_content"],
                    "fixture 思考"
                );
            }
        }
    }

    #[test]
    fn context_budget_counts_visible_answers_reasoning_and_json_escaping() {
        use crate::backends::http::{MAX_CONTEXT, request_body};
        use crate::storage::{Storage, api_tests::turn};
        let storage = Storage::memory();
        let mut turn = turn(&storage, "c", "main");
        turn.profile.config.kind = BackendKind::OpenaiCompatible;
        turn.profile.config.provider = Provider::Kimi;
        let history = vec![
            (
                "old".into(),
                "x".repeat(MAX_CONTEXT),
                Some(json!({"reasoning_content":"short"})),
            ),
            (
                "recent".into(),
                "answer".into(),
                Some(json!({"reasoning_content":"kept"})),
            ),
        ];
        let (body, clipped) = request_body(&turn, &history).unwrap();
        assert!(clipped);
        assert_eq!(body["messages"].as_array().unwrap().len(), 4);
        assert_eq!(body["messages"][2]["content"], "answer");
        assert_eq!(body["messages"][2]["reasoning_content"], "kept");
        assert!(body["messages"].to_string().len() <= MAX_CONTEXT);
        turn.input = "\0".repeat(MAX_CONTEXT / 3);
        assert!(request_body(&turn, &[]).is_err());
    }

    #[test]
    fn failed_empty_or_malformed_outputs_never_become_successful_answers() {
        for case in fixtures::cases() {
            let mut output = Output::default();
            output
                .accept(
                    BackendKind::OpenaiCompatible,
                    &json!({"choices":[{"index":0,"delta":{"content":"partial"}}]}).to_string(),
                )
                .unwrap();
            let error = output
                .accept(BackendKind::OpenaiCompatible, &case.error_event.to_string())
                .unwrap_err();
            assert!(!error.contains("fixture-secret"));
            assert!(!output.complete);
            assert_eq!(output.text, "partial");
        }
        for content in [json!(""), json!(null), json!(" \n")] {
            let mut output = Output::default();
            output.accept(BackendKind::OpenaiCompatible, &json!({"choices":[{"index":0,"delta":{"content":content,"reasoning_content":"fixture"},"finish_reason":"stop"}]}).to_string()).unwrap();
            assert!(
                output
                    .accept(BackendKind::OpenaiCompatible, "[DONE]")
                    .unwrap_err()
                    .contains("正文")
            );
            assert!(output.continuation.is_none());
        }
        for reason in [
            "length",
            "content_filter",
            "sensitive",
            "tool_calls",
            "function_call",
            "unknown",
        ] {
            let mut output = Output::default();
            assert!(output.accept(BackendKind::OpenaiCompatible, &json!({"choices":[{"index":0,"delta":{"content":"partial"},"finish_reason":reason}],"usage":{"total_tokens":10}}).to_string()).is_err());
            assert_eq!(output.text, "partial");
            assert_eq!(output.usage.unwrap()["total_tokens"], 10);
            assert!(!output.complete);
        }
        let mut output = Output::default();
        assert!(
            output
                .accept(
                    BackendKind::OpenaiCompatible,
                    r#"{"choices":[{"delta":{"content":42}}]}"#
                )
                .is_err()
        );
    }
}
