use crate::{protocol::Transport, Result};
use rusb::{Context, DeviceHandle, Direction, TransferType, UsbContext};
use serde::Serialize;
use std::time::Duration;

#[derive(Clone, Serialize)]
pub struct DeviceInfo {
    pub bus: u8,
    pub address: u8,
    pub label: String,
}
pub fn devices() -> Result<Vec<DeviceInfo>> {
    let context = Context::new().map_err(|e| e.to_string())?;
    let mut result = Vec::new();
    for device in context.devices().map_err(|e| e.to_string())?.iter() {
        let Ok(desc) = device.device_descriptor() else {
            continue;
        };
        if desc.vendor_id() == 0x5345 && desc.product_id() == 0x1234 {
            result.push(DeviceInfo {
                bus: device.bus_number(),
                address: device.address(),
                label: format!(
                    "OWON 5345:1234 · bus {} / {} · {:?}",
                    device.bus_number(),
                    device.address(),
                    device.speed()
                ),
            });
        }
    }
    Ok(result)
}

pub struct BulkTransport {
    handle: DeviceHandle<Context>,
    interface: u8,
    input: u8,
    output: u8,
}
impl BulkTransport {
    pub fn open(bus: u8, address: u8) -> Result<Self> {
        let context = Context::new().map_err(|e| e.to_string())?;
        let list = context.devices().map_err(|e| e.to_string())?;
        let device = list
            .iter()
            .find(|d| d.bus_number() == bus && d.address() == address)
            .ok_or("USBデバイスがありません")?;
        let desc = device.device_descriptor().map_err(|e| e.to_string())?;
        if desc.vendor_id() != 0x5345 || desc.product_id() != 0x1234 {
            return Err("指定先がOWONデバイスではありません".into());
        }
        let config = device
            .active_config_descriptor()
            .map_err(|e| format!("USB構成: {e}"))?;
        let mut pairs = Vec::new();
        for interface in config.interfaces() {
            for alternate in interface.descriptors().filter(|d| d.setting_number() == 0) {
                let endpoints: Vec<_> = alternate
                    .endpoint_descriptors()
                    .filter(|e| e.transfer_type() == TransferType::Bulk)
                    .collect();
                let inputs: Vec<_> = endpoints
                    .iter()
                    .filter(|e| e.direction() == Direction::In)
                    .collect();
                let outputs: Vec<_> = endpoints
                    .iter()
                    .filter(|e| e.direction() == Direction::Out)
                    .collect();
                if inputs.len() == 1 && outputs.len() == 1 {
                    pairs.push((
                        alternate.interface_number(),
                        inputs[0].address(),
                        outputs[0].address(),
                    ));
                }
            }
        }
        if pairs.len() != 1 {
            return Err("Bulk IN/OUTの組が一意ではありません".into());
        }
        let (interface, input, output) = pairs[0];
        let handle = device.open().map_err(|e| {
            format!("USBを開けません: {e}（他のアプリの接続・OS権限・ドライバーを確認）")
        })?;
        handle
            .claim_interface(interface)
            .map_err(|e| format!("USBを使用できません: {e}（Python版などを切断してください）"))?;
        // Never reset, change USB configuration, or detach an OS driver.
        Ok(Self {
            handle,
            interface,
            input,
            output,
        })
    }
}
impl Transport for BulkTransport {
    fn write(&mut self, data: &[u8], timeout: Duration) -> Result<()> {
        let size = self
            .handle
            .write_bulk(self.output, data, timeout)
            .map_err(|e| format!("USB送信: {e}"))?;
        if size != data.len() {
            return Err("USB送信が途中で終わりました".into());
        }
        Ok(())
    }
    fn read(&mut self, timeout: Duration) -> Result<Vec<u8>> {
        let mut buffer = [0u8; 4096];
        let size = self
            .handle
            .read_bulk(self.input, &mut buffer, timeout)
            .map_err(|e| format!("USB受信: {e}"))?;
        Ok(buffer[..size].to_vec())
    }
}
impl Drop for BulkTransport {
    fn drop(&mut self) {
        let _ = self.handle.release_interface(self.interface);
    }
}
