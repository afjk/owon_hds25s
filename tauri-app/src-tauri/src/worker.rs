use owon_core::{
    autoset,
    control::{self, Setting, SettingReply},
    generator,
    protocol::Client,
    trigger,
    usb::BulkTransport,
    waveform::{capture_stopped, Frame, LiveReader},
};
use serde::Serialize;
use std::{
    sync::{
        mpsc::{self, Receiver, Sender},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

pub enum Control {
    Preview { paused: bool, interval_ms: u64 },
    Once,
    Capture(Sender<Result<Frame, String>>),
    Setting(Setting, Sender<Result<SettingReply, String>>),
    Measurements(Sender<Result<serde_json::Value, String>>),
    GeneratorRead(Sender<Result<generator::Snapshot, String>>),
    GeneratorSetting(generator::Setting, Sender<Result<generator::Reply, String>>),
    TriggerRead(Sender<Result<trigger::Snapshot, String>>),
    TriggerSetting(trigger::Setting, Sender<Result<trigger::Reply, String>>),
    AutosetRead(Sender<Result<autoset::Snapshot, String>>),
    Autoset(autoset::Request, Sender<Result<autoset::Reply, String>>),
    Shutdown,
}
#[derive(Default, Clone, Serialize)]
pub struct Preview {
    pub frame: Option<Frame>,
    pub paused: bool,
    pub error: Option<String>,
    pub recording: crate::recording::Status,
}
pub struct Worker {
    sender: Sender<Control>,
    join: JoinHandle<()>,
    latest: Arc<Mutex<Preview>>,
    pub recorder: crate::recording::Recorder,
}
impl Worker {
    pub fn start(bus: u8, address: u8) -> Result<(Self, String), String> {
        let (sender, receiver) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::channel();
        let latest = Arc::new(Mutex::new(Preview::default()));
        let worker_latest = latest.clone();
        let join = thread::Builder::new()
            .name("owon-usb".into())
            .spawn(move || {
                // One thread owns creation, claim, every query, release and drop.
                let result =
                    BulkTransport::open(bus, address)
                        .map(Client::new)
                        .and_then(|mut client| {
                            let identity = client.text("*IDN?")?;
                            if !identity.to_ascii_uppercase().contains("HDS") {
                                return Err("HDSシリーズと識別できません".into());
                            }
                            Ok((client, identity))
                        });
                match result {
                    Ok((client, identity)) => {
                        let _ = ready_tx.send(Ok(identity.clone()));
                        run(client, identity, receiver, worker_latest);
                    }
                    Err(message) => {
                        let _ = ready_tx.send(Err(message));
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        match ready_rx.recv().map_err(|e| e.to_string())? {
            Ok(identity) => {
                let record_source = latest.clone();
                let recorder = crate::recording::Recorder::new(move || {
                    let slot = record_source.lock().map_err(|e| e.to_string())?;
                    if let Some(e) = &slot.error {
                        return Err(e.clone());
                    }
                    Ok(if slot.paused {
                        None
                    } else {
                        slot.frame.clone()
                    })
                });
                Ok((
                    Self {
                        sender,
                        join,
                        latest,
                        recorder,
                    },
                    identity,
                ))
            }
            Err(message) => {
                let _ = join.join();
                Err(message)
            }
        }
    }
    pub fn finished(&self) -> bool {
        self.join.is_finished()
    }
    pub fn send(&self, control: Control) -> Result<(), String> {
        self.sender
            .send(control)
            .map_err(|_| "USB接続が終了しました".into())
    }
    pub fn preview(&self, after: u64) -> Result<Preview, String> {
        let mut snapshot = self.latest.lock().map_err(|e| e.to_string())?.clone();
        snapshot.recording = self.recorder.status()?;
        if snapshot.frame.as_ref().is_some_and(|f| f.sequence <= after) {
            snapshot.frame = None;
        }
        Ok(snapshot)
    }
    pub fn close(self) {
        let _ = self.recorder.stop();
        let _ = self.sender.send(Control::Shutdown);
        let _ = self.join.join();
    }
}
fn run(
    mut client: Client<BulkTransport>,
    identity: String,
    receiver: Receiver<Control>,
    latest: Arc<Mutex<Preview>>,
) {
    let mut reader = LiveReader::new(identity.clone());
    let mut paused = false;
    let mut interval = Duration::from_millis(17);
    let mut next = Instant::now();
    let mut once = false;
    loop {
        let wait = if paused && !once {
            Duration::from_millis(100)
        } else {
            next.saturating_duration_since(Instant::now())
                .min(Duration::from_millis(100))
        };
        match receiver.recv_timeout(wait) {
            Ok(Control::Shutdown) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Ok(Control::Preview {
                paused: p,
                interval_ms,
            }) => {
                paused = p;
                interval = Duration::from_millis(interval_ms);
                reader.invalidate();
                next = Instant::now();
                if let Ok(mut slot) = latest.lock() {
                    slot.paused = p
                }
            }
            Ok(Control::Once) => {
                reader.invalidate();
                once = true;
                next = Instant::now()
            }
            Ok(Control::Capture(reply)) => {
                paused = true;
                once = false;
                if let Ok(mut slot) = latest.lock() {
                    slot.paused = true
                }
                let result = capture_stopped(&mut client, &identity);
                reader.invalidate();
                let _ = reply.send(result);
                if client.is_failed() {
                    if let Ok(mut slot) = latest.lock() {
                        slot.error =
                            Some("停止取得中に通信が失敗しました。再接続してください".into())
                    }
                    break;
                }
            }
            Ok(Control::Setting(setting, reply)) => {
                let result = if setting.target == "trigger" {
                    Err(
                        "トリガーは現在値を読み、専用のset_trigger_setting APIで適用してください"
                            .into(),
                    )
                } else {
                    control::apply(&mut client, &setting)
                };
                reader.invalidate();
                let _ = reply.send(result);
                once = true;
                next = Instant::now();
            }
            Ok(Control::GeneratorRead(reply)) => {
                let result =
                    generator::ensure_model(&identity).and_then(|_| generator::read(&mut client));
                let _ = reply.send(result);
            }
            Ok(Control::TriggerRead(reply)) => {
                let result =
                    trigger::ensure_model(&identity).and_then(|_| trigger::read(&mut client));
                let _ = reply.send(result);
            }
            Ok(Control::TriggerSetting(setting, reply)) => {
                let result = trigger::ensure_model(&identity)
                    .and_then(|_| trigger::apply(&mut client, &setting));
                reader.invalidate();
                let _ = reply.send(result);
                once = true;
                next = Instant::now();
            }
            Ok(Control::AutosetRead(reply)) => {
                let result =
                    trigger::ensure_model(&identity).and_then(|_| autoset::read(&mut client));
                let _ = reply.send(result);
            }
            Ok(Control::Autoset(request, reply)) => {
                let result = autoset::ensure_available(&identity)
                    .and_then(|_| autoset::apply(&mut client, &request));
                reader.invalidate();
                let _ = reply.send(result);
                once = true;
                next = Instant::now();
            }
            Ok(Control::GeneratorSetting(setting, reply)) => {
                let result = generator::ensure_model(&identity)
                    .and_then(|_| generator::apply(&mut client, &setting));
                reader.invalidate();
                let _ = reply.send(result);
                once = true;
                next = Instant::now();
            }
            Ok(Control::Measurements(reply)) => {
                let result = (|| {
                    let mut values = serde_json::Map::new();
                    for ch in ["CH1", "CH2"] {
                        let mut measured = serde_json::Map::new();
                        for item in ["FREQUENCY", "PERIOD", "PKPK", "MAX", "MIN", "AVERAGE"] {
                            measured.insert(
                                item.into(),
                                serde_json::Value::String(
                                    client.text(&format!(":MEASUREMENT:{ch}:{item}?"))?,
                                ),
                            );
                        }
                        values.insert(ch.into(), measured.into());
                    }
                    Ok(
                        serde_json::json!({"values": values, "read_at_unix_ms": std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis()}),
                    )
                })();
                let _ = reply.send(result);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        if client.is_failed() {
            if let Ok(mut slot) = latest.lock() {
                slot.error = Some("本体操作中の通信が失敗しました。再接続してください".into());
            }
            break;
        }
        if (paused && !once) || Instant::now() < next {
            continue;
        }
        let started = Instant::now();
        once = false;
        match reader.read(&mut client) {
            Ok(frame) => {
                if let Ok(mut slot) = latest.lock() {
                    slot.frame = Some(frame)
                }
            }
            Err(message) => {
                if let Ok(mut slot) = latest.lock() {
                    slot.error = Some(message)
                }
                break;
            }
        }
        next = started + interval;
    }
}
