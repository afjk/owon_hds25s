//! Native recording: independent of JavaScript timers. One writer, no duplicate frames, no overwrite.
use owon_core::waveform::{self, Frame};
use serde::Serialize;
use std::{
    path::PathBuf,
    sync::{
        mpsc::{self, Sender},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Default, Serialize)]
pub struct Status {
    pub active: bool,
    pub saved: u64,
    pub skipped: u64,
    pub directory: Option<String>,
    pub error: Option<String>,
}
enum Control {
    Start(PathBuf, u64, Sender<Result<String, String>>),
    Stop(Sender<()>),
    Exit,
}
pub struct Recorder {
    sender: Sender<Control>,
    join: Option<JoinHandle<()>>,
    status: Arc<Mutex<Status>>,
}
impl Recorder {
    pub fn new(source: impl Fn() -> Result<Option<Frame>, String> + Send + 'static) -> Self {
        let (sender, receiver) = mpsc::channel();
        let status = Arc::new(Mutex::new(Status::default()));
        let shared = status.clone();
        let join = thread::spawn(move || {
            let mut directory: Option<PathBuf> = None;
            let mut interval = Duration::from_secs(1);
            let mut next = Instant::now();
            let mut previous = 0;
            loop {
                let wait = if directory.is_some() {
                    next.saturating_duration_since(Instant::now())
                } else {
                    Duration::from_millis(100)
                };
                match receiver.recv_timeout(wait) {
                    Ok(Control::Exit) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    Ok(Control::Stop(reply)) => {
                        directory = None;
                        shared.lock().unwrap().active = false;
                        let _ = reply.send(());
                    }
                    Ok(Control::Start(root, ms, reply)) => {
                        let result = (|| {
                            if !(500..=60000).contains(&ms) || !root.is_dir() {
                                return Err(
                                    "保存先フォルダーと間隔（500〜60000 ms）を確認してください"
                                        .into(),
                                );
                            }
                            let stamp = SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_nanos();
                            let folder = root.join(format!("owon-{stamp}"));
                            std::fs::create_dir(&folder).map_err(|e| e.to_string())?;
                            let path = folder.to_string_lossy().into_owned();
                            *shared.lock().unwrap() = Status {
                                active: true,
                                directory: Some(path.clone()),
                                ..Status::default()
                            };
                            directory = Some(folder);
                            interval = Duration::from_millis(ms);
                            next = Instant::now() + interval;
                            previous = 0;
                            Ok(path)
                        })();
                        let _ = reply.send(result);
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                }
                let Some(folder) = &directory else { continue };
                if Instant::now() < next {
                    continue;
                }
                next = Instant::now() + interval;
                let result = source().and_then(|frame| {
                    let Some(frame) = frame.filter(|f| f.sequence > previous) else {
                        shared.lock().unwrap().skipped += 1;
                        return Ok(());
                    };
                    let count = shared.lock().unwrap().saved + 1;
                    let path = folder.join(format!("wave-{count:06}.json"));
                    waveform::save(&path, frame.record)?;
                    previous = frame.sequence;
                    shared.lock().unwrap().saved = count;
                    Ok(())
                });
                if let Err(message) = result {
                    directory = None;
                    let mut s = shared.lock().unwrap();
                    s.active = false;
                    s.error = Some(message);
                }
            }
            shared.lock().unwrap().active = false;
        });
        Self {
            sender,
            join: Some(join),
            status,
        }
    }
    pub fn start(&self, folder: PathBuf, interval_ms: u64) -> Result<String, String> {
        let (tx, rx) = mpsc::channel();
        self.sender
            .send(Control::Start(folder, interval_ms, tx))
            .map_err(|e| e.to_string())?;
        rx.recv_timeout(Duration::from_secs(10))
            .map_err(|e| e.to_string())?
    }
    pub fn stop(&self) -> Result<(), String> {
        let (tx, rx) = mpsc::channel();
        self.sender
            .send(Control::Stop(tx))
            .map_err(|e| e.to_string())?;
        rx.recv_timeout(Duration::from_secs(10))
            .map_err(|e| e.to_string())
    }
    pub fn status(&self) -> Result<Status, String> {
        Ok(self.status.lock().map_err(|e| e.to_string())?.clone())
    }
}
impl Drop for Recorder {
    fn drop(&mut self) {
        let _ = self.sender.send(Control::Exit);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn saves_once_without_frontend_and_stops_without_overwriting() {
        let folder = std::env::temp_dir().join(format!(
            "owon-record-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&folder).unwrap();
        let mut f = waveform::from_record(
            serde_json::from_str(include_str!(
                "../../crates/owon-core/tests/fixtures/synthetic-python-capture.json"
            ))
            .unwrap(),
        )
        .unwrap();
        f.sequence = 1;
        let rec = Recorder::new(move || Ok(Some(f.clone())));
        assert!(rec.start(folder.clone(), 499).is_err());
        let session = PathBuf::from(rec.start(folder.clone(), 500).unwrap());
        thread::sleep(Duration::from_millis(1200));
        rec.stop().unwrap();
        assert_eq!(rec.status().unwrap().saved, 1);
        assert!(rec.status().unwrap().skipped >= 1);
        assert!(waveform::load(&session.join("wave-000001.json")).is_ok());
        drop(rec);
        std::fs::remove_file(session.join("wave-000001.json")).unwrap();
        std::fs::remove_dir(session).unwrap();
        std::fs::remove_dir(folder).unwrap();
    }
}
