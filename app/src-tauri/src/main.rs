#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod export;
mod recording;
mod worker;
use owon_core::{
    autoset,
    control::{Setting, SettingReply},
    generator, trigger, usb,
    waveform::{self, Frame},
};
use serde_json::Value;
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tauri::State;
use worker::{Control, Preview, Worker};

#[derive(Clone, Default)]
struct AppState(Arc<Mutex<Option<Worker>>>);

#[tauri::command]
async fn list_devices() -> Result<Vec<usb::DeviceInfo>, String> {
    tauri::async_runtime::spawn_blocking(usb::devices)
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn connect_device(
    state: State<'_, AppState>,
    bus: u8,
    address: u8,
) -> Result<String, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut slot = state.0.lock().map_err(|e| e.to_string())?;
        if slot.as_ref().is_some_and(|w| !w.finished()) {
            return Err("すでに接続しています".into());
        }
        if let Some(old) = slot.take() {
            old.close();
        }
        let (worker, identity) = Worker::start(bus, address)?;
        *slot = Some(worker);
        Ok(identity)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn disconnect_device(state: State<'_, AppState>) -> Result<(), String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let worker = state.0.lock().map_err(|e| e.to_string())?.take();
        if let Some(w) = worker {
            w.close();
        }
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}
fn send_control(state: &AppState, control: Control) -> Result<(), String> {
    state
        .0
        .lock()
        .map_err(|e| e.to_string())?
        .as_ref()
        .ok_or("未接続です")?
        .send(control)
}
#[tauri::command]
fn set_preview(state: State<'_, AppState>, paused: bool, interval_ms: u64) -> Result<(), String> {
    if !(17..=10000).contains(&interval_ms) {
        return Err("取得間隔は17〜10000 msです".into());
    }
    send_control(
        &state,
        Control::Preview {
            paused,
            interval_ms,
        },
    )
}
#[tauri::command]
fn acquire_once(state: State<'_, AppState>) -> Result<(), String> {
    send_control(&state, Control::Once)
}
#[tauri::command]
async fn set_device_setting(
    state: State<'_, AppState>,
    setting: Setting,
) -> Result<SettingReply, String> {
    owon_core::control::compile(&setting)?;
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let (tx, rx) = std::sync::mpsc::channel();
        send_control(&state, Control::Setting(setting, tx))?;
        rx.recv_timeout(std::time::Duration::from_secs(16))
            .map_err(|e| e.to_string())?
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn read_measurements(state: State<'_, AppState>) -> Result<Value, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let (tx, rx) = std::sync::mpsc::channel();
        send_control(&state, Control::Measurements(tx))?;
        rx.recv_timeout(std::time::Duration::from_secs(32))
            .map_err(|e| e.to_string())?
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn read_generator(state: State<'_, AppState>) -> Result<generator::Snapshot, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let (tx, rx) = std::sync::mpsc::channel();
        send_control(&state, Control::GeneratorRead(tx))?;
        rx.recv_timeout(std::time::Duration::from_secs(20))
            .map_err(|e| e.to_string())?
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn read_trigger(state: State<'_, AppState>) -> Result<trigger::Snapshot, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let (tx, rx) = std::sync::mpsc::channel();
        send_control(&state, Control::TriggerRead(tx))?;
        rx.recv_timeout(std::time::Duration::from_secs(18))
            .map_err(|e| e.to_string())?
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn read_autoset_settings(state: State<'_, AppState>) -> Result<autoset::Snapshot, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let (tx, rx) = std::sync::mpsc::channel();
        send_control(&state, Control::AutosetRead(tx))?;
        rx.recv_timeout(std::time::Duration::from_secs(32))
            .map_err(|e| e.to_string())?
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
fn autoset_available() -> bool {
    autoset::HARDWARE_VERIFIED
}
#[tauri::command]
async fn autoset_device(
    state: State<'_, AppState>,
    request: autoset::Request,
) -> Result<autoset::Reply, String> {
    autoset::validate(&request)?;
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let (tx, rx) = std::sync::mpsc::channel();
        send_control(&state, Control::Autoset(request, tx))?;
        rx.recv_timeout(std::time::Duration::from_secs(32))
            .map_err(|e| e.to_string())?
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn set_trigger_setting(
    state: State<'_, AppState>,
    setting: trigger::Setting,
) -> Result<trigger::Reply, String> {
    trigger::compile(&setting)?;
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let (tx, rx) = std::sync::mpsc::channel();
        send_control(&state, Control::TriggerSetting(setting, tx))?;
        rx.recv_timeout(std::time::Duration::from_secs(32))
            .map_err(|e| e.to_string())?
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn set_generator_setting(
    state: State<'_, AppState>,
    setting: generator::Setting,
) -> Result<generator::Reply, String> {
    generator::compile(&setting, &setting.expected)?;
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let (tx, rx) = std::sync::mpsc::channel();
        send_control(&state, Control::GeneratorSetting(setting, tx))?;
        rx.recv_timeout(std::time::Duration::from_secs(36))
            .map_err(|e| e.to_string())?
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn set_recording(
    state: State<'_, AppState>,
    folder: Option<PathBuf>,
    interval_ms: u64,
) -> Result<Option<String>, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let slot = state.0.lock().map_err(|e| e.to_string())?;
        let w = slot.as_ref().ok_or("未接続です")?;
        if let Some(folder) = folder {
            if w.finished() {
                return Err("接続が終了しています".into());
            }
            w.recorder.start(folder, interval_ms).map(Some)
        } else {
            w.recorder.stop()?;
            Ok(None)
        }
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn list_recordings(folder: PathBuf) -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut paths = Vec::new();
        for entry in std::fs::read_dir(folder).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            if entry.file_type().map_err(|e| e.to_string())?.is_file()
                && entry.path().extension().is_some_and(|ext| ext == "json")
            {
                paths.push(entry.path().to_string_lossy().into_owned());
            }
            if paths.len() > 10000 {
                return Err("再生フォルダーは10000ファイルまでです".into());
            }
        }
        paths.sort();
        Ok(paths)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
fn print_preview(
    webview: tauri::WebviewWindow,
    margins: [f64; 4],
    landscape: bool,
) -> Result<(), String> {
    if margins
        .iter()
        .any(|v| !v.is_finite() || !(0.0..=40.0).contains(v))
    {
        return Err("印刷余白は0〜40 mmです".into());
    }
    #[cfg(target_os = "macos")]
    {
        webview
            .with_webview(move |view| unsafe {
                use objc2_app_kit::{NSPaperOrientation, NSPrintInfo};
                use objc2_foundation::{NSCopying, NSSize};
                use objc2_web_kit::WKWebView;
                let view = &*view.inner().cast::<WKWebView>();
                // Copy rather than change the process-wide shared print settings.
                let info = NSPrintInfo::sharedPrintInfo().copy();
                let points = |mm: f64| mm * 72.0 / 25.4;
                info.setPaperSize(NSSize::new(points(210.0), points(297.0)));
                info.setTopMargin(points(margins[0]));
                info.setRightMargin(points(margins[1]));
                info.setBottomMargin(points(margins[2]));
                info.setLeftMargin(points(margins[3]));
                info.setOrientation(if landscape {
                    NSPaperOrientation::Landscape
                } else {
                    NSPaperOrientation::Portrait
                });
                let operation = view.printOperationWithPrintInfo(&info);
                // WKWebView may substitute its own print-info object during operation creation.
                let operation_info = operation.printInfo();
                operation_info.setPaperSize(NSSize::new(points(210.0), points(297.0)));
                operation_info.setOrientation(if landscape {
                    NSPaperOrientation::Landscape
                } else {
                    NSPaperOrientation::Portrait
                });
                operation_info.setTopMargin(points(margins[0]));
                operation_info.setRightMargin(points(margins[1]));
                operation_info.setBottomMargin(points(margins[2]));
                operation_info.setLeftMargin(points(margins[3]));
                operation.setCanSpawnSeparateThread(true);
                if let Some(window) = view.window() {
                    operation.runOperationModalForWindow_delegate_didRunSelector_contextInfo(
                        &window,
                        None,
                        None,
                        std::ptr::null_mut(),
                    );
                }
            })
            .map_err(|e| e.to_string())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = landscape;
        webview.print().map_err(|e| e.to_string())
    }
}
#[tauri::command]
fn poll_preview(state: State<'_, AppState>, after: u64) -> Result<Preview, String> {
    state
        .0
        .lock()
        .map_err(|e| e.to_string())?
        .as_ref()
        .ok_or("未接続です")?
        .preview(after)
}
#[tauri::command]
async fn capture_stopped(state: State<'_, AppState>) -> Result<Frame, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let (tx, rx) = std::sync::mpsc::channel();
        send_control(&state, Control::Capture(tx))?;
        rx.recv_timeout(std::time::Duration::from_secs(16))
            .map_err(|e| e.to_string())?
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn load_record(path: PathBuf) -> Result<Frame, String> {
    tauri::async_runtime::spawn_blocking(move || waveform::load(&path))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn save_record(path: PathBuf, record: Value) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || waveform::save(&path, record))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn export_csv(path: PathBuf, record: Value) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        use std::{fs::OpenOptions, io::Write};
        let frame = waveform::from_record(record)?;
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(path)
            .map_err(|e| format!("上書きせず保存します: {e}"))?;
        writeln!(
            file,
            "# uncalibrated signed screen bytes; index is not ADC sample index"
        )
        .map_err(|e| e.to_string())?;
        writeln!(file, "byte_index,CH1_raw,CH2_raw").map_err(|e| e.to_string())?;
        for i in 0..frame.values.values().map(Vec::len).max().unwrap_or(0) {
            let val = |name: &str| {
                frame
                    .values
                    .get(name)
                    .and_then(|v| v.get(i))
                    .map(ToString::to_string)
                    .unwrap_or_default()
            };
            writeln!(file, "{i},{},{}", val("CH1"), val("CH2")).map_err(|e| e.to_string())?;
        }
        file.sync_all().map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn save_png(path: PathBuf, bytes: Vec<u8>) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        use std::{fs::OpenOptions, io::Write};
        if bytes.len() > 5 * 1024 * 1024 || !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            return Err("PNGデータが不正です（最大5 MiB）".into());
        }
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(path)
            .map_err(|e| format!("上書きせず保存します: {e}"))?;
        file.write_all(&bytes)
            .and_then(|_| file.sync_all())
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn export_table(
    path: PathBuf,
    record: Value,
    channel: String,
    format: String,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        export::write_new(&path, &export::table(record, &channel, &format)?)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn export_image(path: PathBuf, bytes: Vec<u8>, format: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        export::write_new(&path, &export::image(&bytes, &format)?)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn save_xls(path: PathBuf, bytes: Vec<u8>) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        if !bytes.starts_with(&[0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1]) {
            return Err("Excel BIFF8/OLEデータではありません".into());
        }
        export::write_new(&path, &bytes)
    })
    .await
    .map_err(|e| e.to_string())?
}
fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            list_devices,
            connect_device,
            disconnect_device,
            set_preview,
            acquire_once,
            set_device_setting,
            read_measurements,
            read_generator,
            set_generator_setting,
            read_trigger,
            set_trigger_setting,
            read_autoset_settings,
            autoset_available,
            autoset_device,
            set_recording,
            list_recordings,
            print_preview,
            poll_preview,
            capture_stopped,
            load_record,
            save_record,
            export_csv,
            save_png,
            export_table,
            export_image,
            save_xls
        ])
        .build(tauri::generate_context!())
        .expect("Tauri setup failed")
        .run(|app, event| {
            if matches!(event, tauri::RunEvent::Exit) {
                use tauri::Manager;
                if let Ok(mut slot) = app.state::<AppState>().0.lock() {
                    if let Some(worker) = slot.take() {
                        worker.close();
                    }
                }
            }
        });
}
