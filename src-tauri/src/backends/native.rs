//! Native Messages and Interactions SSE state machines. Only plain chat is enabled.
use super::http::{MAX_RESPONSE, Output};
use serde_json::{Value, json};

fn append(value: &mut Value, field: &str, delta: &str) -> Result<(), String> {
    if value.get(field).is_none() {
        value[field] = json!("");
    }
    let Value::String(current) = &mut value[field] else {
        return Err("内容块文本字段无效。".into());
    };
    if current.len() + delta.len() > MAX_RESPONSE {
        return Err("模型回复过长，已停止接收。".into());
    }
    current.push_str(delta);
    Ok(())
}
fn append_content(value: &mut Value, field: &str, delta: &Value) -> Result<(), String> {
    if delta["type"] != "text" {
        return Err("纯聊天后端收到非文本内容，已停止。".into());
    }
    let text = delta["text"].as_str().ok_or("正文增量无效。")?;
    if value.get(field).is_none() {
        value[field] = json!([]);
    }
    let parts = value[field].as_array_mut().ok_or("内容块列表无效。")?;
    if let Some(last) = parts.last_mut().filter(|p| p["type"] == "text") {
        append(last, "text", text)?;
    } else {
        parts.push(delta.clone());
    }
    Ok(())
}
fn text_parts(value: Option<&Value>) -> Result<String, String> {
    let Some(value) = value else {
        return Ok(String::new());
    };
    let mut text = String::new();
    for part in value.as_array().ok_or("内容块列表无效。")? {
        if part["type"] != "text" {
            return Err("纯聊天后端收到非文本内容，已停止。".into());
        }
        text.push_str(part["text"].as_str().ok_or("正文格式无效。")?);
    }
    Ok(text)
}
impl Output {
    fn merge_usage(&mut self, value: Option<&Value>) {
        if let Some(value) = value.and_then(Value::as_object) {
            let usage = self
                .usage
                .get_or_insert_with(|| json!({}))
                .as_object_mut()
                .unwrap();
            for (key, value) in value {
                usage.insert(key.clone(), value.clone());
            }
        }
    }
    fn start_block(&mut self, event: &Value, field: &str, allowed: &[&str]) -> Result<(), String> {
        let index = event["index"].as_u64().ok_or("内容块索引无效。")? as usize;
        if !self.started || self.open_block.is_some() || index != self.blocks.len() || index >= 1024
        {
            return Err("内容块顺序无效。".into());
        }
        let block = &event[field];
        if !allowed.contains(&block["type"].as_str().unwrap_or("")) {
            return Err("纯聊天后端收到工具调用或不支持的内容类型，已停止。".into());
        }
        self.blocks.push(block.clone());
        self.open_block = Some(index);
        Ok(())
    }
    fn current_block(&mut self, event: &Value) -> Result<&mut Value, String> {
        let index = event["index"].as_u64().ok_or("内容块索引无效。")? as usize;
        if self.open_block != Some(index) {
            return Err("收到没有对应活动内容块的事件。".into());
        }
        self.blocks.get_mut(index).ok_or("内容块不存在。".into())
    }
    fn stop_block(&mut self, event: &Value) -> Result<(), String> {
        self.current_block(event)?;
        self.open_block = None;
        Ok(())
    }
    fn finish_native(&mut self) -> Result<(), String> {
        if !self.started || self.open_block.is_some() || self.text.is_empty() {
            return Err("服务未返回完整的文本回复，已保留部分内容。".into());
        }
        self.continuation = Some(json!(self.blocks));
        self.complete = true;
        Ok(())
    }
    pub(super) fn anthropic(&mut self, event: &Value) -> Result<(), String> {
        match event["type"].as_str() {
            Some("message_start") => {
                if self.started || event["message"]["role"] != "assistant" {
                    return Err("Claude 消息起始事件无效。".into());
                }
                if event["message"]["content"]
                    .as_array()
                    .is_none_or(|c| !c.is_empty())
                {
                    return Err("Claude 消息起始内容无效。".into());
                }
                self.started = true;
                self.merge_usage(event["message"].get("usage"));
            }
            Some("content_block_start") => {
                self.start_block(
                    event,
                    "content_block",
                    &["text", "thinking", "redacted_thinking"],
                )?;
                if event["content_block"]["type"] == "text" {
                    self.text.push_str(
                        event["content_block"]["text"]
                            .as_str()
                            .ok_or("Claude 文本块无效。")?,
                    );
                }
            }
            Some("content_block_delta") => {
                let delta = &event["delta"];
                let block = self.current_block(event)?;
                match (block["type"].as_str(), delta["type"].as_str()) {
                    (Some("text"), Some("text_delta")) => {
                        let text = delta["text"].as_str().ok_or("Claude 文本增量无效。")?;
                        append(block, "text", text)?;
                        self.text.push_str(text);
                    }
                    (Some("thinking"), Some("thinking_delta")) => append(
                        block,
                        "thinking",
                        delta["thinking"].as_str().ok_or("Claude 思考内容无效。")?,
                    )?,
                    (Some("thinking"), Some("signature_delta")) => append(
                        block,
                        "signature",
                        delta["signature"].as_str().ok_or("Claude 续聊签名无效。")?,
                    )?,
                    _ => return Err("Claude 内容块与增量类型不匹配。".into()),
                }
            }
            Some("content_block_stop") => self.stop_block(event)?,
            Some("message_delta") => {
                if !self.started || self.open_block.is_some() {
                    return Err("Claude 消息结束顺序无效。".into());
                }
                if let Some(reason) = event["delta"]["stop_reason"].as_str() {
                    self.finish = Some(reason.into());
                }
                self.merge_usage(event.get("usage"));
            }
            Some("message_stop") => {
                if !matches!(
                    self.finish.as_deref(),
                    Some("end_turn" | "stop_sequence" | "refusal")
                ) {
                    return Err("Claude 回复未正常完成，可能达到输出限制。已保留部分内容。".into());
                }
                self.finish_native()?;
            }
            _ => {}
        }
        Ok(())
    }
    pub(super) fn gemini(&mut self, event: &Value) -> Result<(), String> {
        match event["event_type"].as_str() {
            Some("interaction.created") => {
                if self.started || event["interaction"]["status"] != "in_progress" {
                    return Err("Gemini 轮次起始事件无效。".into());
                }
                self.started = true;
            }
            Some("step.start") => {
                self.start_block(event, "step", &["thought", "model_output"])?;
                if event["step"]["type"] == "model_output" {
                    self.text
                        .push_str(&text_parts(event["step"].get("content"))?);
                }
            }
            Some("step.delta") => {
                let delta = &event["delta"];
                let block = self.current_block(event)?;
                match (block["type"].as_str(), delta["type"].as_str()) {
                    (Some("model_output"), Some("text")) => {
                        append_content(block, "content", delta)?;
                        self.text
                            .push_str(delta["text"].as_str().ok_or("Gemini 文本增量无效。")?);
                    }
                    (Some("thought"), Some("thought_signature")) => {
                        block["signature"] =
                            json!(delta["signature"].as_str().ok_or("Gemini 续聊签名无效。")?);
                    }
                    (Some("thought"), Some("thought_summary")) => {
                        append_content(block, "summary", &delta["content"])?
                    }
                    _ => return Err("Gemini 返回不支持的内容增量，已停止。".into()),
                }
                self.merge_usage(event["metadata"].get("total_usage"));
            }
            Some("step.stop") => self.stop_block(event)?,
            Some("interaction.completed") => {
                self.merge_usage(event["interaction"].get("usage"));
                if event["interaction"]["status"] != "completed" {
                    return Err("Gemini 回复未完成或要求工具操作。已保留部分内容。".into());
                }
                self.finish_native()?;
            }
            Some("interaction.failed" | "error") => {
                return Err("Gemini 在生成过程中返回错误，已保留部分回复。".into());
            }
            Some("interaction.status_update")
                if matches!(
                    event["status"].as_str(),
                    Some("failed" | "cancelled" | "requires_action")
                ) =>
            {
                return Err("Gemini 轮次已停止或要求工具操作。".into());
            }
            _ => {}
        }
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::{
        backends::{http::request_body, types::BackendKind},
        storage::{Storage, api_tests::turn},
    };
    fn send(out: &mut Output, kind: BackendKind, event: Value) {
        out.accept(kind, &event.to_string()).unwrap();
    }
    pub(crate) fn claude_events() -> Vec<Value> {
        vec![
            json!({"type":"message_start","message":{"role":"assistant","content":[],"usage":{"input_tokens":11,"output_tokens":1}}}),
            json!({"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":"","signature":""}}),
            json!({"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"fixture reasoning"}}),
            json!({"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"opaque-signature"}}),
            json!({"type":"content_block_stop","index":0}),
            json!({"type":"content_block_start","index":1,"content_block":{"type":"text","text":""}}),
            json!({"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"Bonjour 🌍"}}),
            json!({"type":"content_block_stop","index":1}),
            json!({"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":7}}),
            json!({"type":"message_stop"}),
        ]
    }
    pub(crate) fn gemini_events() -> Vec<Value> {
        vec![
            json!({"event_type":"interaction.created","interaction":{"status":"in_progress"}}),
            json!({"event_type":"step.start","index":0,"step":{"type":"thought"}}),
            json!({"event_type":"step.delta","index":0,"delta":{"type":"thought_summary","content":{"type":"text","text":"brief summary"}}}),
            json!({"event_type":"step.delta","index":0,"delta":{"type":"thought_signature","signature":"opaque-signature"}}),
            json!({"event_type":"step.stop","index":0}),
            json!({"event_type":"step.start","index":1,"step":{"type":"model_output"}}),
            json!({"event_type":"step.delta","index":1,"delta":{"type":"text","text":"Bonjour "}}),
            json!({"event_type":"step.delta","index":1,"delta":{"type":"text","text":"🌍"}}),
            json!({"event_type":"step.stop","index":1}),
            json!({"event_type":"interaction.completed","interaction":{"status":"completed","usage":{"total_input_tokens":11,"total_output_tokens":7,"total_tokens":18}}}),
        ]
    }
    #[test]
    fn native_protocols_keep_opaque_blocks_and_replay_them_in_their_own_formats() {
        for (kind, events) in [
            (BackendKind::AnthropicMessages, claude_events()),
            (BackendKind::GeminiInteractions, gemini_events()),
        ] {
            let mut out = Output::default();
            for e in events {
                send(&mut out, kind, e);
            }
            assert_eq!(out.text, "Bonjour 🌍");
            assert!(out.complete);
            let blocks = out.continuation.as_ref().unwrap();
            assert_eq!(blocks[0]["signature"], "opaque-signature");
            assert_eq!(
                out.usage.as_ref().unwrap()[if kind == BackendKind::AnthropicMessages {
                    "output_tokens"
                } else {
                    "total_output_tokens"
                }],
                7
            );
            if kind == BackendKind::AnthropicMessages {
                assert_eq!(out.usage.as_ref().unwrap()["input_tokens"], 11);
            }
            let s = Storage::memory();
            let mut turn = turn(&s, "c", "main");
            turn.profile.config.kind = kind;
            let (body, clipped) = request_body(
                &turn,
                &[(
                    "prior question".into(),
                    out.text.clone(),
                    out.continuation.clone(),
                )],
            )
            .unwrap();
            assert!(!clipped);
            if kind == BackendKind::AnthropicMessages {
                assert_eq!(body["messages"][1]["content"], *blocks);
                assert!(body["system"].is_string());
            } else {
                assert_eq!(body["input"][1], blocks[0]);
                assert_eq!(body["input"][2], blocks[1]);
                assert_eq!(body["store"], false);
                assert_eq!(body["input"][3]["type"], "user_input");
                assert!(body.get("previous_interaction_id").is_none());
            }
            assert!(body.get("tools").is_none());
        }
    }
    #[test]
    fn terminal_markers_do_not_hide_limits_tools_or_missing_block_ends() {
        let kind = BackendKind::AnthropicMessages;
        let mut events = claude_events();
        events[8]["delta"]["stop_reason"] = json!("max_tokens");
        let mut out = Output::default();
        for event in &events[..9] {
            send(&mut out, kind, event.clone());
        }
        assert!(out.accept(kind, &events[9].to_string()).is_err());
        assert_eq!(out.text, "Bonjour 🌍");
        assert!(!out.complete);
        for kind in [
            BackendKind::AnthropicMessages,
            BackendKind::GeminiInteractions,
        ] {
            let mut out = Output::default();
            let events = if kind == BackendKind::AnthropicMessages {
                claude_events()
            } else {
                gemini_events()
            };
            send(&mut out, kind, events[0].clone());
            let tool = if kind == BackendKind::AnthropicMessages {
                json!({"type":"content_block_start","index":0,"content_block":{"type":"tool_use"}})
            } else {
                json!({"event_type":"step.start","index":0,"step":{"type":"function_call"}})
            };
            assert!(out.accept(kind, &tool.to_string()).is_err());
            assert!(!out.complete);
            let mut out = Output::default();
            for event in &events[..7] {
                send(&mut out, kind, event.clone());
            }
            assert!(
                out.accept(kind, &events.last().unwrap().to_string())
                    .is_err()
            );
            assert!(!out.complete);
        }
        let mut out = Output::default();
        let mut events = gemini_events();
        events[9]["interaction"]["status"] = json!("requires_action");
        for event in &events[..9] {
            send(&mut out, BackendKind::GeminiInteractions, event.clone());
        }
        assert!(
            out.accept(BackendKind::GeminiInteractions, &events[9].to_string())
                .is_err()
        );
    }
    #[test]
    fn native_streams_ignore_unknown_metadata_but_reject_unmatched_deltas() {
        for kind in [
            BackendKind::AnthropicMessages,
            BackendKind::GeminiInteractions,
        ] {
            let mut out = Output::default();
            send(
                &mut out,
                kind,
                json!({"type":"future_metadata","event_type":"future_metadata","anything":true}),
            );
            assert!(!out.complete);
            let event = if kind == BackendKind::AnthropicMessages {
                json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"wrong"}})
            } else {
                json!({"event_type":"step.delta","index":0,"delta":{"type":"text","text":"wrong"}})
            };
            assert!(out.accept(kind, &event.to_string()).is_err());
            assert_eq!(out.text, "");
            assert!(out.accept(kind, "[DONE]").is_err());
        }
    }
}
