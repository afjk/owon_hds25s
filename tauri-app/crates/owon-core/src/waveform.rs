use crate::{
    protocol::{field, Client, Transport, MAX_PAYLOAD},
    Result,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::Path,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

pub const CALIBRATION: &str = "sample_encoding_and_time_axis_not_yet_verified";
pub const STOPPED: &str = "manual_stop_observed; no acquisition_id_available";
pub const LIVE: &str = "live_sequential_queries; channel_acquisition_alignment_unverified";
pub const FILE_LIMIT: u64 = 5 * MAX_PAYLOAD as u64;

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Frame {
    pub record: Value,
    pub values: BTreeMap<String, Vec<i8>>,
    pub query_ms: f64,
    pub metadata_age_ms: f64,
    pub sequence: u64,
}

pub fn metadata(header: &Value) -> Result<BTreeMap<String, Value>> {
    let mut result = BTreeMap::new();
    for info in field(header, "CHANNEL")?
        .as_array()
        .ok_or("CHANNELが配列ではありません")?
    {
        let name = field(info, "NAME")?
            .as_str()
            .ok_or("CHANNEL NAMEが文字列ではありません")?
            .to_ascii_uppercase();
        if ["CH1", "CH2"].contains(&name.as_str())
            && result.insert(name.clone(), info.clone()).is_some()
        {
            return Err(format!("{name}が重複しています"));
        }
    }
    Ok(result)
}
fn is_on(info: &Value) -> bool {
    field(info, "DISPLAY")
        .ok()
        .and_then(Value::as_str)
        .is_some_and(|v| v.eq_ignore_ascii_case("ON"))
}
fn hex(raw: &[u8]) -> String {
    raw.iter().map(|b| format!("{b:02x}")).collect()
}
fn make_record(
    identity: &str,
    status: &str,
    header: Value,
    raw: &BTreeMap<String, Vec<u8>>,
    consistency: &str,
) -> Value {
    let channels: BTreeMap<_, _> = raw
        .iter()
        .map(|(name, bytes)| {
            (
                name.clone(),
                json!({"byte_count": bytes.len(), "payload_hex": hex(bytes)}),
            )
        })
        .collect();
    json!({"schema_version":1, "source":"owon_hds200_screen", "identity":identity, "status_before":status, "status_after":status,
        "header":header, "channels":channels, "consistency":consistency, "calibration":CALIBRATION,
        "retrieved_at_unix_ms":SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis(),
        "retrieved_at_note":"host retrieval time, not waveform event time"})
}
pub fn from_record(record: Value) -> Result<Frame> {
    if record.get("schema_version").and_then(Value::as_u64) != Some(1)
        || record.get("source").and_then(Value::as_str) != Some("owon_hds200_screen")
        || record.get("calibration").and_then(Value::as_str) != Some(CALIBRATION)
    {
        return Err("未対応の保存形式／校正形式です".into());
    }
    if record.get("identity").and_then(Value::as_str).is_none() {
        return Err("機種情報がありません".into());
    }
    let meta = metadata(record.get("header").ok_or("ヘッダーがありません")?)?;
    if let Some(roles) = record.get("sensor_assignments") {
        let obj = roles.as_object().ok_or("入力割り当てが不正です")?;
        let a = obj.get("CH1").and_then(Value::as_str);
        let b = obj.get("CH2").and_then(Value::as_str);
        if obj.len() != 2
            || !matches!(
                (a, b),
                (Some("audio"), Some("light")) | (Some("light"), Some("audio"))
            )
        {
            return Err("入力割り当てが不正です".into());
        }
    }
    let channels = record
        .get("channels")
        .and_then(Value::as_object)
        .ok_or("受信データがありません")?;
    if channels.len() > 2 || (channels.is_empty() && meta.values().any(is_on)) {
        return Err("CHデータ数が不正です".into());
    }
    let mut values = BTreeMap::new();
    for (name, entry) in channels {
        if !meta.get(name).is_some_and(is_on) {
            return Err(format!("{name}のデータとヘッダーが一致しません"));
        }
        let count = entry
            .get("byte_count")
            .and_then(Value::as_u64)
            .ok_or("バイト数が不正です")?;
        let encoded = entry
            .get("payload_hex")
            .and_then(Value::as_str)
            .ok_or("hexデータがありません")?;
        if count == 0
            || count > MAX_PAYLOAD as u64
            || encoded.len() != count as usize * 2
            || !encoded.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err("バイト数／hexデータが不正です".into());
        }
        let raw: Vec<i8> = encoded
            .as_bytes()
            .chunks_exact(2)
            .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap() as i8)
            .collect();
        values.insert(name.clone(), raw);
    }
    if meta
        .iter()
        .any(|(name, info)| is_on(info) && !values.contains_key(name))
    {
        return Err("ONのCHに受信データがありません".into());
    }
    let consistency = record
        .get("consistency")
        .and_then(Value::as_str)
        .ok_or("取得条件がありません")?;
    if consistency == STOPPED {
        if ["status_before", "status_after"].iter().any(|key| {
            !record
                .get(key)
                .and_then(Value::as_str)
                .is_some_and(|s| s.eq_ignore_ascii_case("STOP"))
        }) || values.len() != 2
            || values["CH1"].len() != values["CH2"].len()
        {
            return Err("停止保存の条件に矛盾があります".into());
        }
    } else if consistency != LIVE {
        return Err("未知の取得条件です".into());
    }
    Ok(Frame {
        record,
        values,
        query_ms: 0.0,
        metadata_age_ms: 0.0,
        sequence: 0,
    })
}

pub struct LiveReader {
    identity: String,
    header: Option<Value>,
    status: String,
    metadata_at: Instant,
    sequence: u64,
}
impl LiveReader {
    pub fn new(identity: String) -> Self {
        Self {
            identity,
            header: None,
            status: String::new(),
            metadata_at: Instant::now(),
            sequence: 0,
        }
    }
    pub fn invalidate(&mut self) {
        self.header = None;
    }
    pub fn read<T: Transport>(&mut self, client: &mut Client<T>) -> Result<Frame> {
        let started = Instant::now();
        if self.header.is_none() || self.metadata_at.elapsed().as_secs_f64() >= 1.0 {
            self.status = client.text(":TRIGGER:STATUS?")?;
            self.header = Some(client.header()?);
            self.metadata_at = Instant::now();
        }
        let header = self.header.as_ref().unwrap();
        let mut raw = BTreeMap::new();
        for (name, info) in metadata(header)? {
            if is_on(&info) {
                raw.insert(
                    name.clone(),
                    client.query(&format!(":DATA:WAVE:SCREEN:{name}?"))?,
                );
            }
        }
        let age = self.metadata_at.elapsed().as_secs_f64() * 1000.0;
        let mut record = make_record(&self.identity, &self.status, header.clone(), &raw, LIVE);
        record["metadata_age_ms"] = json!(age);
        let mut frame = from_record(record)?;
        self.sequence += 1;
        frame.sequence = self.sequence;
        frame.query_ms = started.elapsed().as_secs_f64() * 1000.0;
        frame.metadata_age_ms = age;
        Ok(frame)
    }
}
pub fn capture_stopped<T: Transport>(client: &mut Client<T>, identity: &str) -> Result<Frame> {
    let start = Instant::now();
    let status = client.text(":TRIGGER:STATUS?")?;
    if !status.eq_ignore_ascii_case("STOP") {
        return Err(
            "本体のRUN/STOPで停止してから再実行してください（アプリは本体を停止しません）".into(),
        );
    }
    let before = client.header()?;
    let meta = metadata(&before)?;
    if !["CH1", "CH2"]
        .iter()
        .all(|name| meta.get(*name).is_some_and(is_on))
    {
        return Err("本体でCH1とCH2を両方ONにしてください".into());
    }
    let mut raw = BTreeMap::new();
    for name in ["CH1", "CH2"] {
        raw.insert(
            name.to_string(),
            client.query(&format!(":DATA:WAVE:SCREEN:{name}?"))?,
        );
    }
    let after = client.header()?;
    if !client
        .text(":TRIGGER:STATUS?")?
        .eq_ignore_ascii_case("STOP")
        || before != after
    {
        return Err("取得中に本体の状態／設定が変わりました".into());
    }
    let mut frame = from_record(make_record(identity, "STOP", before, &raw, STOPPED))?;
    frame.query_ms = start.elapsed().as_secs_f64() * 1000.0;
    Ok(frame)
}
pub fn load(path: &Path) -> Result<Frame> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    if file.metadata().map_err(|e| e.to_string())?.len() > FILE_LIMIT {
        return Err("ファイル上限は5 MiBです".into());
    }
    let mut bytes = Vec::new();
    file.take(FILE_LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > FILE_LIMIT {
        return Err("ファイル上限は5 MiBです".into());
    }
    from_record(serde_json::from_slice(&bytes).map_err(|e| e.to_string())?)
}
pub fn save(path: &Path, record: Value) -> Result<()> {
    let frame = from_record(record)?;
    let bytes = serde_json::to_vec_pretty(&frame.record).map_err(|e| e.to_string())?;
    if bytes.len() as u64 > FILE_LIMIT {
        return Err("ファイル上限は5 MiBです".into());
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| format!("保存できません（既存ファイルは上書きしません）: {e}"))?;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|e| format!("保存に失敗しました。作成途中のファイルを確認してください: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Value {
        serde_json::from_str(include_str!(
            "../tests/fixtures/synthetic-python-capture.json"
        ))
        .unwrap()
    }
    #[test]
    fn loads_existing_python_capture() {
        let f = from_record(fixture()).unwrap();
        assert_eq!(f.values["CH1"].len(), 600);
        assert_eq!(f.values["CH1"][0], 89);
    }
    #[test]
    fn rejects_inconsistent_stopped_status() {
        let mut r = fixture();
        r["status_after"] = json!("AUTO");
        assert!(from_record(r).is_err());
    }
    #[test]
    fn rejects_odd_hex_and_count() {
        let mut r = fixture();
        r["channels"]["CH1"]["payload_hex"] = json!("a");
        assert!(from_record(r).is_err());
    }
    #[test]
    fn rejects_invalid_roles() {
        let mut r = fixture();
        r["sensor_assignments"]["CH1"] = json!("light");
        assert!(from_record(r).is_err());
    }
    #[test]
    fn rejects_unknown_channel() {
        let mut r = fixture();
        r["channels"]["CH3"] = r["channels"]["CH1"].clone();
        assert!(from_record(r).is_err());
    }
    #[test]
    fn rejects_disabled_channel() {
        let mut r = fixture();
        r["header"]["CHANNEL"][0]["DISPLAY"] = json!("OFF");
        assert!(from_record(r).is_err());
    }
    #[test]
    fn accepts_live_empty_frame_only_when_both_channels_off() {
        let mut r = fixture();
        r["consistency"] = json!(LIVE);
        r["channels"] = json!({});
        assert!(from_record(r.clone()).is_err());
        r["header"]["CHANNEL"][0]["DISPLAY"] = json!("OFF");
        r["header"]["CHANNEL"][1]["DISPLAY"] = json!("OFF");
        assert!(from_record(r).unwrap().values.is_empty());
    }
    struct Fake {
        responses: std::collections::VecDeque<Vec<u8>>,
        commands: Vec<String>,
    }
    impl Transport for Fake {
        fn write(&mut self, data: &[u8], _: std::time::Duration) -> Result<()> {
            self.commands
                .push(String::from_utf8_lossy(data).trim().into());
            Ok(())
        }
        fn read(&mut self, _: std::time::Duration) -> Result<Vec<u8>> {
            self.responses.pop_front().ok_or("timeout".into())
        }
    }
    fn framed(bytes: &[u8]) -> Vec<u8> {
        let mut result = (bytes.len() as u32).to_le_bytes().to_vec();
        result.extend(bytes);
        result
    }
    fn fake_stop(status_after: &[u8], change_header: bool) -> Client<Fake> {
        let before = fixture()["header"].clone();
        let mut after = before.clone();
        if change_header {
            after["TIMEBASE"]["SCALE"] = json!("2ms");
        }
        Client::new(Fake {
            responses: [
                b"STOP\n".to_vec(),
                framed(&serde_json::to_vec(&before).unwrap()),
                framed(&[1, 2]),
                framed(&[3, 4]),
                framed(&serde_json::to_vec(&after).unwrap()),
                status_after.to_vec(),
            ]
            .into(),
            commands: vec![],
        })
    }
    #[test]
    fn stopped_capture_checks_both_ends() {
        let mut c = fake_stop(b"STOP\n", false);
        let f = capture_stopped(&mut c, "OWON,HDS25S").unwrap();
        assert_eq!(f.record["consistency"], STOPPED);
        assert_eq!(f.values["CH2"], [3, 4]);
    }
    #[test]
    fn running_device_cannot_be_captured() {
        let mut c = Client::new(Fake {
            responses: [b"AUTO\n".to_vec()].into(),
            commands: vec![],
        });
        assert!(capture_stopped(&mut c, "HDS25S").is_err());
        assert!(!c.is_failed());
    }
    #[test]
    fn acquisition_state_change_rejected() {
        assert!(capture_stopped(&mut fake_stop(b"AUTO\n", false), "HDS25S").is_err());
    }
    #[test]
    fn header_change_rejected() {
        assert!(capture_stopped(&mut fake_stop(b"STOP\n", true), "HDS25S").is_err());
    }
    #[test]
    fn live_metadata_is_cached() {
        let head = framed(&serde_json::to_vec(&fixture()["header"]).unwrap());
        let mut client = Client::new(Fake {
            responses: [
                b"AUTO\n".to_vec(),
                head,
                framed(&[1, 2]),
                framed(&[3, 4]),
                framed(&[5, 6]),
                framed(&[7, 8]),
            ]
            .into(),
            commands: vec![],
        });
        let mut reader = LiveReader::new("HDS25S".into());
        assert_eq!(reader.read(&mut client).unwrap().sequence, 1);
        let f = reader.read(&mut client).unwrap();
        assert_eq!(f.sequence, 2);
        assert_eq!(f.values["CH1"], [5, 6]);
        assert_eq!(f.record["consistency"], LIVE);
        assert!(f.record.get("sensor_assignments").is_none());
    }
    #[test]
    fn save_never_overwrites() {
        let path = std::env::temp_dir().join(format!(
            "owon-core-save-{}-{}.json",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        save(&path, fixture()).unwrap();
        assert!(save(&path, fixture()).is_err());
        assert_eq!(load(&path).unwrap().values["CH2"].len(), 600);
        std::fs::remove_file(path).unwrap();
    }
}
