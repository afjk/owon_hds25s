"""Read-only HDS200 SCPI queries, independent of the USB transport."""

import json
import struct
import time
from typing import Protocol


QUERIES = {
    "*IDN?": False,
    ":TRIGGER:STATUS?": False,
    ":DATA:WAVE:SCREEN:HEAD?": True,
    ":DATA:WAVE:SCREEN:CH1?": True,
    ":DATA:WAVE:SCREEN:CH2?": True,
}
MAX_PAYLOAD = 1024 * 1024


class ProtocolError(RuntimeError):
    pass


class Transport(Protocol):
    def write(self, data: bytes, timeout_ms: int) -> None: ...

    def read(self, size: int, timeout_ms: int) -> bytes: ...


class QueryClient:
    """One outstanding request; a framing failure invalidates this session."""

    def __init__(self, transport: Transport, timeout_ms: int = 2000):
        if timeout_ms <= 0:
            raise ValueError("timeout_ms must be positive")
        self.transport = transport
        self.timeout_ms = timeout_ms
        self.failed = False

    def query(self, command: str) -> bytes:
        command = command.upper()
        if command not in QUERIES:
            raise ValueError("This diagnostic client only permits known read-only queries")
        if self.failed:
            raise ProtocolError("Session invalid after a failed query; reconnect the device")
        try:
            deadline = time.monotonic() + self.timeout_ms / 1000
            self.transport.write((command + "\n").encode("ascii"), self.timeout_ms)
            return self._receive(QUERIES[command], deadline)
        except Exception:
            self.failed = True
            raise

    def _receive(self, framed: bool, deadline: float) -> bytes:
        data = bytearray()
        expected = None
        while True:
            remaining_ms = int((deadline - time.monotonic()) * 1000)
            if remaining_ms <= 0:
                raise TimeoutError(f"Response incomplete: received {len(data)} bytes")
            # Large reads prevent overflow when a USB transfer contains header + payload.
            chunk = self.transport.read(4096, remaining_ms)
            if not chunk:
                raise TimeoutError(f"Empty response: received {len(data)} bytes")
            data.extend(chunk)
            if len(data) > MAX_PAYLOAD + 6:
                raise ProtocolError("Response exceeds diagnostic size limit")
            if framed:
                if expected is None and len(data) >= 4:
                    expected = struct.unpack_from("<I", data)[0]
                    if not 0 < expected <= MAX_PAYLOAD:
                        raise ProtocolError(f"Invalid little-endian payload length: {expected}")
                if expected is not None and len(data) >= expected + 4:
                    tail = bytes(data[expected + 4 :])
                    if tail not in (b"", b"\n", b"\r\n"):
                        raise ProtocolError("Unexpected bytes after binary response")
                    return bytes(data[4 : expected + 4])
            else:
                newline = data.find(b"\n")
                if newline >= 0:
                    if newline != len(data) - 1:
                        raise ProtocolError("Unexpected bytes after text response")
                    return bytes(data[:newline]).removesuffix(b"\r")
                if len(data) > 4096:
                    raise ProtocolError("Text response too long or missing newline")

    def text(self, command: str) -> str:
        return self.query(command).decode("ascii").strip()

    def header(self) -> dict:
        head = json.loads(self.query(":DATA:WAVE:SCREEN:HEAD?"))
        if not isinstance(head, dict):
            raise ProtocolError("Waveform header must be a JSON object")
        return head


def field(mapping: dict, name: str):
    """Vendor examples and firmware use different JSON key casing."""
    for key, value in mapping.items():
        if key.casefold() == name.casefold():
            return value
    raise ProtocolError(f"Missing waveform header field: {name}")


def capture_stopped(client: QueryClient) -> dict:
    """Preserve screen bytes; decoding and time calibration need the actual firmware."""
    identity = client.text("*IDN?")
    if "HDS" not in identity.upper():
        raise ProtocolError(f"Device is not identified as an HDS oscilloscope: {identity!r}")
    status_before = client.text(":TRIGGER:STATUS?")
    if status_before.upper() != "STOP":
        raise ProtocolError("本体のRUN/STOPで収録を停止してからcaptureを実行してください")
    head_before = client.header()
    channels = field(head_before, "CHANNEL")
    if not isinstance(channels, list) or not all(isinstance(ch, dict) for ch in channels):
        raise ProtocolError("Invalid CHANNEL metadata")
    displays = {
        str(field(ch, "NAME")).upper(): str(field(ch, "DISPLAY")).upper()
        for ch in channels
    }
    if any(displays.get(name) != "ON" for name in ("CH1", "CH2")):
        raise ProtocolError("本体でCH1とCH2を両方ONにしてください")
    raw = {name: client.query(f":DATA:WAVE:SCREEN:{name}?") for name in ("CH1", "CH2")}
    head_after = client.header()
    status_after = client.text(":TRIGGER:STATUS?")
    if status_after.upper() != "STOP" or head_before != head_after:
        raise ProtocolError("取得中に状態が変わりました。停止状態を維持して再取得してください")
    if len(raw["CH1"]) != len(raw["CH2"]):
        raise ProtocolError("Channel payload lengths differ; retain diagnostics and investigate")
    return {
        "schema_version": 1,
        "source": "owon_hds200_screen",
        "identity": identity,
        "status_before": status_before,
        "status_after": status_after,
        "consistency": "manual_stop_observed; no acquisition_id_available",
        "header": head_before,
        "channels": {
            name: {"byte_count": len(data), "payload_hex": data.hex()}
            for name, data in raw.items()
        },
        "calibration": "sample_encoding_and_time_axis_not_yet_verified",
    }
