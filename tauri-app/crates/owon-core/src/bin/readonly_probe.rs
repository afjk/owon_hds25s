//! Bounded read-only hardware diagnostics. No arbitrary SCPI, settings, reset,
//! output enable, driver detach or changes to the application's allowlist.
use owon_core::{
    protocol::{decode_response, Transport},
    usb::{self, BulkTransport},
};
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Copy)]
struct Trial {
    name: &'static str,
    command: &'static str,
    ending: &'static str,
}
const GENERATOR: &[Trial] = &[
    Trial {
        name: "waveform",
        command: ":FUNCTION?",
        ending: "\n",
    },
    Trial {
        name: "frequency",
        command: ":FUNCTION:FREQUENCY?",
        ending: "\n",
    },
    Trial {
        name: "period",
        command: ":FUNCTION:PERIOD?",
        ending: "\n",
    },
    Trial {
        name: "amplitude",
        command: ":FUNCTION:AMPLITUDE?",
        ending: "\n",
    },
    Trial {
        name: "offset",
        command: ":FUNCTION:OFFSET?",
        ending: "\n",
    },
    Trial {
        name: "output_state_candidate",
        command: ":CHANNEL?",
        ending: "\n",
    },
    Trial {
        name: "load",
        command: ":FUNCTION:LOAD?",
        ending: "\n",
    },
];
const TRIGGER: &[Trial] = &[
    Trial {
        name: "source_full",
        command: ":TRIGGER:SINGLE:SOURCE?",
        ending: "\n",
    },
    Trial {
        name: "source_mixed_case",
        command: ":TRIGger:SINGle:SOURce?",
        ending: "\n",
    },
    Trial {
        name: "source_short",
        command: ":TRIG:SING:SOUR?",
        ending: "\n",
    },
    Trial {
        name: "source_crlf",
        command: ":TRIGGER:SINGLE:SOURCE?",
        ending: "\r\n",
    },
    Trial {
        name: "coupling",
        command: ":TRIGGER:SINGLE:COUPLING?",
        ending: "\n",
    },
    Trial {
        name: "edge",
        command: ":TRIGGER:SINGLE:EDGE?",
        ending: "\n",
    },
    Trial {
        name: "slope_example",
        command: ":TRIGGER:SINGLE:SLOPE?",
        ending: "\n",
    },
    Trial {
        name: "level",
        command: ":TRIGGER:SINGLE:EDGE:LEVEL?",
        ending: "\n",
    },
    Trial {
        name: "sweep",
        command: ":TRIGGER:SINGLE:SWEEP?",
        ending: "\n",
    },
    Trial {
        name: "edge_double_colon_in_pdf",
        command: ":TRIGGER:SINGLE::EDGE?",
        ending: "\n",
    },
    Trial {
        name: "level_double_colon_in_pdf",
        command: ":TRIGGER:SINGLE::EDGE:LEVEL?",
        ending: "\n",
    },
];
// Bounded diagnostic hypotheses, not promoted to supported application APIs.
// EDGE:SOURCE appears in a SCPI syntax introduction; other nesting variants
// test whether this firmware uses a different subsystem hierarchy.
const TRIGGER_PATHS: &[Trial] = &[
    Trial {
        name: "source_under_edge",
        command: ":TRIGGER:SINGLE:EDGE:SOURCE?",
        ending: "\n",
    },
    Trial {
        name: "coupling_under_edge",
        command: ":TRIGGER:SINGLE:EDGE:COUPLING?",
        ending: "\n",
    },
    Trial {
        name: "sweep_under_edge",
        command: ":TRIGGER:SINGLE:EDGE:SWEEP?",
        ending: "\n",
    },
    Trial {
        name: "source_without_single",
        command: ":TRIGGER:EDGE:SOURCE?",
        ending: "\n",
    },
    Trial {
        name: "coupling_without_single",
        command: ":TRIGGER:EDGE:COUPLING?",
        ending: "\n",
    },
    Trial {
        name: "source_root",
        command: ":TRIGGER:SOURCE?",
        ending: "\n",
    },
    Trial {
        name: "coupling_root",
        command: ":TRIGGER:COUPLING?",
        ending: "\n",
    },
    Trial {
        name: "sweep_root",
        command: ":TRIGGER:SWEEP?",
        ending: "\n",
    },
    Trial {
        name: "mode_from_official_app_string",
        command: ":TRIGGER:SINGLE:MODE?",
        ending: "\n",
    },
];
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn observe<T: Transport>(transport: &mut T, command: &str, ending: &str, framed: bool) -> Value {
    // Fixed candidates only; this guard also protects all diagnostic baselines.
    assert!(command.ends_with('?') && !command.contains([';', '\n', '\r', ' ']));
    assert!(matches!(ending, "\n" | "\r\n"));
    let wire = format!("{command}{ending}");
    let started = Instant::now();
    let mut raw = Vec::new();
    let mut chunks = Vec::new();
    let mut payload = None;
    let mut error = None;
    if let Err(e) = transport.write(wire.as_bytes(), Duration::from_secs(2)) {
        error = Some(e);
    } else {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                error = Some("response deadline exceeded".into());
                break;
            };
            match transport.read(remaining) {
                Ok(bytes) if !bytes.is_empty() => {
                    chunks.push(json!({"elapsed_ms": started.elapsed().as_secs_f64()*1000.0, "bytes": bytes.len()}));
                    raw.extend(bytes);
                    match decode_response(&raw, framed) {
                        Ok(Some(p)) => {
                            payload = Some(p);
                            break;
                        }
                        Ok(None) => {}
                        Err(e) => {
                            error = Some(e);
                            break;
                        }
                    }
                }
                Ok(_) => {
                    error = Some("empty USB response".into());
                    break;
                }
                Err(e) => {
                    error = Some(e);
                    break;
                }
            }
        }
    }
    let complete = payload.is_some();
    json!({
        "command": command, "ending": if ending == "\r\n" { "CRLF" } else { "LF" },
        "wire_hex": hex(wire.as_bytes()), "complete": complete,
        "outcome": if complete { "complete" } else if raw.is_empty() { "no_response_or_send_failure" } else { "partial_or_invalid_response" },
        "response_hex": hex(&raw), "response_text": String::from_utf8(raw.clone()).ok(),
        "payload_text": payload.and_then(|p| String::from_utf8(p).ok()),
        "elapsed_ms": started.elapsed().as_secs_f64()*1000.0, "chunks": chunks, "error": error
    })
}
fn baseline(transport: &mut BulkTransport) -> Result<Value, String> {
    let id = observe(transport, "*IDN?", "\n", false);
    let text = id["payload_text"]
        .as_str()
        .unwrap_or("")
        .to_ascii_uppercase();
    if id["complete"] != true || !text.contains("OWON,HDS") {
        return Err(format!(
            "Identity baseline failed; stop rather than interpret stale data: {id}"
        ));
    }
    let status = observe(transport, ":TRIGGER:STATUS?", "\n", false);
    if status["complete"] != true {
        return Err(format!("Status baseline failed: {status}"));
    }
    let head = observe(transport, ":DATA:WAVE:SCREEN:HEAD?", "\n", true);
    if head["complete"] != true {
        return Err(format!("Header baseline failed: {head}"));
    }
    let header: Value =
        serde_json::from_str(head["payload_text"].as_str().ok_or("non-UTF8 header")?)
            .map_err(|e| e.to_string())?;
    Ok(json!({"identity": id["payload_text"], "status": status["payload_text"], "header": header}))
}
fn main() -> Result<(), String> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let trials = match args.first().map(String::as_str) {
        Some("generator") => GENERATOR,
        Some("trigger") => TRIGGER,
        Some("trigger-paths") => TRIGGER_PATHS,
        _ => {
            return Err(
                "Usage: readonly_probe generator|trigger|trigger-paths [new-report.json]".into(),
            )
        }
    };
    if args.len() > 2 {
        return Err("Too many arguments".into());
    }
    let devices = usb::devices()?;
    if devices.len() != 1 {
        return Err(format!(
            "Exactly one OWON device is required: {}",
            serde_json::to_string(&devices).unwrap()
        ));
    }
    let device = &devices[0];
    let mut results = Vec::new();
    let mut aborted = None;
    for trial in trials {
        // A failed session is dropped. Each candidate uses a new handle and must
        // first pass identity/status/header baselines. No reset or buffer flush.
        let mut transport = BulkTransport::open(device.bus, device.address)?;
        let before = match baseline(&mut transport) {
            Ok(value) => value,
            Err(e) => {
                aborted = Some(e);
                break;
            }
        };
        let observation = observe(&mut transport, trial.command, trial.ending, false);
        eprintln!(
            "{}: {} ({:.0} ms)",
            trial.name,
            observation["outcome"],
            observation["elapsed_ms"].as_f64().unwrap_or(0.0)
        );
        results.push(json!({"name": trial.name, "baseline": before, "observation": observation}));
        // Do not send a follow-up query on this handle after a failed response.
        drop(transport);
    }
    let postflight = if aborted.is_none() {
        let mut transport = BulkTransport::open(device.bus, device.address)?;
        match baseline(&mut transport) {
            Ok(v) => Some(v),
            Err(e) => {
                aborted = Some(e);
                None
            }
        }
    } else {
        None
    };
    let report = json!({
        "schema_version": 1, "profile": args[0],
        "observed_at_unix_ms": SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis(),
        "policy": "Fixed read-only queries only; no settings/reset/output enable/driver changes; fresh handle and strict baseline before each candidate",
        "usb": device, "trials": results, "postflight": postflight, "aborted": aborted
    });
    let bytes = serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?;
    if let Some(path) = args.get(1) {
        use std::{fs::OpenOptions, io::Write};
        let path = PathBuf::from(path);
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)
            .map_err(|e| e.to_string())?;
        file.write_all(&bytes)
            .and_then(|_| file.sync_all())
            .map_err(|e| e.to_string())?;
        println!("Report saved: {}", path.display());
    } else {
        println!("{}", String::from_utf8(bytes).unwrap());
    }
    if report["aborted"].is_null() {
        Ok(())
    } else {
        Err("Diagnostic stopped on baseline failure; see report".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    struct Fake {
        chunks: VecDeque<Vec<u8>>,
        wire: Vec<u8>,
    }
    impl Transport for Fake {
        fn write(&mut self, bytes: &[u8], _: Duration) -> Result<(), String> {
            self.wire.extend(bytes);
            Ok(())
        }
        fn read(&mut self, _: Duration) -> Result<Vec<u8>, String> {
            self.chunks.pop_front().ok_or("timeout".into())
        }
    }
    #[test]
    fn candidates_cannot_write_settings() {
        for trial in GENERATOR.iter().chain(TRIGGER).chain(TRIGGER_PATHS) {
            assert!(trial.command.ends_with('?'));
            assert!(!trial.command.contains([';', '\n', '\r', ' ']));
        }
    }
    #[test]
    fn preserves_case_terminators_and_unterminated_evidence() {
        let mut fake = Fake {
            chunks: [b"CH1".to_vec()].into(),
            wire: Vec::new(),
        };
        let r = observe(&mut fake, ":TRIGger:SINGle:SOURce?", "\r\n", false);
        assert_eq!(fake.wire, b":TRIGger:SINGle:SOURce?\r\n");
        assert_eq!(r["complete"], false);
        assert_eq!(r["response_hex"], "434831");
        assert_eq!(r["outcome"], "partial_or_invalid_response");
    }
    #[test]
    fn complete_and_missing_responses_are_distinct() {
        let mut fake = Fake {
            chunks: [b"SI".to_vec(), b"NE\n".to_vec()].into(),
            wire: Vec::new(),
        };
        assert_eq!(
            observe(&mut fake, ":FUNCTION?", "\n", false)["payload_text"],
            "SINE"
        );
        assert_eq!(
            observe(&mut fake, ":FUNCTION?", "\n", false)["outcome"],
            "no_response_or_send_failure"
        );
    }
}
