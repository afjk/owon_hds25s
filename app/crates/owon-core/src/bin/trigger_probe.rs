//! Explicit bounded hardware test of the production trigger API. Optional,
//! separately confirmed temporary GEN ON/OFF; never changes waveform/voltage.
//! No reset, RUN/STOP or raw console. Each trial restores its original parameter.
use owon_core::{
    control, generator,
    protocol::Client,
    trigger::{self, Parameter, Setting, Snapshot},
    usb::{self, BulkTransport},
    waveform,
};
use serde_json::{json, Value};
use std::{fs::OpenOptions, io::Write, path::PathBuf, time::Duration};

fn generator_equal(a: &generator::Snapshot, b: &generator::Snapshot) -> bool {
    a.waveform.eq_ignore_ascii_case(&b.waveform)
        && a.output.eq_ignore_ascii_case(&b.output)
        && a.load.eq_ignore_ascii_case(&b.load)
        && [&a.frequency, &a.period, &a.amplitude, &a.offset]
            .iter()
            .zip([&b.frequency, &b.period, &b.amplitude, &b.offset])
            .all(|(a, b)| control::equivalent(a, b))
}
fn scope_settings(header: &Value) -> Value {
    let mut h = header.clone();
    if let Some(o) = h.as_object_mut() {
        o.retain(|k, _| !k.eq_ignore_ascii_case("Trig") && !k.eq_ignore_ascii_case("RUNSTATUS"));
    }
    if let Some(chs) = h.get_mut("CHANNEL").and_then(Value::as_array_mut) {
        for ch in chs {
            if let Some(o) = ch.as_object_mut() {
                o.retain(|k, _| !k.eq_ignore_ascii_case("FREQUENCE"));
            }
        }
    }
    h
}
fn apply(
    c: &mut Client<BulkTransport>,
    expected: &Snapshot,
    p: Parameter,
    value: &str,
    report: &mut Value,
    name: &str,
) -> Result<Snapshot, String> {
    let r = trigger::apply(
        c,
        &Setting {
            parameter: p,
            value: value.into(),
            expected: expected.clone(),
            confirm_single: true,
            rounding_workaround: false,
        },
    )?;
    eprintln!(
        "{name}: {} => {}, verified={}, preserved={}, status={}",
        r.command,
        p.value(&r.after),
        r.verified,
        r.preserved,
        r.after.status
    );
    let after = r.after.clone();
    report["trials"]
        .as_array_mut()
        .unwrap()
        .push(json!({"name":name,"reply":r}));
    // A mismatched write is recorded; the caller restores only the known changed
    // parameter, with a fresh guarded baseline. No guessed compensation commands.
    Ok(after)
}
fn output(
    c: &mut Client<BulkTransport>,
    expected: &generator::Snapshot,
    value: &str,
    report: &mut Value,
) -> Result<generator::Snapshot, String> {
    let reply = generator::apply(
        c,
        &generator::Setting {
            parameter: generator::Parameter::Output,
            value: value.into(),
            expected: expected.clone(),
            confirm_output: value == "ON",
        },
    )?;
    let after = reply.after.clone();
    let verified = reply.verified;
    report["generator_trials"]
        .as_array_mut()
        .unwrap()
        .push(json!(reply));
    if !verified {
        return Err(format!("GEN OUT {value}の読戻しが一致しません"));
    }
    Ok(after)
}
fn run(mode: &str, report: &mut Value) -> Result<(), String> {
    let devices = usb::devices()?;
    if devices.len() != 1 {
        return Err(format!(
            "実機を1台だけ接続してください: {}",
            serde_json::to_string(&devices).unwrap()
        ));
    }
    let d = &devices[0];
    let mut c = Client::new(BulkTransport::open(d.bus, d.address)?);
    let id = c.text("*IDN?")?;
    trigger::ensure_model(&id)?;
    let before = trigger::read(&mut c)?;
    let header = c.header()?;
    let gen = generator::read(&mut c)?;
    let mut reader = waveform::LiveReader::new(id.clone());
    let initial_wave = reader.read(&mut c)?;
    report["baseline"] =
        json!({"identity":id,"trigger":before,"header":header,"generator":gen,"wave":initial_wave});
    if mode == "restore-level" {
        if !gen.output.eq_ignore_ascii_case("OFF")
            || !before.source.eq_ignore_ascii_case("CH1")
            || !before.coupling.eq_ignore_ascii_case("DC")
            || !control::equivalent(&before.sweep, "AUTO")
        {
            return Err("復元はGEN OFF・CH1/DC/Autoでのみ実行します（書込みなし）".into());
        }
        let restored = apply(
            &mut c,
            &before,
            Parameter::Level,
            "200mv",
            report,
            "restore_original_160mV_with_200mV_quantization_test",
        )?;
        report["original_level_restored"] = json!(control::equivalent(&restored.level, "160mV"));
    }
    let temporary = mode == "verify-output";
    let active_gen = if temporary {
        if !gen.output.eq_ignore_ascii_case("OFF")
            || !gen.waveform.eq_ignore_ascii_case("SINE")
            || !control::equivalent(&gen.frequency, "1000")
            || !control::equivalent(&gen.amplitude, "1.5")
            || !control::equivalent(&gen.offset, "0")
            || !gen.load.eq_ignore_ascii_case("INF")
            || initial_wave.values.len() != 2
            || !control::equivalent(&before.sweep, "AUTO")
            || before.status.eq_ignore_ascii_case("STOP")
        {
            return Err("一時ON試験はGENがOFF・1kHz SINE・1.5Vpp・0V・INF、両CH表示、本体Auto動作中のみ実行します（書込みなし）".into());
        }
        match output(&mut c, &gen, "ON", report) {
            Ok(after) => after,
            Err(e) => {
                if c.is_failed() {
                    report["output_cleanup"] =
                        json!("USB session failed; output state unknown; physical OFF required");
                    return Err(format!(
                        "{e}。通信断で出力OFFを確認できません。本体でGEN OUTをOFFにしてください"
                    ));
                }
                output(&mut c, &gen, "OFF", report)?;
                report["output_cleanup"] =
                    json!("OFF readback verified after unsuccessful ON readback");
                return Err(e);
            }
        }
    } else {
        gen.clone()
    };
    let trial_result = (|| -> Result<(), String> {
        if mode == "verify" || mode == "verify-output" {
            if !active_gen.output.eq_ignore_ascii_case("ON")
                || !active_gen.waveform.eq_ignore_ascii_case("SINE")
                || !control::equivalent(&active_gen.frequency, "1000")
                || initial_wave.values.len() != 2
            {
                return Err(
                    "検証はGEN OUTが1 kHz正弦波・ON・両CH表示の状態でのみ実行します".into(),
                );
            }
            if !control::equivalent(&before.sweep, "AUTO")
                || before.status.eq_ignore_ascii_case("STOP")
            {
                return Err(
                    "検証は本体がAuto・動作中の状態から行います。設定を書かず終了します".into(),
                );
            }
            let mut current = before.clone();
            for (p, value) in [
                (
                    Parameter::Source,
                    if before.source.eq_ignore_ascii_case("CH1") {
                        "CH2"
                    } else {
                        "CH1"
                    },
                ),
                (
                    Parameter::Coupling,
                    if before.coupling.eq_ignore_ascii_case("DC") {
                        "AC"
                    } else {
                        "DC"
                    },
                ),
                (
                    Parameter::Edge,
                    if before.edge.eq_ignore_ascii_case("RISE") {
                        "FALL"
                    } else {
                        "RISE"
                    },
                ),
                (Parameter::Sweep, "NORMAL"),
            ] {
                current = apply(
                    &mut c,
                    &current,
                    p,
                    value,
                    report,
                    &format!("change_{}", p.name()),
                )?;
                let mut reader = waveform::LiveReader::new(id.clone());
                let wave = reader.read(&mut c)?;
                report["trials"].as_array_mut().unwrap().last_mut().unwrap()["wave"] = json!(wave);
                current = apply(
                    &mut c,
                    &current,
                    p,
                    p.value(&before),
                    report,
                    &format!("restore_{}", p.name()),
                )?;
                let restored = report["trials"].as_array().unwrap().last().unwrap()["reply"]
                    ["verified"]
                    .as_bool()
                    .unwrap_or(false);
                if !restored {
                    return Err("復元の読戻しが一致しません。別の値を試さず終了します".into());
                }
                if report["trials"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|r| r["reply"]["verified"] == false)
                {
                    return Err(
                        "変更の読戻しが不一致でした。元の値を復元して、残りの試験を中止しました"
                            .into(),
                    );
                }
            }
            current = apply(
                &mut c,
                &current,
                Parameter::Sweep,
                "SINGLE",
                report,
                "change_single",
            )?;
            let mut statuses = Vec::new();
            for _ in 0..10 {
                let status = c.text(":TRIGGER:STATUS?")?;
                statuses.push(status.clone());
                if status.eq_ignore_ascii_case("STOP") {
                    break;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            report["single_statuses"] = json!(statuses);
            if statuses
                .last()
                .is_some_and(|s| s.eq_ignore_ascii_case("STOP"))
            {
                let a = waveform::capture_stopped(&mut c, &id)?;
                let b = waveform::capture_stopped(&mut c, &id)?;
                report["single_stopped"] = json!({"first":a,"second":b,"stable_payloads":a.values==b.values,"acquisition_id_available":false});
            }
            current = apply(
                &mut c,
                &current,
                Parameter::Sweep,
                &before.sweep,
                report,
                "restore_single_to_original_mode",
            )?;
            report["restored_trigger"] = json!(current);
        }
        Ok(())
    })();
    // OFF is separately authorized cleanup, not an automatic retry of a failed
    // setting. A poisoned USB session cannot safely send any more commands.
    if temporary {
        if c.is_failed() {
            report["output_cleanup"] =
                json!("USB session failed; output state unknown; physical OFF required");
            return trial_result.and(Err(
                "通信断で出力OFFを確認できません。本体でGEN OUTをOFFにしてください".into(),
            ));
        }
        output(&mut c, &active_gen, "OFF", report)?;
        report["output_cleanup"] = json!("OFF readback verified");
    }
    let after = trigger::read(&mut c)?;
    let gen_after = generator::read(&mut c)?;
    let head_after = c.header()?;
    let restored = [
        Parameter::Source,
        Parameter::Coupling,
        Parameter::Edge,
        Parameter::Sweep,
        Parameter::Level,
    ]
    .iter()
    .all(|p| control::equivalent(p.value(&before), p.value(&after)));
    report["postflight"] = json!({"trigger":after,"generator":gen_after,"header":head_after,"generator_preserved":generator_equal(&gen,&gen_after),"scope_settings_preserved":scope_settings(&header)==scope_settings(&head_after),"trigger_settings_restored":restored});
    trial_result?;
    if mode == "verify" || mode == "verify-output" {
        if !restored
            || !generator_equal(&gen, &gen_after)
            || scope_settings(&header) != scope_settings(&head_after)
        {
            return Err("最終設定が試験前と一致しません。記録を確認してください".into());
        }
    } else if mode == "restore-level" && report["original_level_restored"] != true {
        return Err("元の160mVへの復元を確認できません。本体でレベルを戻してください".into());
    }
    Ok(())
}
fn valid_args(args: &[String]) -> bool {
    matches!(
        args.first().map(String::as_str),
        Some("read" | "restore-level")
    ) && args.len() == 2
        || matches!(args.first().map(String::as_str), Some("verify"))
            && args.len() == 3
            && args[2] == "--gen-both-connected"
        || matches!(args.first().map(String::as_str), Some("verify-output"))
            && args.len() == 4
            && args[2] == "--gen-both-connected"
            && args[3] == "--confirm-temporary-output"
}
fn main() -> Result<(), String> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if !valid_args(&args) {
        return Err("Usage: trigger_probe read NEW-REPORT.json | restore-level NEW-REPORT.json | verify NEW-REPORT.json --gen-both-connected | verify-output NEW-REPORT.json --gen-both-connected --confirm-temporary-output".into());
    }
    let path = PathBuf::from(&args[1]);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    let mut report = json!({"schema_version":1,"mode":args[0],"policy":"fixed typed trigger settings, each restored; optional explicitly confirmed temporary GEN ON/OFF only; no waveform/frequency/voltage writes, RUN/STOP, reset or retries after failed USB session","trials":[],"generator_trials":[],"error":null});
    let result = run(&args[0], &mut report);
    if let Err(e) = &result {
        report["error"] = json!(e);
    }
    file.write_all(serde_json::to_string_pretty(&report).unwrap().as_bytes())
        .map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    eprintln!("postflight={}", report["postflight"]);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn write_tests_require_explicit_connection_acknowledgment() {
        let args = |parts: &[&str]| parts.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(valid_args(&args(&["read", "new.json"])));
        assert!(valid_args(&args(&[
            "verify",
            "new.json",
            "--gen-both-connected"
        ])));
        assert!(valid_args(&args(&[
            "verify-output",
            "new.json",
            "--gen-both-connected",
            "--confirm-temporary-output"
        ])));
        for parts in [
            &[][..],
            &["verify", "new.json"],
            &["read", "new.json", "--gen-both-connected"],
            &["verify", "new.json", "--reset"],
            &["verify-output", "new.json", "--gen-both-connected"],
            &["verify-output", "new.json", "--confirm-temporary-output"],
        ] {
            assert!(!valid_args(&args(parts)));
        }
    }
    #[test]
    fn scope_preservation_ignores_only_observational_fields() {
        let a = json!({"RUNSTATUS":"AUTO","Trig":{"edge":"RISE"},"CHANNEL":[{"COUPLING":"DC","FREQUENCE":1000}],"TIMEBASE":{"SCALE":"500us"}});
        let mut b = a.clone();
        b["RUNSTATUS"] = json!("STOP");
        b["Trig"]["edge"] = json!("FALL");
        b["CHANNEL"][0]["FREQUENCE"] = json!(1001);
        assert_eq!(scope_settings(&a), scope_settings(&b));
        b["CHANNEL"][0]["COUPLING"] = json!("AC");
        assert_ne!(scope_settings(&a), scope_settings(&b));
    }
}
