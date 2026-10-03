"""macOS connection probe for OWON HDS25S / HDS200. No settings commands."""

import argparse
import json
from datetime import datetime, timezone
from pathlib import Path
import sys

from owon_protocol import ProtocolError, QueryClient, capture_stopped


def usb_modules():
    try:
        import usb.backend.libusb1
        import usb.core
        import usb.util
    except ImportError as exc:
        raise RuntimeError("PyUSB未導入です。.venv/bin/python -m pip install -r requirements.txt") from exc
    backend = usb.backend.libusb1.get_backend()
    if backend is None:
        candidates = (
            "/opt/homebrew/opt/libusb/lib/libusb-1.0.dylib",
            "/usr/local/opt/libusb/lib/libusb-1.0.dylib",
        )
        for candidate in candidates:
            if Path(candidate).is_file():
                backend = usb.backend.libusb1.get_backend(find_library=lambda _: candidate)
                if backend is not None:
                    break
    if backend is None:
        raise RuntimeError("libusbが見つかりません。Homebrewで brew install libusb を実行してください")
    return usb.core, usb.util, backend


def describe(device, util) -> dict:
    result = {
        "vid": f"0x{device.idVendor:04x}",
        "pid": f"0x{device.idProduct:04x}",
        "bus": device.bus,
        "address": device.address,
        "device_class": f"0x{device.bDeviceClass:02x}",
    }
    try:
        result["product"] = util.get_string(device, device.iProduct) if device.iProduct else None
        result["interfaces"] = [
            {
                "configuration": cfg.bConfigurationValue,
                "interface": interface.bInterfaceNumber,
                "alternate": interface.bAlternateSetting,
                "class": f"0x{interface.bInterfaceClass:02x}",
                "endpoints": [
                    {
                        "address": f"0x{ep.bEndpointAddress:02x}",
                        "type": ("control", "isochronous", "bulk", "interrupt")[ep.bmAttributes & 3],
                        "max_packet_size": ep.wMaxPacketSize,
                    }
                    for ep in interface
                ],
            }
            for cfg in device
            for interface in cfg
        ]
    except Exception as exc:
        result["descriptor_error"] = str(exc)
    return result


class BulkTransport:
    """Only claim a discovered Bulk pair. Do not reset or detach kernel drivers."""

    def __init__(self, device, core, util):
        self.device, self.core, self.util = device, core, util
        self.claimed = False
        try:
            cfg = device.get_active_configuration()
            pairs = []
            for interface in cfg:
                if interface.bAlternateSetting != 0:
                    continue
                inputs = [ep for ep in interface if ep.bmAttributes & 3 == 2 and ep.bEndpointAddress & 0x80]
                outputs = [ep for ep in interface if ep.bmAttributes & 3 == 2 and not ep.bEndpointAddress & 0x80]
                if len(inputs) == len(outputs) == 1:
                    pairs.append((interface.bInterfaceNumber, inputs[0], outputs[0]))
            if len(pairs) != 1:
                raise ProtocolError("Bulk IN/OUTを一意に選べません。devicesのUSB情報を確認してください")
            self.interface, self.input, self.output = pairs[0]
            util.claim_interface(device, self.interface)
            self.claimed = True
        except Exception:
            self.close()
            raise

    def write(self, data: bytes, timeout_ms: int) -> None:
        written = self.output.write(data, timeout=timeout_ms)
        if written != len(data):
            raise ProtocolError(f"Partial USB write: {written}/{len(data)} bytes")

    def read(self, size: int, timeout_ms: int) -> bytes:
        try:
            return bytes(self.input.read(size, timeout=timeout_ms))
        except self.core.USBTimeoutError as exc:
            raise TimeoutError("USB応答がタイムアウトしました。USBモードとケーブルを確認してください") from exc

    def close(self):
        try:
            if self.claimed:
                self.util.release_interface(self.device, self.interface)
        finally:
            self.claimed = False
            self.util.dispose_resources(self.device)


def positive_int(value: str) -> int:
    result = int(value, 0)
    if result <= 0:
        raise argparse.ArgumentTypeError("positive integer required")
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("devices", "probe", "capture"))
    parser.add_argument("--vid", type=lambda s: int(s, 0), default=0x5345)
    parser.add_argument("--pid", type=lambda s: int(s, 0), default=0x1234)
    parser.add_argument("--address", type=lambda s: int(s, 0), help="multiple devices: USB address from devices")
    parser.add_argument("--timeout-ms", type=positive_int, default=2000)
    parser.add_argument("--output", type=Path, help="capture JSON path; existing files are never overwritten")
    args = parser.parse_args()
    if args.output and args.action != "capture":
        parser.error("--output is only supported for capture")
    transport = None
    try:
        core, util, backend = usb_modules()
        if args.action == "devices":
            devices = list(core.find(find_all=True, backend=backend))
            descriptions = []
            for device in devices:
                try:
                    descriptions.append(describe(device, util))
                finally:
                    util.dispose_resources(device)
            print(json.dumps({"libusb_ready": True, "devices": descriptions}, ensure_ascii=False, indent=2))
            return 0
        matches = list(core.find(find_all=True, idVendor=args.vid, idProduct=args.pid, backend=backend))
        if args.address is not None:
            matches = [device for device in matches if device.address == args.address]
        if len(matches) != 1:
            raise RuntimeError(
                f"対象USBデバイス数: {len(matches)}。本体をHIDモードで接続し、devicesでVID/PIDを確認してください。"
                "複数ある場合は--addressを指定してください"
            )
        device = matches[0]
        transport = BulkTransport(device, core, util)
        client = QueryClient(transport, args.timeout_ms)
        if args.action == "probe":
            result = {
                "identity": client.text("*IDN?"),
                "trigger_status": client.text(":TRIGGER:STATUS?"),
                "header": client.header(),
                "usb": describe(device, util),
                "note": "metadata_only; waveform_format_not_verified",
            }
            print(json.dumps(result, ensure_ascii=False, indent=2))
        else:
            result = capture_stopped(client)
            captured_at = datetime.now(timezone.utc)
            result["retrieved_at_utc"] = captured_at.isoformat()
            result["retrieved_at_note"] = "host retrieval time, not waveform event time"
            output = args.output or Path("captures") / captured_at.strftime("capture-%Y%m%dT%H%M%S.%fZ.json")
            output.parent.mkdir(parents=True, exist_ok=True)
            with output.open("x", encoding="utf-8") as stream:
                json.dump(result, stream, ensure_ascii=False, indent=2)
                stream.write("\n")
            print(f"保存: {output.resolve()}")
            print("受信バイト数: " + ", ".join(f"{name}={ch['byte_count']}" for name, ch in result["channels"].items()))
        return 0
    except (RuntimeError, ValueError, OSError) as exc:
        print(f"エラー: {exc}", file=sys.stderr)
        return 1
    except KeyboardInterrupt:
        return 130
    finally:
        if transport is not None:
            try:
                transport.close()
            except OSError as exc:
                print(f"USB終了処理: {exc}", file=sys.stderr)


if __name__ == "__main__":
    raise SystemExit(main())
