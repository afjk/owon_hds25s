//! HDS25S generator controls. One parameter per request; no automatic output
//! enable, rollback, load switching or arbitrary waveform upload.
use crate::{
    protocol::{Client, Transport},
    Result,
};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub waveform: String,
    pub frequency: String,
    pub period: String,
    pub amplitude: String,
    pub offset: String,
    pub output: String,
    pub load: String,
    pub read_at_unix_ms: u64,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Parameter {
    Waveform,
    Frequency,
    Amplitude,
    Offset,
    Output,
}
#[derive(Clone, Debug, Deserialize)]
pub struct Setting {
    pub parameter: Parameter,
    pub value: String,
    pub expected: Snapshot,
    #[serde(default)]
    pub confirm_output: bool,
}
#[derive(Debug, Serialize)]
pub struct Reply {
    pub command: String,
    pub before: Snapshot,
    pub after: Snapshot,
    pub verified: bool,
    pub preserved: bool,
}
pub struct Command {
    pub(crate) set: String,
    setting: Setting,
    baseline: Snapshot,
}
pub fn ensure_model(identity: &str) -> Result<()> {
    if identity
        .split(',')
        .nth(1)
        .is_some_and(|s| s.trim().eq_ignore_ascii_case("HDS25S"))
    {
        Ok(())
    } else {
        Err("GEN OUT操作は現在HDS25Sのみ対応しています".into())
    }
}
pub fn query_allowed(s: &str) -> bool {
    matches!(
        s,
        ":FUNCTION?"
            | ":FUNCTION:FREQUENCY?"
            | ":FUNCTION:PERIOD?"
            | ":FUNCTION:AMPLITUDE?"
            | ":FUNCTION:OFFSET?"
            | ":FUNCTION:LOAD?"
            | ":CHANNEL?"
    )
}
pub fn read<T: Transport>(client: &mut Client<T>) -> Result<Snapshot> {
    Ok(Snapshot {
        waveform: client.text(":FUNCTION?")?,
        frequency: client.text(":FUNCTION:FREQUENCY?")?,
        period: client.text(":FUNCTION:PERIOD?")?,
        amplitude: client.text(":FUNCTION:AMPLITUDE?")?,
        offset: client.text(":FUNCTION:OFFSET?")?,
        output: client.text(":CHANNEL?")?,
        load: client.text(":FUNCTION:LOAD?")?,
        read_at_unix_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64,
    })
}
fn number(s: &str) -> Result<f64> {
    s.trim()
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .ok_or_else(|| "GEN OUTの数値が不正です。単位なしの有限数値を指定してください".into())
}
pub fn max_frequency(waveform: &str) -> Option<f64> {
    match waveform.trim().to_ascii_uppercase().as_str() {
        "SINE" => Some(10_000_000.0),
        "SQUARE" => Some(2_000_000.0),
        "RAMP" => Some(1_000_000.0),
        "PULSE" => Some(5_000_000.0),
        _ => None,
    }
}
fn safe_state(s: &Snapshot) -> Result<()> {
    let max = max_frequency(&s.waveform)
        .ok_or("GEN OUTの波形種はSINE/SQUARE/RAMP/PULSEのみ対応しています")?;
    if !(0.1..=max).contains(&number(&s.frequency)?) {
        return Err("GEN OUT周波数が波形種の許可範囲外です（最小0.1 Hz）".into());
    }
    let amplitude = number(&s.amplitude)?;
    if !(0.02..=5.0).contains(&amplitude) {
        return Err("GEN OUT振幅は0.02〜5 Vppです".into());
    }
    if number(&s.offset)?.abs() + amplitude / 2.0 > 2.5 + 1e-12 {
        return Err("安全制限: |オフセット| + 振幅/2 は2.5 V以下にしてください".into());
    }
    if !s.load.eq_ignore_ascii_case("INF") {
        return Err("GEN OUT設定変更は負荷INFでのみ対応しています。負荷切替は未実装です".into());
    }
    if !matches!(s.output.to_ascii_uppercase().as_str(), "ON" | "OFF") {
        return Err("GEN OUTの出力状態が不明です".into());
    }
    Ok(())
}
pub fn compile(setting: &Setting, baseline: &Snapshot) -> Result<Command> {
    let value = setting.value.trim();
    if value.is_empty() || !value.is_ascii() || value.contains([';', '\n', '\r', '?', ':']) {
        return Err("GEN OUT設定値が不正です".into());
    }
    // Observed on HDS25S V12.1.0: even :FUNCTION SINE (same value)
    // enables output from OFF. Never configure silently while output is OFF.
    // Require a SEPARATE, explicitly confirmed ON operation for all parameters.
    if setting.parameter != Parameter::Output && !baseline.output.eq_ignore_ascii_case("ON") {
        return Err("本体が設定送信で出力をONにする場合があります。設定変更は接続先を確認して出力ONにした後で行ってください".into());
    }
    let mut next = baseline.clone();
    let (path, value) = match setting.parameter {
        Parameter::Waveform => {
            let v = value.to_ascii_uppercase();
            if max_frequency(&v).is_none() {
                return Err("未対応のGEN OUT波形種です".into());
            }
            next.waveform = v.clone();
            (":FUNCTION", v)
        }
        Parameter::Frequency => {
            let v = number(value)?.to_string();
            next.frequency = v.clone();
            (":FUNCTION:FREQUENCY", v)
        }
        Parameter::Amplitude => {
            let v = number(value)?.to_string();
            next.amplitude = v.clone();
            (":FUNCTION:AMPLITUDE", v)
        }
        Parameter::Offset => {
            let v = number(value)?.to_string();
            next.offset = v.clone();
            (":FUNCTION:OFFSET", v)
        }
        Parameter::Output => {
            let v = value.to_ascii_uppercase();
            if !matches!(v.as_str(), "ON" | "OFF") {
                return Err("GEN OUT出力はON/OFFです".into());
            }
            if v == "ON" && !setting.confirm_output {
                return Err("GEN OUT出力ONには接続先を確認した明示的な承認が必要です".into());
            }
            next.output = v.clone();
            (":CHANNEL", v)
        }
    };
    // OFF remains available even for an unsupported waveform / unsafe baseline.
    if !(setting.parameter == Parameter::Output && value == "OFF") {
        safe_state(&next)?;
    }
    Ok(Command {
        set: format!("{path} {value}"),
        setting: setting.clone(),
        baseline: baseline.clone(),
    })
}
pub(crate) fn validate_command(command: &Command) -> Result<()> {
    if compile(&command.setting, &command.baseline)?.set != command.set {
        Err("GEN OUTコマンドが検証済みの値と一致しません".into())
    } else {
        Ok(())
    }
}
fn numeric_equal(a: &str, b: &str) -> bool {
    match (number(a), number(b)) {
        (Ok(a), Ok(b)) => (a - b).abs() <= 1e-9 * a.abs().max(b.abs()).max(1e-12),
        _ => a.trim().eq_ignore_ascii_case(b.trim()),
    }
}
fn same_except(a: &Snapshot, b: &Snapshot, parameter: Option<Parameter>) -> bool {
    (parameter == Some(Parameter::Waveform) || a.waveform.eq_ignore_ascii_case(&b.waveform))
        && (parameter == Some(Parameter::Frequency)
            || (numeric_equal(&a.frequency, &b.frequency) && numeric_equal(&a.period, &b.period)))
        && (parameter == Some(Parameter::Amplitude) || numeric_equal(&a.amplitude, &b.amplitude))
        && (parameter == Some(Parameter::Offset) || numeric_equal(&a.offset, &b.offset))
        && (parameter == Some(Parameter::Output) || a.output.eq_ignore_ascii_case(&b.output))
        && a.load.eq_ignore_ascii_case(&b.load)
}
pub fn apply<T: Transport>(client: &mut Client<T>, setting: &Setting) -> Result<Reply> {
    compile(setting, &setting.expected)?;
    let before = read(client)?;
    let stopping =
        setting.parameter == Parameter::Output && setting.value.trim().eq_ignore_ascii_case("OFF");
    if !stopping && !same_except(&before, &setting.expected, None) {
        return Err("GEN OUT設定が前回読取りから変わっています。再読取りしてから適用してください（書込みなし）".into());
    }
    let command = compile(setting, &before)?;
    client.write_generator(&command)?;
    let after = read(client)?;
    let requested = match setting.parameter {
        Parameter::Waveform => after.waveform.eq_ignore_ascii_case(setting.value.trim()),
        Parameter::Frequency => {
            numeric_equal(&after.frequency, &setting.value)
                && number(&after.period).is_ok_and(|p| {
                    number(&after.frequency).is_ok_and(|f| (p * f - 1.0).abs() <= 1e-6)
                })
        }
        Parameter::Amplitude => numeric_equal(&after.amplitude, &setting.value),
        Parameter::Offset => numeric_equal(&after.offset, &setting.value),
        Parameter::Output => after.output.eq_ignore_ascii_case(setting.value.trim()),
    };
    let preserved = same_except(&before, &after, Some(setting.parameter));
    Ok(Reply {
        command: command.set,
        before,
        after,
        verified: requested && preserved,
        preserved,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        collections::VecDeque,
        sync::{Arc, Mutex},
        time::Duration,
    };
    fn state() -> Snapshot {
        Snapshot {
            waveform: "SINe".into(),
            frequency: "2.000000e+03".into(),
            period: "5.000000e-04".into(),
            amplitude: "1.000000e+00".into(),
            offset: "0.000000e+00".into(),
            output: "ON".into(),
            load: "INF".into(),
            read_at_unix_ms: 1,
        }
    }
    fn setting(parameter: Parameter, value: &str) -> Setting {
        Setting {
            parameter,
            value: value.into(),
            expected: state(),
            confirm_output: false,
        }
    }
    #[test]
    fn restricts_model_and_queries() {
        assert!(ensure_model("OWON,HDS25S,00000000,V12.1.0").is_ok());
        assert!(ensure_model("OWON,HDS25,123,V1").is_err());
        assert!(ensure_model("OWON,HDS272S,123,V1").is_err());
        assert!(!query_allowed(":CHANNEL ON"));
        assert!(!query_allowed(":FUNCTION:LOAD OFF"));
    }
    #[test]
    fn rejects_injection_nonfinite_and_unsupported_waveforms() {
        for value in ["2000;:CHANNEL ON", "2000\n*RST", "NaN", "inf", "", "2kHz"] {
            assert!(compile(&setting(Parameter::Frequency, value), &state()).is_err());
        }
        assert!(compile(&setting(Parameter::Waveform, "SINC"), &state()).is_err());
        assert!(serde_json::from_str::<Parameter>("\"load\"").is_err());
    }
    #[test]
    fn uses_waveform_specific_frequency_limits() {
        for (wave, max) in [
            ("SINE", 10e6),
            ("SQUARE", 2e6),
            ("RAMP", 1e6),
            ("PULSE", 5e6),
        ] {
            let mut s = state();
            s.waveform = wave.into();
            assert!(compile(&setting(Parameter::Frequency, &max.to_string()), &s).is_ok());
            assert!(compile(&setting(Parameter::Frequency, &(max + 1.0).to_string()), &s).is_err());
        }
        assert!(compile(&setting(Parameter::Frequency, "0.09"), &state()).is_err());
        let mut high = state();
        high.frequency = "5000000".into();
        assert!(compile(&setting(Parameter::Waveform, "RAMP"), &high).is_err());
    }
    #[test]
    fn validates_combined_voltage_and_load() {
        assert!(compile(&setting(Parameter::Amplitude, "5"), &state()).is_ok());
        assert!(compile(&setting(Parameter::Amplitude, "0.019"), &state()).is_err());
        assert!(compile(&setting(Parameter::Offset, "2"), &state()).is_ok());
        assert!(compile(&setting(Parameter::Offset, "2.01"), &state()).is_err());
        let mut loaded = state();
        loaded.load = "50".into();
        assert!(compile(&setting(Parameter::Frequency, "2000"), &loaded).is_err());
    }
    #[test]
    fn output_on_requires_confirmation_and_off_is_always_allowed() {
        assert!(compile(&setting(Parameter::Output, "ON"), &state()).is_err());
        let mut on = setting(Parameter::Output, "ON");
        on.confirm_output = true;
        assert!(compile(&on, &state()).is_ok());
        let mut unknown = state();
        unknown.waveform = "SINC".into();
        unknown.load = "50".into();
        assert!(compile(&setting(Parameter::Output, "OFF"), &unknown).is_ok());
    }
    #[test]
    fn off_state_cannot_be_configured_or_implicitly_enabled() {
        let mut off = state();
        off.output = "OFF".into();
        for (p, value) in [
            (Parameter::Waveform, "SINE"),
            (Parameter::Frequency, "2000"),
            (Parameter::Amplitude, "1"),
            (Parameter::Offset, "0"),
        ] {
            assert!(compile(&setting(p, value), &off).is_err());
        }
    }
    struct Fake {
        responses: VecDeque<Vec<u8>>,
        writes: Arc<Mutex<Vec<String>>>,
    }
    impl Transport for Fake {
        fn write(&mut self, bytes: &[u8], _: Duration) -> Result<()> {
            self.writes
                .lock()
                .unwrap()
                .push(String::from_utf8(bytes.to_vec()).unwrap());
            Ok(())
        }
        fn read(&mut self, _: Duration) -> Result<Vec<u8>> {
            self.responses.pop_front().ok_or("timeout".into())
        }
    }
    fn responses(s: &Snapshot) -> Vec<Vec<u8>> {
        [
            &s.waveform,
            &s.frequency,
            &s.period,
            &s.amplitude,
            &s.offset,
            &s.output,
            &s.load,
        ]
        .into_iter()
        .map(|v| format!("{v}\n").into_bytes())
        .collect()
    }
    fn client(responses: Vec<Vec<u8>>) -> (Client<Fake>, Arc<Mutex<Vec<String>>>) {
        let writes = Arc::new(Mutex::new(vec![]));
        (
            Client::new(Fake {
                responses: responses.into(),
                writes: writes.clone(),
            }),
            writes,
        )
    }
    #[test]
    fn stale_expected_state_never_writes_settings() {
        let mut changed = state();
        changed.amplitude = "2".into();
        let (mut c, writes) = client(responses(&changed));
        assert!(apply(&mut c, &setting(Parameter::Frequency, "2000")).is_err());
        assert_eq!(writes.lock().unwrap().len(), 7);
        assert!(writes
            .lock()
            .unwrap()
            .iter()
            .all(|s| s.trim().ends_with('?')));
    }
    #[test]
    fn explicit_off_is_allowed_despite_stale_other_settings() {
        let mut changed = state();
        changed.frequency = "1000".into();
        changed.period = "0.001".into();
        let mut after = changed.clone();
        after.output = "OFF".into();
        let (mut c, writes) = client([responses(&changed), responses(&after)].concat());
        let r = apply(&mut c, &setting(Parameter::Output, "OFF")).unwrap();
        assert!(r.verified);
        assert_eq!(writes.lock().unwrap()[7], ":CHANNEL OFF\n");
    }
    #[test]
    fn frequency_write_is_single_and_preserves_other_fields() {
        let mut after = state();
        after.frequency = "1000".into();
        after.period = "0.001".into();
        let (mut c, writes) = client([responses(&state()), responses(&after)].concat());
        let r = apply(&mut c, &setting(Parameter::Frequency, "1000")).unwrap();
        assert!(r.verified && r.preserved);
        assert_eq!(writes.lock().unwrap()[7], ":FUNCTION:FREQUENCY 1000\n");
        assert_eq!(writes.lock().unwrap().len(), 15);
    }
    #[test]
    fn collateral_changes_are_not_verified_and_command_tampering_is_rejected() {
        let mut after = state();
        after.amplitude = "2".into();
        let (mut c, _) = client([responses(&state()), responses(&after)].concat());
        let r = apply(&mut c, &setting(Parameter::Frequency, "2000")).unwrap();
        assert!(!r.verified && !r.preserved);
        let mut command = compile(&setting(Parameter::Frequency, "2000"), &state()).unwrap();
        command.set = ":CHANNEL ON".into();
        let (mut c, writes) = client(vec![]);
        assert!(c.write_generator(&command).is_err());
        assert!(writes.lock().unwrap().is_empty());
    }
}
