//! Typed, allowlisted scope-input settings. No AWG output, reset, arbitrary SCPI or undocumented RUN command.
use crate::{
    protocol::{Client, Transport},
    Result,
};
use serde::{Deserialize, Serialize};

pub const TIMEBASES: &[&str] = &[
    "5.0ns", "10.0ns", "20.0ns", "50.0ns", "100ns", "200ns", "500ns", "1.0us", "2.0us", "5.0us",
    "10us", "20us", "50us", "100us", "200us", "500us", "1.0ms", "2.0ms", "5.0ms", "10ms", "20ms",
    "50ms", "100ms", "200ms", "500ms", "1.0s", "2.0s", "5.0s", "10s", "20s", "50s", "100s", "200s",
    "500s", "1000s",
];
pub const SCALES: &[&str] = &[
    "10.0mV", "20.0mV", "50.0mV", "100mV", "200mV", "500mV", "1.00V", "2.00V", "5.00V", "10.0V",
    "20.0V", "50.0V", "100V", "200V", "500V", "4.00V",
];

#[derive(Clone, Deserialize, Debug)]
pub struct Setting {
    pub target: String,
    pub parameter: String,
    pub value: String,
}
pub struct Command {
    pub(crate) set: String,
    pub(crate) query: String,
    pub(crate) expected: String,
}
#[derive(Serialize, Debug)]
pub struct SettingReply {
    pub command: String,
    pub before: String,
    pub after: String,
    pub verified: bool,
}

pub fn compile(s: &Setting) -> Result<Command> {
    let value = s.value.trim();
    if value.is_empty() || !value.is_ascii() || value.contains([';', '\n', '\r', '?', ':']) {
        return Err("設定値が不正です".into());
    }
    let upper = value.to_ascii_uppercase();
    let (path, valid) = match (s.target.as_str(), s.parameter.as_str()) {
        ("CH1" | "CH2", "display") => (
            format!(":{}:DISPLAY", s.target),
            ["ON", "OFF"].contains(&upper.as_str()),
        ),
        ("CH1" | "CH2", "coupling") => (
            format!(":{}:COUPLING", s.target),
            ["AC", "DC", "GND"].contains(&upper.as_str()),
        ),
        ("CH1" | "CH2", "probe") => (
            format!(":{}:PROBE", s.target),
            ["1X", "10X", "20X", "100X", "1000X"].contains(&upper.as_str()),
        ),
        ("CH1" | "CH2", "scale") => (
            format!(":{}:SCALE", s.target),
            SCALES.iter().any(|v| equivalent(v, value)),
        ),
        ("CH1" | "CH2", "offset") => (
            format!(":{}:OFFSET", s.target),
            decimal(value, -200.0, 200.0),
        ),
        ("horizontal", "scale") => (
            ":HORIZONTAL:SCALE".into(),
            TIMEBASES.iter().any(|v| equivalent(v, value)),
        ),
        ("horizontal", "offset") => (":HORIZONTAL:OFFSET".into(), decimal(value, -10.0, 10.0)),
        ("acquire", "mode") => (
            ":ACQUIRE:MODE".into(),
            ["SAMPLE", "PEAK"].contains(&upper.as_str()),
        ),
        ("acquire", "memory") => (
            ":ACQUIRE:DEPMEM".into(),
            ["4K", "8K"].contains(&upper.as_str()),
        ),
        ("trigger", "source") => (
            ":TRIGGER:SOURCE".into(),
            ["CH1", "CH2"].contains(&upper.as_str()),
        ),
        ("trigger", "coupling") => (
            ":TRIGGER:COUPLING".into(),
            ["AC", "DC"].contains(&upper.as_str()),
        ),
        ("trigger", "edge") => (
            ":TRIGGER:SINGLE:EDGE".into(),
            ["RISE", "FALL"].contains(&upper.as_str()),
        ),
        ("trigger", "sweep") => (
            ":TRIGGER:SWEEP".into(),
            ["AUTO", "NORMAL", "SINGLE"].contains(&upper.as_str()),
        ),
        ("trigger", "level") => (
            ":TRIGGER:SINGLE:EDGE:LEVEL".into(),
            voltage(value).is_some_and(|v| v.abs() <= 100.0),
        ),
        _ => return Err("未対応の本体設定です".into()),
    };
    if !valid {
        return Err("設定値が許可範囲外です。時間軸・感度は一覧から選択してください".into());
    }
    Ok(Command {
        set: format!("{path} {value}"),
        query: format!("{path}?"),
        expected: value.into(),
    })
}
fn decimal(s: &str, min: f64, max: f64) -> bool {
    s.parse::<f64>()
        .is_ok_and(|v| v.is_finite() && (min..=max).contains(&v))
}
pub(crate) fn voltage(s: &str) -> Option<f64> {
    quantity(
        s,
        &[
            ("PV", 1e-12),
            ("NV", 1e-9),
            ("UV", 1e-6),
            ("MV", 1e-3),
            ("V", 1.0),
        ],
    )
}
fn quantity(s: &str, units: &[(&str, f64)]) -> Option<f64> {
    let s = s.trim().to_ascii_uppercase().replace(' ', "");
    for (unit, multiplier) in units {
        if let Some(number) = s.strip_suffix(unit) {
            return number
                .parse::<f64>()
                .ok()
                .filter(|v| v.is_finite())
                .map(|v| v * multiplier);
        }
    }
    s.parse::<f64>().ok().filter(|v| v.is_finite())
}
pub fn equivalent(a: &str, b: &str) -> bool {
    let norm = |s: &str| match s.trim().to_ascii_uppercase().as_str() {
        "SAMP" => "SAMPLE".into(),
        "NORM" => "NORMAL".into(),
        "SING" => "SINGLE".into(),
        other => other.replace(' ', ""),
    };
    if norm(a) == norm(b) {
        return true;
    }
    let parsed = |s: &str| {
        let upper = s.trim().to_ascii_uppercase();
        // Unitless instrument readbacks inherit the requested dimension; explicit
        // seconds and volts must never compare equal just because numbers match.
        let dimension = if upper.ends_with('S') {
            1
        } else if upper.ends_with('V') {
            2
        } else {
            0
        };
        quantity(
            s,
            &[
                ("NS", 1e-9),
                ("US", 1e-6),
                ("MS", 1e-3),
                ("S", 1.0),
                ("PV", 1e-12),
                ("NV", 1e-9),
                ("UV", 1e-6),
                ("MV", 1e-3),
                ("V", 1.0),
            ],
        )
        .map(|value| (value, dimension))
    };
    match (parsed(a), parsed(b)) {
        (Some((a, da)), Some((b, db))) if da == 0 || db == 0 || da == db => {
            (a - b).abs() <= 1e-9 * a.abs().max(b.abs()).max(1e-12)
        }
        _ => false,
    }
}
pub fn query_allowed(s: &str) -> bool {
    let mut valid = vec![
        ":HORIZONTAL:SCALE?",
        ":HORIZONTAL:OFFSET?",
        ":ACQUIRE:MODE?",
        ":ACQUIRE:DEPMEM?",
        ":TRIGGER:SOURCE?",
        ":TRIGGER:COUPLING?",
        ":TRIGGER:SINGLE:EDGE?",
        ":TRIGGER:SINGLE:EDGE:LEVEL?",
        ":TRIGGER:SWEEP?",
    ];
    let mut dynamic = Vec::new();
    for ch in ["CH1", "CH2"] {
        for p in ["DISPLAY", "COUPLING", "PROBE", "SCALE", "OFFSET"] {
            dynamic.push(format!(":{ch}:{p}?"));
        }
        for p in ["FREQUENCY", "PERIOD", "PKPK", "MAX", "MIN", "AVERAGE"] {
            dynamic.push(format!(":MEASUREMENT:{ch}:{p}?"));
        }
    }
    valid.extend(dynamic.iter().map(String::as_str));
    valid.contains(&s)
}
pub fn apply<T: Transport>(client: &mut Client<T>, setting: &Setting) -> Result<SettingReply> {
    let command = compile(setting)?;
    let before = client.text(&command.query)?;
    client.write_setting(&command)?;
    let after = client.text(&command.query)?;
    let verified = equivalent(&after, &command.expected);
    Ok(SettingReply {
        command: command.set,
        before,
        after,
        verified,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_injection_and_unknown_operations() {
        for value in ["ON;:CHANNEL ON", "ON\n*RST", "NaN", "", "1kV"] {
            assert!(compile(&Setting {
                target: "CH1".into(),
                parameter: "display".into(),
                value: value.into()
            })
            .is_err());
        }
        assert!(compile(&Setting {
            target: "awg".into(),
            parameter: "output".into(),
            value: "ON".into()
        })
        .is_err());
    }
    #[test]
    fn offsets_and_timebases_are_bounded() {
        assert!(compile(&Setting {
            target: "CH2".into(),
            parameter: "offset".into(),
            value: "201".into()
        })
        .is_err());
        assert!(compile(&Setting {
            target: "horizontal".into(),
            parameter: "scale".into(),
            value: "1.0ms".into()
        })
        .is_ok());
        assert!(compile(&Setting {
            target: "trigger".into(),
            parameter: "level".into(),
            value: "-25mV".into()
        })
        .is_ok());
    }
    #[test]
    fn readback_normalizes_units_and_abbreviations() {
        assert!(equivalent("1.00 V", "1000mV"));
        assert!(equivalent("SAMP", "SAMPLE"));
        assert!(equivalent("0.001s", "1ms"));
        assert!(!equivalent("2V", "1V"));
        assert!(equivalent("2e+00", "2V"));
        assert!(!equivalent("1ms", "1mV"));
    }
    #[test]
    fn only_known_queries_allowed() {
        assert!(query_allowed(":MEASUREMENT:CH2:PKPK?"));
        assert!(!query_allowed(":FUNCtion?"));
    }
    #[test]
    fn accepts_observed_probe_and_scale_but_keeps_values_bounded() {
        for (target, parameter, value) in [
            ("CH1", "probe", "20X"),
            ("CH1", "scale", "4.00V"),
            ("CH2", "offset", "-2.0001"),
            ("horizontal", "offset", "0.00"),
        ] {
            assert!(compile(&Setting {
                target: target.into(),
                parameter: parameter.into(),
                value: value.into()
            })
            .is_ok());
        }
        for (parameter, value) in [
            ("probe", "999X"),
            ("scale", "NaN"),
            ("scale", "4ms"),
            ("offset", "-200.0001"),
            ("offset", "inf"),
        ] {
            assert!(compile(&Setting {
                target: "CH1".into(),
                parameter: parameter.into(),
                value: value.into()
            })
            .is_err());
        }
    }
    #[test]
    fn setting_is_serial_read_write_read_and_mismatch_is_reported() {
        use std::{collections::VecDeque, time::Duration};
        struct Fake {
            responses: VecDeque<Vec<u8>>,
            commands: Vec<String>,
        }
        impl Transport for Fake {
            fn write(&mut self, data: &[u8], _: Duration) -> Result<()> {
                self.commands
                    .push(String::from_utf8(data.to_vec()).unwrap());
                Ok(())
            }
            fn read(&mut self, _: Duration) -> Result<Vec<u8>> {
                self.responses.pop_front().ok_or("timeout".into())
            }
        }
        let mut client = Client::new(Fake {
            responses: [
                b"DC\n".to_vec(),
                b"AC\n".to_vec(),
                b"AC\n".to_vec(),
                b"AC\n".to_vec(),
            ]
            .into(),
            commands: Vec::new(),
        });
        let reply = apply(
            &mut client,
            &Setting {
                target: "CH1".into(),
                parameter: "coupling".into(),
                value: "AC".into(),
            },
        )
        .unwrap();
        assert_eq!(
            (reply.before.as_str(), reply.after.as_str(), reply.verified),
            ("DC", "AC", true)
        );
        let reply = apply(
            &mut client,
            &Setting {
                target: "CH1".into(),
                parameter: "coupling".into(),
                value: "DC".into(),
            },
        )
        .unwrap();
        assert!(!reply.verified);
        assert!(!client.is_failed());
    }
}
