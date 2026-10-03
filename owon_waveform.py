"""Raw screen-waveform model. No unverified voltage or time conversion."""

from copy import deepcopy
from dataclasses import dataclass
from datetime import datetime, timezone
import json
from pathlib import Path
import struct
import time

from owon_protocol import MAX_PAYLOAD, ProtocolError, field


def optional_field(mapping, name, default="—"):
    if not isinstance(mapping, dict):
        return default
    try:
        return field(mapping, name)
    except ProtocolError:
        return default


def channel_metadata(header):
    channels = field(header, "CHANNEL")
    if not isinstance(channels, list) or not all(isinstance(item, dict) for item in channels):
        raise ProtocolError("CHANNEL情報が不正です")
    result = {}
    for item in channels:
        name = str(field(item, "NAME")).upper()
        if name in ("CH1", "CH2"):
            if name in result:
                raise ProtocolError(f"{name}の情報が重複しています")
            result[name] = item
    return result


@dataclass(frozen=True)
class ScreenFrame:
    record: dict
    payloads: dict[str, bytes]
    query_ms: float = 0.0
    received_monotonic: float = 0.0
    metadata_age_ms: float = 0.0
    metadata_updated: bool = True
    sequence: int = 0

    @property
    def header(self):
        return self.record["header"]

    @property
    def identity(self):
        return self.record["identity"]

    @property
    def status(self):
        return self.record.get("status_after", "—")

    @property
    def is_stopped_capture(self):
        return self.record.get("consistency") == "manual_stop_observed; no acquisition_id_available"

    def counts(self, name):
        """One signed display value per byte, NOT a calibrated ADC sample."""
        raw = self.payloads.get(name, b"")
        return struct.unpack(f"<{len(raw)}b", raw)

    @classmethod
    def from_record(cls, record, query_ms=0.0):
        if not isinstance(record, dict):
            raise ValueError("保存データはJSONオブジェクトである必要があります")
        if type(record.get("schema_version")) is not int or record["schema_version"] != 1 or record.get("source") != "owon_hds200_screen":
            raise ValueError("このアプリのHDS200保存形式ではありません")
        if not isinstance(record.get("identity"), str) or not isinstance(record.get("header"), dict):
            raise ValueError("機種情報または波形ヘッダーがありません")
        if record.get("calibration") != "sample_encoding_and_time_axis_not_yet_verified":
            raise ValueError("未対応の波形校正形式です")
        assignments = record.get("sensor_assignments")
        if assignments is not None and (
            not isinstance(assignments, dict)
            or set(assignments) != {"CH1", "CH2"}
            or not all(isinstance(value, str) for value in assignments.values())
            or set(assignments.values()) != {"audio", "light"}
        ):
            raise ValueError("光・音の入力割り当てが不正です")
        metadata = channel_metadata(record["header"])
        channels = record.get("channels")
        if not isinstance(channels, dict) or not channels or set(channels) - {"CH1", "CH2"}:
            raise ValueError("チャンネルの保存情報が不正です")
        payloads = {}
        for name, entry in channels.items():
            if name not in metadata or str(optional_field(metadata[name], "DISPLAY")).upper() != "ON":
                raise ValueError(f"{name}のヘッダーと受信データが一致しません")
            if not isinstance(entry, dict) or not isinstance(entry.get("payload_hex"), str):
                raise ValueError(f"{name}の受信データがありません")
            count = entry.get("byte_count")
            if type(count) is not int or not 0 < count <= MAX_PAYLOAD:
                raise ValueError(f"{name}のバイト数が不正です")
            encoded = entry["payload_hex"]
            if len(encoded) != count * 2:
                raise ValueError(f"{name}のバイト数とhexの長さが一致しません")
            raw = bytes.fromhex(encoded)
            if len(raw) != count:
                raise ValueError(f"{name}のバイト数が一致しません")
            payloads[name] = raw
        if record.get("consistency") == "manual_stop_observed; no acquisition_id_available":
            if any(str(record.get(key, "")).upper() != "STOP" for key in ("status_before", "status_after")):
                raise ValueError("停止波形の保存情報に矛盾があります")
            if set(payloads) != {"CH1", "CH2"} or len(payloads["CH1"]) != len(payloads["CH2"]):
                raise ValueError("停止波形には同じ長さのCH1/CH2データが必要です")
        return cls(deepcopy(record), payloads, query_ms)


def stamp_record(record):
    result = deepcopy(record)
    result["retrieved_at_utc"] = datetime.now(timezone.utc).isoformat()
    result["retrieved_at_note"] = "host retrieval time, not waveform event time"
    return result


class LiveWaveformReader:
    """Cache display metadata only. Never use this cache for a stopped capture."""

    def __init__(self, client, identity, metadata_interval=1.0, clock=time.monotonic):
        if metadata_interval < 0:
            raise ValueError("metadata_interval must not be negative")
        self.client = client
        self.identity = identity
        self.metadata_interval = metadata_interval
        self.clock = clock
        self.invalidate()

    def invalidate(self):
        self.header = None
        self.metadata_at = 0.0

    def read(self):
        started = self.clock()
        updated = self.header is None or started - self.metadata_at >= self.metadata_interval
        if updated:
            status = self.client.text(":TRIGGER:STATUS?")
            header = self.client.header()
            metadata = channel_metadata(header)
            self.status, self.header, self.metadata = status, header, metadata
            self.metadata_at = self.clock()
        payloads = {
            name: self.client.query(f":DATA:WAVE:SCREEN:{name}?")
            for name, info in self.metadata.items()
            if str(optional_field(info, "DISPLAY")).upper() == "ON"
        }
        received_at = self.clock()
        age_ms = (received_at - self.metadata_at) * 1000
        # This metadata is for preview; it may lag a physical setting change.
        record = stamp_record({
            "schema_version": 1,
            "source": "owon_hds200_screen",
            "identity": self.identity,
            "status_before": self.status,
            "status_after": self.status,
            "consistency": "live_sequential_queries; channel_acquisition_alignment_unverified",
            "header": self.header,
            "metadata_age_ms": round(age_ms, 3),
            "metadata_refresh_interval_ms": self.metadata_interval * 1000,
            "channels": {
                name: {"byte_count": len(raw), "payload_hex": raw.hex()}
                for name, raw in payloads.items()
            },
            "calibration": "sample_encoding_and_time_axis_not_yet_verified",
        })
        return ScreenFrame(
            record, payloads, (received_at - started) * 1000,
            received_at, age_ms, updated,
        )


def read_live_frame(client, identity):
    """Compatibility helper: fully refresh metadata for this one read."""
    return LiveWaveformReader(client, identity, metadata_interval=0).read()


def load_capture(path):
    path = Path(path)
    if path.stat().st_size > 5 * MAX_PAYLOAD:
        raise ValueError("保存ファイルが大きすぎます（最大5 MiB）")
    with path.open(encoding="utf-8") as stream:
        frame = ScreenFrame.from_record(json.load(stream))
    if not frame.is_stopped_capture:
        raise ValueError("停止状態を確認した保存波形ではありません")
    return frame


def save_capture(frame, path):
    if not frame.is_stopped_capture:
        raise ValueError("ライブ表示を測定用の停止波形として保存できません")
    # Validate again before writing; never overwrite a previous measurement.
    ScreenFrame.from_record(frame.record)
    serialized = json.dumps(frame.record, ensure_ascii=False, indent=2) + "\n"
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("x", encoding="utf-8") as stream:
        stream.write(serialized)
    return path.resolve()
