//! Incremental Server-Sent Events decoder for streaming responses.

#[derive(Debug, Default, Clone, PartialEq)]
pub struct SseEvent {
    pub event: Option<String>,
    pub data: String,
}

#[derive(Default)]
pub struct SseDecoder {
    buffer: Vec<u8>,
    event: Option<String>,
    data: Vec<String>,
}

impl SseDecoder {
    /// Feeds raw bytes; returns every event completed by them.
    pub fn push(&mut self, bytes: &[u8]) -> Vec<SseEvent> {
        self.buffer.extend_from_slice(bytes);
        let mut out = Vec::new();
        while let Some(pos) = self.buffer.iter().position(|&b| b == b'\n') {
            let raw: Vec<u8> = self.buffer.drain(..=pos).collect();
            let line = String::from_utf8_lossy(&raw);
            self.handle_line(line.trim_end_matches(['\n', '\r']), &mut out);
        }
        out
    }

    /// Flushes whatever is left when the stream closes without a trailing blank line.
    pub fn finish(&mut self) -> Vec<SseEvent> {
        let mut out = Vec::new();
        if !self.buffer.is_empty() {
            let raw = std::mem::take(&mut self.buffer);
            let line = String::from_utf8_lossy(&raw).to_string();
            self.handle_line(line.trim_end_matches('\r'), &mut out);
        }
        self.dispatch(&mut out);
        out
    }

    fn handle_line(&mut self, line: &str, out: &mut Vec<SseEvent>) {
        if line.is_empty() {
            self.dispatch(out);
            return;
        }
        if line.starts_with(':') {
            return; // comment / keep-alive
        }
        let (field, value) = match line.split_once(':') {
            Some((field, value)) => (field, value.strip_prefix(' ').unwrap_or(value)),
            None => (line, ""),
        };
        match field {
            "data" => self.data.push(value.to_string()),
            "event" => self.event = Some(value.to_string()),
            _ => {}
        }
    }

    fn dispatch(&mut self, out: &mut Vec<SseEvent>) {
        if self.data.is_empty() {
            self.event = None;
            return;
        }
        out.push(SseEvent { event: self.event.take(), data: self.data.join("\n") });
        self.data.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handles_split_chunks_crlf_and_comments() {
        let mut d = SseDecoder::default();
        let mut events = d.push(b": ping\r\nevent: content_block_delta\r\ndata: {\"a\"");
        assert!(events.is_empty());
        events.extend(d.push(b":1}\r\n\r\ndata: [DONE]\n\n"));
        events.extend(d.push(b"data: tail"));
        events.extend(d.finish());
        assert_eq!(
            events,
            vec![
                SseEvent { event: Some("content_block_delta".into()), data: "{\"a\":1}".into() },
                SseEvent { event: None, data: "[DONE]".into() },
                SseEvent { event: None, data: "tail".into() },
            ]
        );
    }

    #[test]
    fn multibyte_split_across_chunks() {
        let mut d = SseDecoder::default();
        let text = "data: héllo\n\n".as_bytes();
        let mut events = d.push(&text[..8]);
        events.extend(d.push(&text[8..]));
        assert_eq!(events[0].data, "héllo");
    }
}
