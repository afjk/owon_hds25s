//! Guarded Auto setup. Fixed command only, one write, bounded readback polling.
//! No generator writes, automatic rollback, reset, or arbitrary SCPI.
use crate::{
    control, generator,
    protocol::{Client, Transport},
    trigger, Result,
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, time::Duration};

// The restore preflight exposed a CH2 scale mismatch. Keep production writes
// disabled until the hardware test and baseline restoration both pass.
pub const HARDWARE_VERIFIED: bool = false;
pub fn ensure_available(identity: &str) -> Result<()> {
    trigger::ensure_model(identity)?;
    if !HARDWARE_VERIFIED {
        return Err("Auto設定は実機検証待ちです。現在は設定読取りのみ使用できます".into());
    }
    Ok(())
}

pub const FIELDS: &[(&str, &str, &str)] = &[
    ("CH1", "display", ":CH1:DISPLAY?"),
    ("CH1", "coupling", ":CH1:COUPLING?"),
    ("CH1", "probe", ":CH1:PROBE?"),
    ("CH1", "scale", ":CH1:SCALE?"),
    ("CH1", "offset", ":CH1:OFFSET?"),
    ("CH2", "display", ":CH2:DISPLAY?"),
    ("CH2", "coupling", ":CH2:COUPLING?"),
    ("CH2", "probe", ":CH2:PROBE?"),
    ("CH2", "scale", ":CH2:SCALE?"),
    ("CH2", "offset", ":CH2:OFFSET?"),
    ("horizontal", "scale", ":HORIZONTAL:SCALE?"),
    ("horizontal", "offset", ":HORIZONTAL:OFFSET?"),
    ("acquire", "mode", ":ACQUIRE:MODE?"),
    ("acquire", "memory", ":ACQUIRE:DEPMEM?"),
];
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub scope: BTreeMap<String, String>,
    pub trigger: trigger::Snapshot,
    pub generator: generator::Snapshot,
}
#[derive(Deserialize)]
pub struct Request {
    pub expected: Snapshot,
    pub confirm: bool,
}
#[derive(Debug, Serialize)]
pub struct Change {
    pub field: String,
    pub before: String,
    pub after: String,
}
#[derive(Debug, Serialize)]
pub struct Reply {
    pub command: String,
    pub before: Snapshot,
    pub after: Snapshot,
    pub changes: Vec<Change>,
    pub readback_stable: bool,
    pub readback_reads: usize,
    pub generator_preserved: bool,
    // No dedicated Auto acknowledgement exists. Unchanged settings alone do
    // not prove the command was executed, even when readbacks are stable.
    pub effect_observed: bool,
}
pub fn read<T: Transport>(c: &mut Client<T>) -> Result<Snapshot> {
    let mut scope = BTreeMap::new();
    for (target, p, query) in FIELDS {
        scope.insert(format!("{target}.{p}"), c.text(query)?);
    }
    Ok(Snapshot {
        scope,
        trigger: trigger::read(c)?,
        generator: generator::read(c)?,
    })
}
fn values(s: &Snapshot) -> BTreeMap<String, String> {
    let mut v = s.scope.clone();
    for p in [
        trigger::Parameter::Source,
        trigger::Parameter::Coupling,
        trigger::Parameter::Edge,
        trigger::Parameter::Sweep,
        trigger::Parameter::Level,
    ] {
        v.insert(format!("trigger.{}", p.name()), p.value(&s.trigger).into());
    }
    v
}
pub fn generator_equal(a: &generator::Snapshot, b: &generator::Snapshot) -> bool {
    a.waveform.eq_ignore_ascii_case(&b.waveform)
        && a.output.eq_ignore_ascii_case(&b.output)
        && a.load.eq_ignore_ascii_case(&b.load)
        && [&a.frequency, &a.period, &a.amplitude, &a.offset]
            .into_iter()
            .zip([&b.frequency, &b.period, &b.amplitude, &b.offset])
            .all(|(a, b)| control::equivalent(a, b))
}
pub fn same_settings(a: &Snapshot, b: &Snapshot) -> bool {
    let av = values(a);
    let bv = values(b);
    av.len() == bv.len()
        && av
            .iter()
            .all(|(k, v)| bv.get(k).is_some_and(|bv| control::equivalent(v, bv)))
        && generator_equal(&a.generator, &b.generator)
}
pub fn validate(r: &Request) -> Result<()> {
    if !r.confirm {
        return Err("Auto設定は感度・時間軸・トリガーなどを変更します。確認してください".into());
    }
    if r.expected.scope.len() != FIELDS.len()
        || FIELDS
            .iter()
            .any(|(t, p, _)| !r.expected.scope.contains_key(&format!("{t}.{p}")))
    {
        return Err("Auto設定の現在値を再読取りしてください".into());
    }
    Ok(())
}
pub fn apply<T: Transport>(c: &mut Client<T>, r: &Request) -> Result<Reply> {
    validate(r)?;
    let before = read(c)?;
    if !same_settings(&before, &r.expected) {
        return Err("本体設定が変わっています。再読取りしてください（Auto書込みなし）".into());
    }
    c.write_autoset()?;
    std::thread::sleep(Duration::from_millis(1000));
    let mut after = read(c)?;
    let mut stable = false;
    let mut reads = 1;
    for _ in 0..3 {
        std::thread::sleep(Duration::from_millis(250));
        let next = read(c)?;
        stable = same_settings(&after, &next);
        after = next;
        reads += 1;
        if stable {
            break;
        }
    }
    let b = values(&before);
    let changes = values(&after)
        .into_iter()
        .filter_map(|(field, after)| {
            let before = b.get(&field)?;
            (!control::equivalent(before, &after)).then(|| Change {
                field,
                before: before.clone(),
                after,
            })
        })
        .collect::<Vec<_>>();
    Ok(Reply {
        command: ":AUT .".into(),
        generator_preserved: generator_equal(&before.generator, &after.generator),
        effect_observed: !changes.is_empty(),
        before,
        after,
        changes,
        readback_stable: stable,
        readback_reads: reads,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        collections::VecDeque,
        sync::{Arc, Mutex},
    };
    fn snapshot() -> Snapshot {
        let scope = FIELDS
            .iter()
            .zip([
                "ON", "DC", "20X", "4e+00", "0.00", "ON", "DC", "1000X", "1e+02", "-2.00", "5e-04",
                "0.00", "SAMPle", "4K",
            ])
            .map(|((t, p, _), v)| (format!("{t}.{p}"), v.into()))
            .collect();
        Snapshot {
            scope,
            trigger: trigger::Snapshot {
                source: "CH1".into(),
                coupling: "DC".into(),
                edge: "RISe".into(),
                sweep: "AUTo".into(),
                level: "160mV".into(),
                status: "TRIG".into(),
                read_at_unix_ms: 0,
            },
            generator: generator::Snapshot {
                waveform: "SINe".into(),
                frequency: "1000".into(),
                period: "0.001".into(),
                amplitude: "1.5".into(),
                offset: "0".into(),
                output: "OFF".into(),
                load: "INF".into(),
                read_at_unix_ms: 0,
            },
        }
    }
    struct Fake {
        replies: VecDeque<Vec<u8>>,
        commands: Arc<Mutex<Vec<String>>>,
        fail_auto: bool,
    }
    impl Transport for Fake {
        fn write(&mut self, data: &[u8], _: Duration) -> Result<()> {
            let s = String::from_utf8(data.to_vec()).unwrap();
            self.commands.lock().unwrap().push(s.clone());
            if self.fail_auto && s == ":AUT .\n" {
                Err("write failed".into())
            } else {
                Ok(())
            }
        }
        fn read(&mut self, _: Duration) -> Result<Vec<u8>> {
            self.replies.pop_front().ok_or("timeout".into())
        }
    }
    fn client(snapshots: &[Snapshot]) -> (Client<Fake>, Arc<Mutex<Vec<String>>>) {
        client_with_failure(snapshots, false)
    }
    fn client_with_failure(
        snapshots: &[Snapshot],
        fail_auto: bool,
    ) -> (Client<Fake>, Arc<Mutex<Vec<String>>>) {
        let mut replies = VecDeque::new();
        for s in snapshots {
            let mut texts = FIELDS
                .iter()
                .map(|(t, p, _)| s.scope[&format!("{t}.{p}")].clone())
                .collect::<Vec<_>>();
            texts.extend(
                [
                    &s.trigger.source,
                    &s.trigger.coupling,
                    &s.trigger.edge,
                    &s.trigger.sweep,
                    &s.trigger.level,
                    &s.trigger.status,
                ]
                .map(String::clone),
            );
            texts.extend(
                [
                    &s.generator.waveform,
                    &s.generator.frequency,
                    &s.generator.period,
                    &s.generator.amplitude,
                    &s.generator.offset,
                    &s.generator.output,
                    &s.generator.load,
                ]
                .map(String::clone),
            );
            replies.extend(texts.into_iter().map(|v| format!("{v}\n").into_bytes()));
        }
        let commands = Arc::new(Mutex::new(Vec::new()));
        (
            Client::new(Fake {
                replies,
                commands: commands.clone(),
                fail_auto,
            }),
            commands,
        )
    }
    fn request() -> Request {
        Request {
            expected: snapshot(),
            confirm: true,
        }
    }
    #[test]
    fn production_auto_is_gated_until_hardware_validation() {
        assert!(ensure_available("OWON,HDS25S,123,V12.1.0").is_err());
        assert!(ensure_available("OTHER,HDS25S,123,V12.1.0").is_err());
    }
    #[test]
    fn unconfirmed_or_incomplete_baseline_never_touches_usb() {
        let (mut c, w) = client(&[]);
        let mut r = request();
        r.confirm = false;
        assert!(apply(&mut c, &r).is_err());
        r.confirm = true;
        r.expected.scope.remove("CH1.scale");
        assert!(apply(&mut c, &r).is_err());
        assert!(w.lock().unwrap().is_empty());
    }
    #[test]
    fn stale_scope_or_generator_never_writes_auto() {
        for gen in [false, true] {
            let mut current = snapshot();
            if gen {
                current.generator.output = "ON".into();
            } else {
                current.scope.insert("CH1.offset".into(), "1.00".into());
            }
            let (mut c, w) = client(&[current]);
            assert!(apply(&mut c, &request()).is_err());
            assert!(w.lock().unwrap().iter().all(|s| s.ends_with("?\n")));
        }
    }
    #[test]
    fn one_auto_write_reports_changes_without_generator_writes() {
        let mut after = snapshot();
        after
            .scope
            .insert("horizontal.scale".into(), "1e-04".into());
        let (mut c, w) = client(&[snapshot(), after.clone(), after]);
        let reply = apply(&mut c, &request()).unwrap();
        assert!(reply.readback_stable && reply.generator_preserved && reply.effect_observed);
        assert_eq!(reply.readback_reads, 2);
        assert_eq!(reply.changes.len(), 1);
        assert_eq!(reply.changes[0].field, "horizontal.scale");
        let writes = w
            .lock()
            .unwrap()
            .iter()
            .filter(|s| !s.ends_with("?\n"))
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(writes, [":AUT .\n"]);
    }
    #[test]
    fn unchanged_readbacks_do_not_claim_auto_executed() {
        let (mut c, _) = client(&[snapshot(), snapshot(), snapshot()]);
        let reply = apply(&mut c, &request()).unwrap();
        assert!(reply.readback_stable);
        assert!(!reply.effect_observed);
    }
    #[test]
    fn generator_changes_are_reported_not_rolled_back() {
        let mut after = snapshot();
        after.generator.output = "ON".into();
        let (mut c, w) = client(&[snapshot(), after.clone(), after]);
        assert!(!apply(&mut c, &request()).unwrap().generator_preserved);
        assert_eq!(
            w.lock()
                .unwrap()
                .iter()
                .filter(|s| !s.ends_with("?\n"))
                .count(),
            1
        );
    }
    #[test]
    fn failed_read_or_write_poisons_session_without_retry() {
        let (mut c, w) = client(&[]);
        assert!(apply(&mut c, &request()).is_err());
        assert!(c.is_failed());
        assert_eq!(w.lock().unwrap().len(), 1);
        let (mut c, w) = client(&[snapshot()]);
        // Readback timeout after the one command must not re-send Auto.
        assert!(apply(&mut c, &request()).is_err());
        assert!(c.is_failed());
        assert_eq!(
            w.lock()
                .unwrap()
                .iter()
                .filter(|s| s.as_str() == ":AUT .\n")
                .count(),
            1
        );
        let (mut c, w) = client_with_failure(&[snapshot()], true);
        assert!(apply(&mut c, &request()).is_err());
        assert!(c.is_failed());
        assert_eq!(w.lock().unwrap().len(), 28);
        assert!(apply(&mut c, &request()).is_err());
        assert_eq!(
            w.lock()
                .unwrap()
                .iter()
                .filter(|s| s.as_str() == ":AUT .\n")
                .count(),
            1
        );
    }
    #[test]
    fn readbacks_ignore_status_and_timestamp_but_are_bounded() {
        let mut after = snapshot();
        after.trigger.status = "STOP".into();
        after.trigger.read_at_unix_ms = 999;
        assert!(same_settings(&snapshot(), &after));
        let mut frames = vec![snapshot()];
        for i in 1..=4 {
            let mut s = snapshot();
            s.scope.insert("CH1.offset".into(), i.to_string());
            frames.push(s);
        }
        let (mut c, w) = client(&frames);
        let reply = apply(&mut c, &request()).unwrap();
        assert!(!reply.readback_stable);
        assert_eq!(reply.readback_reads, 4);
        assert_eq!(
            w.lock()
                .unwrap()
                .iter()
                .filter(|s| !s.ends_with("?\n"))
                .count(),
            1
        );
    }
}
