//! Bounded UTF-8 SSE framing; a network chunk is not a line or an event.
#[derive(Default)]
pub struct Decoder {
    line: Vec<u8>,
    data: String,
    after_cr: bool,
    first_line: bool,
    started: bool,
}

const MAX_EVENT: usize = 2 * 1024 * 1024;

impl Decoder {
    pub fn push(&mut self, bytes: &[u8]) -> Result<Vec<String>, String> {
        let mut events = Vec::new();
        for &byte in bytes {
            if self.after_cr && byte == b'\n' {
                self.after_cr = false;
                continue;
            }
            self.after_cr = byte == b'\r';
            if byte == b'\r' || byte == b'\n' {
                let raw = std::mem::take(&mut self.line);
                let line = std::str::from_utf8(&raw).map_err(|_| "服务返回了无效 UTF-8 流。")?;
                if !self.started {
                    self.first_line = true;
                    self.started = true;
                }
                let line = if self.first_line {
                    self.first_line = false;
                    line.trim_start_matches('\u{feff}')
                } else {
                    line
                };
                if line.is_empty() {
                    if !self.data.is_empty() {
                        self.data.pop(); // last newline is the SSE separator, not application data
                        events.push(std::mem::take(&mut self.data));
                    }
                } else if let Some(data) = line.strip_prefix("data:") {
                    self.data.push_str(data.strip_prefix(' ').unwrap_or(data));
                    self.data.push('\n');
                } else if line == "data" {
                    self.data.push('\n');
                }
            } else {
                self.line.push(byte);
            }
            if self.line.len() + self.data.len() > MAX_EVENT {
                return Err("服务返回的单个事件过大，已停止读取。".into());
            }
        }
        Ok(events)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_chunk_boundary_preserves_multilingual_and_multiline_events() {
        let wire =
            "\u{feff}: heartbeat\r\ndata: {\"text\":\r\ndata: \"你好 🌍\"}\r\n\r\ndata: [DONE]\n\n"
                .as_bytes();
        for split in 0..=wire.len() {
            let mut d = Decoder::default();
            let mut events = d.push(&wire[..split]).unwrap();
            events.extend(d.push(&wire[split..]).unwrap());
            assert_eq!(events, ["{\"text\":\n\"你好 🌍\"}", "[DONE]"]);
        }
    }
    #[test]
    fn invalid_or_unbounded_stream_does_not_grow_without_limit() {
        assert!(Decoder::default().push(&[0xff, b'\n']).is_err());
        assert!(Decoder::default().push(&vec![b'x'; MAX_EVENT + 1]).is_err());
        assert!(
            Decoder::default()
                .push(b"data: incomplete")
                .unwrap()
                .is_empty()
        );
    }
}
