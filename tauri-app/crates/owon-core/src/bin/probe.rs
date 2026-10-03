use owon_core::{
    protocol::Client,
    usb::{self, BulkTransport},
    waveform::LiveReader,
};
fn main() -> Result<(), String> {
    let devices = usb::devices()?;
    println!("{}", serde_json::to_string_pretty(&devices).unwrap());
    let settings = std::env::args().any(|a| a == "--settings");
    let measures = std::env::args().any(|a| a == "--measurements");
    if settings || measures || std::env::args().any(|a| a == "--read") {
        if devices.len() != 1 {
            return Err("実機を1台だけ接続してください".into());
        }
        let d = &devices[0];
        let mut client = Client::new(BulkTransport::open(d.bus, d.address)?);
        let id = client.text("*IDN?")?;
        println!("{id}");
        if measures {
            for ch in ["CH1", "CH2"] {
                for item in ["FREQUENCY", "PERIOD", "PKPK", "MAX", "MIN", "AVERAGE"] {
                    let command = format!(":MEASUREMENT:{ch}:{item}?");
                    println!("{command} => {}", client.text(&command)?);
                }
            }
            return Ok(());
        }
        if settings {
            println!("header={}", client.header()?);
            for command in [
                ":CH1:DISPLAY?",
                ":CH1:COUPLING?",
                ":CH1:PROBE?",
                ":CH1:SCALE?",
                ":CH1:OFFSET?",
                ":CH2:DISPLAY?",
                ":CH2:COUPLING?",
                ":CH2:PROBE?",
                ":CH2:SCALE?",
                ":CH2:OFFSET?",
                ":HORIZONTAL:SCALE?",
                ":HORIZONTAL:OFFSET?",
                ":ACQUIRE:MODE?",
                ":ACQUIRE:DEPMEM?",
            ] {
                println!("{command} => {}", client.text(command)?);
            }
            let current = client.text(":CH1:COUPLING?")?;
            let reply = owon_core::control::apply(
                &mut client,
                &owon_core::control::Setting {
                    target: "CH1".into(),
                    parameter: "coupling".into(),
                    value: current,
                },
            )?;
            println!("same-value coupling write: {reply:?}");
            return Ok(());
        }
        let mut reader = LiveReader::new(id);
        let mut elapsed = Vec::new();
        for _ in 0..40 {
            let frame = reader.read(&mut client)?;
            elapsed.push(frame.query_ms);
            if frame.sequence == 1 {
                println!(
                    "status={} bytes={:?}",
                    frame.record["status_after"],
                    frame
                        .values
                        .iter()
                        .map(|(n, v)| (n, v.len()))
                        .collect::<Vec<_>>()
                );
            }
        }
        println!(
            "40 reads: mean {:.2} ms, {:.2} reads/s",
            elapsed.iter().sum::<f64>() / 40.0,
            40000.0 / elapsed.iter().sum::<f64>()
        );
    }
    Ok(())
}
