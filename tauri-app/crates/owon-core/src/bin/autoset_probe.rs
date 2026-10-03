//! Bounded production API diagnostics. No arbitrary SCPI or overwrite.
use owon_core::{
    autoset::{self, Snapshot},
    control, generator,
    protocol::Client,
    trigger::{self, Parameter},
    usb::{self, BulkTransport},
};
use serde_json::{json, Value};
use std::{fs::OpenOptions, io::Write, time::Duration};

fn restoration(s: &Snapshot) -> Result<Vec<control::Setting>, String> {
    autoset::FIELDS
        .iter()
        .map(|(t, p, _)| {
            let mut value = s.scope[&format!("{t}.{p}")].clone();
            if *p == "scale" {
                let list = if *t == "horizontal" {
                    control::TIMEBASES
                } else {
                    control::SCALES
                };
                value = list
                    .iter()
                    .find(|v| control::equivalent(v, &value))
                    .ok_or("復元可能な感度・時間軸がありません")?
                    .to_string();
            }
            if *p == "offset" {
                let v = value.parse::<f64>().map_err(|e| e.to_string())?;
                value = if v == 0.0 {
                    "0".into()
                } else {
                    format!("{:.4}", v + v.signum() * 0.0001)
                };
            }
            let setting = control::Setting {
                target: (*t).into(),
                parameter: (*p).into(),
                value,
            };
            control::compile(&setting)?;
            Ok(setting)
        })
        .collect()
}
fn restore(
    c: &mut Client<BulkTransport>,
    before: &Snapshot,
    report: &mut Value,
    all: bool,
) -> Result<(), String> {
    for setting in restoration(before)? {
        let key = format!("{}.{}", setting.target, setting.parameter);
        let now = autoset::read(c)?;
        if !all && control::equivalent(&now.scope[&key], &before.scope[&key]) {
            continue;
        }
        let reply = control::apply(c, &setting)?;
        std::thread::sleep(Duration::from_millis(100));
        let after = autoset::read(c)?;
        report["restoration"]
            .as_array_mut()
            .unwrap()
            .push(json!({"setting":setting.value,"key":key,"reply":reply,"after":after}));
        if !control::equivalent(&after.scope[&key], &before.scope[&key]) {
            return Err(format!("{key}の復元読戻しが不一致です。試行を中止します"));
        }
    }
    for p in [
        Parameter::Source,
        Parameter::Coupling,
        Parameter::Edge,
        Parameter::Sweep,
        Parameter::Level,
    ] {
        let now = trigger::read(c)?;
        if !all && control::equivalent(p.value(&now), p.value(&before.trigger)) {
            continue;
        }
        let reply = trigger::apply(
            c,
            &trigger::Setting {
                parameter: p,
                value: p.value(&before.trigger).into(),
                expected: now,
                confirm_single: true,
                rounding_workaround: p == Parameter::Level,
            },
        )?;
        let ok = reply.verified;
        report["restoration"]
            .as_array_mut()
            .unwrap()
            .push(json!(reply));
        if !ok {
            return Err(format!("trigger.{}の復元読戻しが不一致です", p.name()));
        }
    }
    Ok(())
}
fn output(c: &mut Client<BulkTransport>, value: &str, report: &mut Value) -> Result<(), String> {
    let expected = generator::read(c)?;
    let reply = generator::apply(
        c,
        &generator::Setting {
            parameter: generator::Parameter::Output,
            value: value.into(),
            expected,
            confirm_output: value == "ON",
        },
    )?;
    let ok = reply.verified;
    report["generator_trials"]
        .as_array_mut()
        .unwrap()
        .push(json!(reply));
    if ok {
        Ok(())
    } else {
        Err(format!("GEN {value}読戻しが不一致です"))
    }
}
fn run(mode: &str, report: &mut Value) -> Result<(), String> {
    let devices = usb::devices()?;
    if devices.len() != 1 {
        return Err("実機を1台だけ接続してください".into());
    }
    let d = &devices[0];
    let mut c = Client::new(BulkTransport::open(d.bus, d.address)?);
    let id = c.text("*IDN?")?;
    trigger::ensure_model(&id)?;
    let before = autoset::read(&mut c)?;
    report["identity"] = json!(id);
    report["baseline"] = json!(before);
    report["header_before"] = c.header()?;
    if mode == "read" {
        return Ok(());
    }
    if !before.generator.output.eq_ignore_ascii_case("OFF")
        || !control::equivalent(&before.trigger.sweep, "AUTO")
        || before.trigger.status.eq_ignore_ascii_case("STOP")
    {
        return Err("試験はGEN OFF・本体Auto動作中からのみ実行します（書込みなし）".into());
    }
    if mode == "restore-original-scale" {
        if before.scope["CH2.probe"] != "1000X"
            || !control::equivalent(&before.scope["CH2.scale"], "200V")
        {
            return Err("復元対象が想定と異なります（書込みなし）".into());
        }
        let reply = control::apply(
            &mut c,
            &control::Setting {
                target: "CH2".into(),
                parameter: "scale".into(),
                value: "100V".into(),
            },
        )?;
        report["trials"].as_array_mut().unwrap().push(json!(reply));
        std::thread::sleep(Duration::from_millis(100));
        let after = autoset::read(&mut c)?;
        report["postflight"] = json!({"snapshot":after,"header":c.header()?,"original_CH2_scale_restored":control::equivalent(&after.scope["CH2.scale"],"100V")});
        return if report["postflight"]["original_CH2_scale_restored"] == true {
            Ok(())
        } else {
            Err("100Vへの復元を確認できません。本体で戻してください".into())
        };
    }
    restoration(&before)?;
    if mode == "check-restore" {
        restore(&mut c, &before, report, true)?;
    } else if mode == "rounding" {
        if !control::equivalent(&before.trigger.level, "160mV")
            || !before.trigger.source.eq_ignore_ascii_case("CH1")
            || !before.trigger.coupling.eq_ignore_ascii_case("DC")
        {
            return Err("丸め試験はCH1/DC/160mVからのみ実行します（書込みなし）".into());
        }
        for value in ["160mV", "320mV", "160mV"] {
            let expected = trigger::read(&mut c)?;
            let reply = trigger::apply(
                &mut c,
                &trigger::Setting {
                    parameter: Parameter::Level,
                    value: value.into(),
                    expected,
                    confirm_single: false,
                    rounding_workaround: true,
                },
            )?;
            let ok = reply.verified;
            report["trials"].as_array_mut().unwrap().push(json!(reply));
            if !ok {
                return Err("丸め対策の読戻し不一致。再送や別の補正値は試しません".into());
            }
        }
    } else {
        let g = &before.generator;
        if !g.waveform.eq_ignore_ascii_case("SINE")
            || !control::equivalent(&g.frequency, "1000")
            || !control::equivalent(&g.amplitude, "1.5")
            || !control::equivalent(&g.offset, "0")
            || !g.load.eq_ignore_ascii_case("INF")
        {
            return Err("GEN試験は既存1kHz SINE/1.5Vpp/0V/INFのみ（書込みなし）".into());
        }
        if let Err(e) = output(&mut c, "ON", report) {
            if c.is_failed() {
                report["cleanup"] = json!("output unknown; physical OFF required");
                return Err(format!("{e}。通信断のため本体でGEN OUTをOFFにしてください"));
            }
            output(&mut c, "OFF", report)?;
            return Err(e);
        }
        let result = (|| {
            let expected = autoset::read(&mut c)?;
            let reply = autoset::apply(
                &mut c,
                &autoset::Request {
                    expected,
                    confirm: true,
                },
            )?;
            let acceptable = reply.readback_stable && reply.generator_preserved;
            report["autoset"] = json!(reply);
            report["header_after_auto"] = c.header()?;
            if !acceptable {
                return Err("Auto後の設定が安定しないかGEN設定が変わりました".into());
            }
            restore(&mut c, &before, report, false)
        })();
        if c.is_failed() {
            report["cleanup"] =
                json!("USB session failed; output state unknown; physical OFF required");
            return Err("通信断。GEN OUTを本体でOFFにし、保存された元設定へ戻してください".into());
        }
        output(&mut c, "OFF", report)?;
        result?;
    }
    let after = autoset::read(&mut c)?;
    let restored = autoset::same_settings(&before, &after);
    report["postflight"] =
        json!({"snapshot":after,"all_settings_restored":restored,"header":c.header()?});
    if !restored {
        return Err("試験前と最終設定が一致しません".into());
    }
    Ok(())
}
fn valid_args(a: &[String]) -> bool {
    a.len() == 2
        && matches!(
            a[0].as_str(),
            "read" | "check-restore" | "rounding" | "restore-original-scale"
        )
        || a.len() == 4
            && a[0] == "verify-output"
            && a[2] == "--gen-both-connected"
            && a[3] == "--confirm-temporary-output"
}
fn main() -> Result<(), String> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if !valid_args(&args) {
        return Err("Usage: autoset_probe read|check-restore|rounding NEW-REPORT.json | verify-output NEW-REPORT.json --gen-both-connected --confirm-temporary-output".into());
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args[1])
        .map_err(|e| e.to_string())?;
    let mut report = json!({"mode":args[0],"policy":"fixed typed settings only; no reset, RUN/STOP, arbitrary SCPI or retries after failed USB; optional acknowledged temporary GEN output; explicit baseline restoration","restoration":[],"trials":[],"generator_trials":[],"error":null});
    let result = run(&args[0], &mut report);
    if let Err(e) = &result {
        report["error"] = json!(e);
    }
    file.write_all(serde_json::to_string_pretty(&report).unwrap().as_bytes())
        .map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    eprintln!(
        "error={} postflight={}",
        report["error"], report["postflight"]
    );
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn requires_exact_args() {
        let a = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(valid_args(&a(&["read", "new.json"])));
        assert!(!valid_args(&a(&[])));
        assert!(!valid_args(&a(&["verify-output", "new.json"])));
        assert!(valid_args(&a(&[
            "verify-output",
            "new.json",
            "--gen-both-connected",
            "--confirm-temporary-output"
        ])));
    }
}
