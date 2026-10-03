use crate::Result;
use serde_json::Value;
use std::time::{Duration, Instant};

pub const MAX_PAYLOAD: usize = 1024 * 1024;
pub trait Transport: Send {
    fn write(&mut self, data: &[u8], timeout: Duration) -> Result<()>;
    fn read(&mut self, timeout: Duration) -> Result<Vec<u8>>;
}

pub struct Client<T: Transport> {
    transport: T,
    failed: bool,
    timeout: Duration,
}
impl<T: Transport> Client<T> {
    pub fn new(transport: T) -> Self {
        Self {
            transport,
            failed: false,
            timeout: Duration::from_secs(2),
        }
    }
    pub fn is_failed(&self) -> bool {
        self.failed
    }
    pub fn query(&mut self, command: &str) -> Result<Vec<u8>> {
        let command = command.to_ascii_uppercase();
        let framed = match command.as_str() {
            "*IDN?" | ":TRIGGER:STATUS?" => false,
            ":DATA:WAVE:SCREEN:HEAD?" | ":DATA:WAVE:SCREEN:CH1?" | ":DATA:WAVE:SCREEN:CH2?" => true,
            _ if crate::control::query_allowed(&command) => false,
            _ if crate::generator::query_allowed(&command) => false,
            _ => return Err("既知の読取専用コマンド以外は許可されません".into()),
        };
        if self.failed {
            return Err("通信セッションが無効です。USBを再接続してください".into());
        }
        let result = self.exchange(&command, framed);
        if result.is_err() {
            self.failed = true;
        }
        result
    }
    pub fn write_setting(&mut self, command: &crate::control::Command) -> Result<()> {
        if self.failed {
            return Err("通信セッションが無効です。USBを再接続してください".into());
        }
        // Command fields are checked again, so callers cannot bypass the typed compiler.
        let (path, value) = command
            .set
            .split_once(' ')
            .ok_or("設定コマンドが不正です")?;
        if command.query != format!("{path}?")
            || !crate::control::query_allowed(&command.query)
            || value.contains([';', '\n', '\r'])
            || path.starts_with(":MEASUREMENT")
        {
            return Err("設定コマンドが不正です".into());
        }
        let result = self
            .transport
            .write(format!("{}\n", command.set).as_bytes(), self.timeout);
        if result.is_err() {
            self.failed = true;
        }
        result
    }
    pub fn write_generator(&mut self, command: &crate::generator::Command) -> Result<()> {
        if self.failed {
            return Err("通信セッションが無効です。USBを再接続してください".into());
        }
        crate::generator::validate_command(command)?;
        let result = self
            .transport
            .write(format!("{}\n", command.set).as_bytes(), self.timeout);
        if result.is_err() {
            self.failed = true;
        }
        result
    }
    pub(crate) fn write_autoset(&mut self) -> Result<()> {
        if self.failed {
            return Err("通信セッションが無効です。USBを再接続してください".into());
        }
        let result = self.transport.write(b":AUT .\n", self.timeout);
        if result.is_err() {
            self.failed = true;
        }
        result
    }
    fn exchange(&mut self, command: &str, framed: bool) -> Result<Vec<u8>> {
        let deadline = Instant::now() + self.timeout;
        self.transport
            .write(format!("{command}\n").as_bytes(), self.timeout)?;
        let mut data = Vec::new();
        loop {
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .ok_or("応答タイムアウト")?;
            let chunk = self.transport.read(remaining)?;
            if chunk.is_empty() {
                return Err("空のUSB応答".into());
            }
            data.extend(chunk);
            if let Some(payload) = decode_response(&data, framed)? {
                return Ok(payload);
            }
        }
    }
    pub fn text(&mut self, command: &str) -> Result<String> {
        let raw = self.query(command)?;
        if !raw.is_ascii() {
            self.failed = true;
            return Err("非ASCIIのテキスト応答".into());
        }
        Ok(String::from_utf8(raw)
            .map_err(|e| e.to_string())?
            .trim()
            .to_string())
    }
    pub fn header(&mut self) -> Result<Value> {
        let raw = self.query(":DATA:WAVE:SCREEN:HEAD?")?;
        let result = serde_json::from_slice::<Value>(&raw)
            .map_err(|e| e.to_string())
            .and_then(|head| {
                if head.is_object() {
                    Ok(head)
                } else {
                    Err("ヘッダーがJSONオブジェクトではありません".into())
                }
            });
        if result.is_err() {
            self.failed = true;
        }
        result
    }
}

pub fn decode_response(data: &[u8], framed: bool) -> Result<Option<Vec<u8>>> {
    if data.len() > MAX_PAYLOAD + 6 {
        return Err("応答がサイズ上限を超えています".into());
    }
    if framed {
        if data.len() < 4 {
            return Ok(None);
        }
        let size = u32::from_le_bytes(data[..4].try_into().unwrap()) as usize;
        if size == 0 || size > MAX_PAYLOAD {
            return Err(format!("不正なlittle-endian長: {size}"));
        }
        if data.len() < size + 4 {
            return Ok(None);
        }
        let tail = &data[size + 4..];
        if ![&b""[..], &b"\n"[..], &b"\r\n"[..]].contains(&tail) {
            return Err("バイナリ応答の末尾に未知のデータ".into());
        }
        Ok(Some(data[4..size + 4].to_vec()))
    } else {
        if let Some(pos) = data.iter().position(|b| *b == b'\n') {
            if pos != data.len() - 1 {
                return Err("テキスト応答の末尾に未知のデータ".into());
            }
            let body = &data[..pos];
            Ok(Some(body.strip_suffix(b"\r").unwrap_or(body).to_vec()))
        } else if data.len() > 4096 {
            Err("テキスト応答が長すぎます".into())
        } else {
            Ok(None)
        }
    }
}

pub fn field<'a>(value: &'a Value, name: &str) -> Result<&'a Value> {
    value
        .as_object()
        .and_then(|obj| obj.iter().find(|(key, _)| key.eq_ignore_ascii_case(name)))
        .map(|(_, v)| v)
        .ok_or_else(|| format!("ヘッダーに{name}がありません"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    struct Fake {
        chunks: VecDeque<Vec<u8>>,
        writes: usize,
    }
    impl Transport for Fake {
        fn write(&mut self, _: &[u8], _: Duration) -> Result<()> {
            self.writes += 1;
            Ok(())
        }
        fn read(&mut self, _: Duration) -> Result<Vec<u8>> {
            self.chunks.pop_front().ok_or("timeout".into())
        }
    }
    #[test]
    fn fragmented_binary_preserves_newlines() {
        let fake = Fake {
            chunks: [vec![3, 0], vec![0, 0, 10], vec![13, 255, 13, 10]].into(),
            writes: 0,
        };
        assert_eq!(
            Client::new(fake).query(":data:wave:screen:ch1?").unwrap(),
            [10, 13, 255]
        );
    }
    #[test]
    fn zero_and_oversized_lengths_rejected() {
        assert!(decode_response(&[0, 0, 0, 0], true).is_err());
        assert!(decode_response(&u32::MAX.to_le_bytes(), true).is_err());
    }
    #[test]
    fn unexpected_tail_rejected() {
        assert!(decode_response(&[1, 0, 0, 0, 42, 43], true).is_err());
    }
    #[test]
    fn text_crlf() {
        assert_eq!(
            decode_response(b"STOP\r\n", false).unwrap().unwrap(),
            b"STOP"
        );
    }
    #[test]
    fn extra_text_rejected() {
        assert!(decode_response(b"STOP\nMORE", false).is_err());
    }
    #[test]
    fn rejected_commands_do_not_write() {
        let mut client = Client::new(Fake {
            chunks: VecDeque::new(),
            writes: 0,
        });
        for command in [":RUN", "*IDN?;:STOP", "*IDN?\n:STOP"] {
            assert!(client.query(command).is_err());
        }
        assert_eq!(client.transport.writes, 0);
    }
    #[test]
    fn failed_session_never_retries() {
        let mut client = Client::new(Fake {
            chunks: [vec![1, 0]].into(),
            writes: 0,
        });
        assert!(client.query(":DATA:WAVE:SCREEN:CH1?").is_err());
        assert!(client.query("*IDN?").is_err());
        assert_eq!(client.transport.writes, 1);
    }
    #[test]
    fn header_keys_case_insensitive() {
        assert_eq!(
            field(&serde_json::json!({"TimeBase":42}), "TIMEBASE").unwrap(),
            42
        );
    }
}
