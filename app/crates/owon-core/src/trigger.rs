//! HDS25S V12.1.0 edge trigger. Fixed, hardware-observed query paths;
//! one guarded write, full trigger readback, no RUN/STOP/reset/output commands.
use crate::{
    control,
    protocol::{Client, Transport},
    Result,
};
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub source: String,
    pub coupling: String,
    pub edge: String,
    pub sweep: String,
    pub level: String,
    pub status: String,
    pub read_at_unix_ms: u64,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Parameter {
    Source,
    Coupling,
    Edge,
    Sweep,
    Level,
}
impl Parameter {
    pub fn name(self) -> &'static str {
        match self {
            Self::Source => "source",
            Self::Coupling => "coupling",
            Self::Edge => "edge",
            Self::Sweep => "sweep",
            Self::Level => "level",
        }
    }
    pub fn value(self, s: &Snapshot) -> &str {
        match self {
            Self::Source => &s.source,
            Self::Coupling => &s.coupling,
            Self::Edge => &s.edge,
            Self::Sweep => &s.sweep,
            Self::Level => &s.level,
        }
    }
}
const PARAMETERS: [Parameter; 5] = [
    Parameter::Source,
    Parameter::Coupling,
    Parameter::Edge,
    Parameter::Sweep,
    Parameter::Level,
];
#[derive(Clone, Debug, Deserialize)]
pub struct Setting {
    pub parameter: Parameter,
    pub value: String,
    pub expected: Snapshot,
    #[serde(default)]
    pub confirm_single: bool,
    #[serde(default)]
    pub rounding_workaround: bool,
}
#[derive(Debug, Serialize)]
pub struct Reply {
    pub command: String,
    pub before: Snapshot,
    pub after: Snapshot,
    pub verified: bool,
    pub preserved: bool,
    pub linked_level_changed: bool,
    pub readback_stable: bool,
    pub readback_reads: usize,
    pub requested_value: String,
    pub rounding_workaround: bool,
}
pub fn ensure_model(identity: &str) -> Result<()> {
    let parts: Vec<_> = identity.split(',').map(str::trim).collect();
    if parts.len() == 4
        && parts[0].eq_ignore_ascii_case("OWON")
        && parts[1].eq_ignore_ascii_case("HDS25S")
        && parts[3].eq_ignore_ascii_case("V12.1.0")
    {
        Ok(())
    } else {
        Err("トリガー操作は現在OWON HDS25S V12.1.0のみ対応しています".into())
    }
}
pub fn read<T: Transport>(client: &mut Client<T>) -> Result<Snapshot> {
    Ok(Snapshot {
        source: client.text(":TRIGGER:SOURCE?")?,
        coupling: client.text(":TRIGGER:COUPLING?")?,
        edge: client.text(":TRIGGER:SINGLE:EDGE?")?,
        sweep: client.text(":TRIGGER:SWEEP?")?,
        level: client.text(":TRIGGER:SINGLE:EDGE:LEVEL?")?,
        status: client.text(":TRIGGER:STATUS?")?,
        read_at_unix_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64,
    })
}
pub fn compile(setting: &Setting) -> Result<control::Command> {
    control::compile(&control::Setting {
        target: "trigger".into(),
        parameter: setting.parameter.name().into(),
        value: setting.value.clone(),
    })?;
    // Check all baseline fields before any USB operation, including tiny voltage units.
    for p in PARAMETERS {
        control::compile(&control::Setting {
            target: "trigger".into(),
            parameter: p.name().into(),
            value: p.value(&setting.expected).into(),
        })?;
    }
    let single =
        setting.parameter == Parameter::Sweep && control::equivalent(&setting.value, "SINGLE");
    if single && !setting.confirm_single {
        return Err(
            "Singleは本体の取得状態を変える可能性があります。確認して適用してください".into(),
        );
    }
    let value = if setting.rounding_workaround {
        if setting.parameter != Parameter::Level {
            return Err("丸め対策はトリガーレベル専用です".into());
        }
        let v = control::voltage(&setting.value).ok_or("レベルが不正です")?;
        // Based on owowon's workaround; zero remains zero. Verification always
        // compares the readback to the user's original value, not this wire value.
        if v.abs() > 100.0 {
            return Err("レベルが許可範囲外です".into());
        }
        let wire = if v == 0.0 {
            0.0
        } else {
            v + v.signum() * 0.0001
        };
        if wire.abs() > 100.0 {
            return Err("上限値では丸め対策を無効にしてください".into());
        }
        format!("{wire:.12}V")
    } else {
        setting.value.clone()
    };
    control::compile(&control::Setting {
        target: "trigger".into(),
        parameter: setting.parameter.name().into(),
        value,
    })
}
fn same_except(a: &Snapshot, b: &Snapshot, except: Option<Parameter>) -> bool {
    PARAMETERS
        .iter()
        .all(|p| Some(*p) == except || control::equivalent(p.value(a), p.value(b)))
}
pub fn apply<T: Transport>(client: &mut Client<T>, setting: &Setting) -> Result<Reply> {
    let command = compile(setting)?;
    let before = read(client)?;
    if !same_except(&before, &setting.expected, None) {
        return Err(
            "トリガー設定が本体側で変わっています。再読取りしてから適用してください（書込みなし）"
                .into(),
        );
    }
    client.write_setting(&command)?;
    // HDS25S applies settings asynchronously: an immediate query can still
    // return the old source while subsequent fields already reflect the new CH.
    // Poll reads only (never repeat the write), requiring two matching snapshots.
    std::thread::sleep(Duration::from_millis(50));
    let mut after = read(client)?;
    let mut readback_stable = false;
    let mut readback_reads = 1;
    for _ in 0..3 {
        std::thread::sleep(Duration::from_millis(50));
        let next = read(client)?;
        readback_reads += 1;
        readback_stable = same_except(&after, &next, None);
        after = next;
        if readback_stable {
            break;
        }
    }
    let preserved = same_except(&before, &after, Some(setting.parameter));
    // Observed source and trigger-coupling changes select a corresponding level
    // (e.g. DC 160mV -> AC 0V -> DC 160mV). Report the linked level explicitly;
    // never silently rewrite it. All other non-requested settings remain guarded.
    let level_linked = matches!(setting.parameter, Parameter::Source | Parameter::Coupling);
    let linked_level_changed = level_linked && !control::equivalent(&before.level, &after.level);
    let compatible = preserved
        || (level_linked
            && PARAMETERS.iter().all(|p| {
                *p == setting.parameter
                    || *p == Parameter::Level
                    || control::equivalent(p.value(&before), p.value(&after))
            }));
    let verified = readback_stable
        && compatible
        && control::equivalent(setting.parameter.value(&after), &setting.value);
    Ok(Reply {
        command: command.set,
        before,
        after,
        verified,
        preserved,
        linked_level_changed,
        readback_stable,
        readback_reads,
        requested_value: setting.value.clone(),
        rounding_workaround: setting.rounding_workaround,
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
    fn snapshot() -> Snapshot {
        Snapshot {
            source: "CH1".into(),
            coupling: "DC".into(),
            edge: "RISe".into(),
            sweep: "AUTo".into(),
            level: "0.00pV".into(),
            status: "TRIG".into(),
            read_at_unix_ms: 0,
        }
    }
    fn setting(p: Parameter, value: &str) -> Setting {
        Setting {
            parameter: p,
            value: value.into(),
            expected: snapshot(),
            confirm_single: false,
            rounding_workaround: false,
        }
    }
    struct Fake {
        replies: VecDeque<Vec<u8>>,
        commands: Arc<Mutex<Vec<String>>>,
    }
    impl Transport for Fake {
        fn write(&mut self, data: &[u8], _: Duration) -> Result<()> {
            self.commands
                .lock()
                .unwrap()
                .push(String::from_utf8(data.to_vec()).unwrap());
            Ok(())
        }
        fn read(&mut self, _: Duration) -> Result<Vec<u8>> {
            self.replies.pop_front().ok_or("timeout".into())
        }
    }
    fn texts(s: &Snapshot) -> Vec<Vec<u8>> {
        PARAMETERS
            .iter()
            .map(|p| format!("{}\n", p.value(s)).into_bytes())
            .chain([format!("{}\n", s.status).into_bytes()])
            .collect()
    }
    fn client(states: &[Snapshot]) -> (Client<Fake>, Arc<Mutex<Vec<String>>>) {
        let commands = Arc::new(Mutex::new(Vec::new()));
        (
            Client::new(Fake {
                replies: states.iter().flat_map(texts).collect(),
                commands: commands.clone(),
            }),
            commands,
        )
    }
    #[test]
    fn correct_paths_and_model_guard() {
        for (p, value, path) in [
            (Parameter::Source, "CH2", ":TRIGGER:SOURCE"),
            (Parameter::Coupling, "AC", ":TRIGGER:COUPLING"),
            (Parameter::Edge, "FALL", ":TRIGGER:SINGLE:EDGE"),
            (Parameter::Sweep, "NORMAL", ":TRIGGER:SWEEP"),
            (Parameter::Level, "100mV", ":TRIGGER:SINGLE:EDGE:LEVEL"),
        ] {
            assert_eq!(
                compile(&setting(p, value)).unwrap().set,
                format!("{path} {value}")
            );
        }
        assert!(ensure_model("OWON,HDS25S,123,V12.1.0").is_ok());
        assert!(ensure_model("OTHER,HDS25S,123,V12.1.0").is_err());
        assert!(ensure_model("OWON,HDS25S,123,V99.0").is_err());
        assert!(ensure_model("OWON,HDS25,123,V12.1.0").is_err());
    }
    #[test]
    fn rejects_injection_nonfinite_bounds_and_unconfirmed_single() {
        for value in ["NaN", "inf", "101V", "0V;*RST", "", "0V\n:RUN"] {
            assert!(compile(&setting(Parameter::Level, value)).is_err());
        }
        assert!(compile(&setting(Parameter::Sweep, "SINGLE")).is_err());
        let mut s = setting(Parameter::Sweep, "SINGLE");
        s.confirm_single = true;
        assert!(compile(&s).is_ok());
    }
    #[test]
    fn tiny_units_and_dimensional_comparison() {
        assert!(control::equivalent("0.00pV", "0V"));
        assert!(control::equivalent("1000nV", "1uV"));
        assert!(!control::equivalent("1ms", "1mV"));
    }
    #[test]
    fn rounding_compiler_is_opt_in_signed_bounded_and_does_not_change_zero() {
        for (value, wire) in [
            ("160mV", "0.160100000000V"),
            ("-160mV", "-0.160100000000V"),
            ("0V", "0.000000000000V"),
        ] {
            let mut s = setting(Parameter::Level, value);
            assert!(compile(&s).unwrap().set.ends_with(value));
            s.rounding_workaround = true;
            assert!(compile(&s).unwrap().set.ends_with(wire));
        }
        for value in ["100V", "-100V", "101V", "0V;*RST", "NaN"] {
            let mut s = setting(Parameter::Level, value);
            s.rounding_workaround = true;
            assert!(compile(&s).is_err());
        }
        let mut s = setting(Parameter::Source, "CH2");
        s.rounding_workaround = true;
        assert!(compile(&s).is_err());
    }
    #[test]
    fn rounding_readback_verifies_requested_value_not_the_transmitted_epsilon() {
        let mut s = setting(Parameter::Level, "160mV");
        s.rounding_workaround = true;
        let mut after = snapshot();
        after.level = "160mV".into();
        let (mut c, commands) = client(&[snapshot(), after.clone(), after.clone()]);
        let r = apply(&mut c, &s).unwrap();
        assert!(r.verified);
        assert_eq!(r.requested_value, "160mV");
        assert_eq!(
            commands.lock().unwrap()[6],
            ":TRIGGER:SINGLE:EDGE:LEVEL 0.160100000000V\n"
        );
        after.level = "160.1mV".into();
        let (mut c, _) = client(&[snapshot(), after.clone(), after]);
        assert!(!apply(&mut c, &s).unwrap().verified);
    }
    #[test]
    fn stale_baseline_does_not_write() {
        let mut current = snapshot();
        current.source = "CH2".into();
        let (mut c, writes) = client(&[current]);
        assert!(apply(&mut c, &setting(Parameter::Edge, "FALL")).is_err());
        assert!(writes.lock().unwrap().iter().all(|s| s.ends_with("?\n")));
    }
    #[test]
    fn full_read_write_read_and_collateral_changes() {
        let mut after = snapshot();
        after.edge = "FALL".into();
        let (mut c, writes) = client(&[snapshot(), after.clone(), after.clone()]);
        assert!(
            apply(&mut c, &setting(Parameter::Edge, "FALL"))
                .unwrap()
                .verified
        );
        let writes = writes.lock().unwrap();
        assert_eq!(writes.len(), 19);
        assert_eq!(writes[6], ":TRIGGER:SINGLE:EDGE FALL\n");
        after.level = "1V".into();
        let (mut c, _) = client(&[snapshot(), after.clone(), after]);
        let r = apply(&mut c, &setting(Parameter::Edge, "FALL")).unwrap();
        assert!(!r.verified);
        assert!(!r.preserved);
    }
    #[test]
    fn mismatch_is_reported_without_retry_or_rollback() {
        let (mut c, writes) = client(&[snapshot(), snapshot(), snapshot()]);
        let r = apply(&mut c, &setting(Parameter::Level, "100mV")).unwrap();
        assert!(!r.verified);
        assert!(r.preserved);
        assert_eq!(writes.lock().unwrap().len(), 19);
    }
    #[test]
    fn automatic_status_changes_are_not_stale_settings() {
        let mut before = snapshot();
        before.status = "STOP".into();
        let mut after = before.clone();
        after.edge = "FALL".into();
        let (mut c, _) = client(&[before, after.clone(), after]);
        assert!(
            apply(&mut c, &setting(Parameter::Edge, "FALL"))
                .unwrap()
                .verified
        );
    }
    #[test]
    fn failed_read_never_sends_a_write_or_retries() {
        let (mut c, writes) = client(&[]);
        assert!(apply(&mut c, &setting(Parameter::Edge, "FALL")).is_err());
        assert!(c.is_failed());
        assert_eq!(writes.lock().unwrap().len(), 1);
    }
    #[test]
    fn source_waits_for_consistent_readback_and_reports_channel_level() {
        let mut transitioning = snapshot();
        transitioning.level = "160mV".into();
        let mut after = transitioning.clone();
        after.source = "CH2".into();
        let (mut c, writes) = client(&[snapshot(), transitioning, after.clone(), after]);
        let r = apply(&mut c, &setting(Parameter::Source, "CH2")).unwrap();
        assert!(r.verified && r.linked_level_changed && r.readback_stable);
        assert!(!r.preserved);
        assert_eq!(r.readback_reads, 3);
        assert_eq!(
            writes
                .lock()
                .unwrap()
                .iter()
                .filter(|s| !s.ends_with("?\n"))
                .count(),
            1
        );
    }
    #[test]
    fn unstable_readback_is_not_verified_and_does_not_repeat_write() {
        let mut a = snapshot();
        a.edge = "FALL".into();
        let (mut c, writes) = client(&[snapshot(), a.clone(), snapshot(), a, snapshot()]);
        let r = apply(&mut c, &setting(Parameter::Edge, "FALL")).unwrap();
        assert!(!r.verified && !r.readback_stable);
        assert_eq!(r.readback_reads, 4);
        assert_eq!(
            writes
                .lock()
                .unwrap()
                .iter()
                .filter(|s| !s.ends_with("?\n"))
                .count(),
            1
        );
    }
    #[test]
    fn coupling_reports_linked_level_but_rejects_other_collateral_changes() {
        let mut before = snapshot();
        before.level = "160mV".into();
        let mut after = snapshot();
        after.coupling = "AC".into();
        let mut s = setting(Parameter::Coupling, "AC");
        s.expected = before.clone();
        let (mut c, _) = client(&[before.clone(), after.clone(), after.clone()]);
        let r = apply(&mut c, &s).unwrap();
        assert!(r.verified && r.linked_level_changed);
        assert!(!r.preserved);
        after.edge = "FALL".into();
        let (mut c, _) = client(&[before, after.clone(), after]);
        assert!(!apply(&mut c, &s).unwrap().verified);
    }
}
