from pathlib import Path
import tempfile
import unittest

from owon_waveform import LiveWaveformReader, ScreenFrame, load_capture, read_live_frame, save_capture, stamp_record


def stopped_record():
    return stamp_record({
        "schema_version": 1,
        "source": "owon_hds200_screen",
        "identity": "OWON,HDS25S,test,1.0",
        "status_before": "STOP",
        "status_after": "STOP",
        "consistency": "manual_stop_observed; no acquisition_id_available",
        "calibration": "sample_encoding_and_time_axis_not_yet_verified",
        "sensor_assignments": {"CH1": "audio", "CH2": "light"},
        "header": {"CHANNEL": [{"NAME": "CH1", "DISPLAY": "ON"}, {"NAME": "CH2", "DISPLAY": "ON"}]},
        "channels": {
            "CH1": {"byte_count": 4, "payload_hex": "007f80ff"},
            "CH2": {"byte_count": 4, "payload_hex": "0a0d0102"},
        },
    })


class FakeClient:
    failed = False

    def __init__(self, status="AUTO", enabled=("CH1", "CH2")):
        self.status = status
        self.enabled = enabled
        self.commands = []

    def text(self, command):
        self.commands.append(command)
        return "OWON,HDS25S,test,1.0" if command == "*IDN?" else self.status

    def header(self):
        self.commands.append(":DATA:WAVE:SCREEN:HEAD?")
        return {
            "CHANNEL": [{"NAME": name, "DISPLAY": "ON" if name in self.enabled else "OFF"} for name in ("CH1", "CH2")],
            "TIMEBASE": {"SCALE": "1.0ms"},
            "SAMPLE": {"SAMPLERATE": "250KSa/s"},
        }

    def query(self, command):
        self.commands.append(command)
        return bytes.fromhex("007f80ff") if "CH1?" in command else bytes.fromhex("0a0d0102")


class WaveformTests(unittest.TestCase):
    def test_signed_values_preserve_every_received_byte(self):
        record = stopped_record()
        frame = ScreenFrame.from_record(record)
        self.assertEqual(frame.counts("CH1"), (0, 127, -128, -1))
        self.assertEqual(frame.counts("CH2"), (10, 13, 1, 2))
        record["channels"]["CH1"]["payload_hex"] = "00000000"
        self.assertEqual(frame.record["channels"]["CH1"]["payload_hex"], "007f80ff")

    def test_save_load_roundtrip_preserves_raw_header_and_sensor_mapping(self):
        frame = ScreenFrame.from_record(stopped_record())
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "capture.json"
            save_capture(frame, path)
            loaded = load_capture(path)
            self.assertEqual(loaded.record, frame.record)
            self.assertEqual(loaded.payloads, frame.payloads)
            with self.assertRaises(FileExistsError):
                save_capture(frame, path)
            self.assertEqual(load_capture(path).record, frame.record)

    def test_corrupt_records_are_rejected(self):
        variations = []
        bad = stopped_record()
        bad["channels"]["CH1"]["byte_count"] = 5
        variations.append(bad)
        bad = stopped_record()
        bad["channels"]["CH2"]["payload_hex"] = "not-hex!"
        variations.append(bad)
        bad = stopped_record()
        bad["status_after"] = "AUTO"
        variations.append(bad)
        bad = stopped_record()
        bad["sensor_assignments"]["CH2"] = "audio"
        variations.append(bad)
        bad = stopped_record()
        bad["sensor_assignments"]["CH2"] = ["light"]
        variations.append(bad)
        bad = stopped_record()
        bad["header"]["CHANNEL"][0]["DISPLAY"] = "OFF"
        variations.append(bad)
        for record in variations:
            with self.subTest(record=record), self.assertRaises(ValueError):
                ScreenFrame.from_record(record)

    def test_live_reads_only_enabled_channels_and_cannot_be_saved_as_measurement(self):
        client = FakeClient(enabled=("CH2",))
        frame = read_live_frame(client, "OWON,HDS25S,test,1.0")
        self.assertEqual(set(frame.payloads), {"CH2"})
        self.assertNotIn(":DATA:WAVE:SCREEN:CH1?", client.commands)
        self.assertFalse(frame.is_stopped_capture)
        with self.assertRaises(ValueError):
            save_capture(frame, "/path/that/must/not/be/written.json")

    def test_both_off_is_empty_not_a_synthetic_signal(self):
        frame = read_live_frame(FakeClient(enabled=()), "OWON,HDS25S,test,1.0")
        self.assertEqual(frame.payloads, {})
        self.assertEqual(frame.counts("CH1"), ())

    def test_fast_preview_reads_metadata_once_then_only_waveforms(self):
        clock = [100.0]
        client = FakeClient()
        reader = LiveWaveformReader(client, "OWON,HDS25S,test,1.0", clock=lambda: clock[0])
        first = reader.read()
        self.assertTrue(first.metadata_updated)
        client.commands.clear()
        clock[0] += 0.5
        second = reader.read()
        self.assertEqual(client.commands, [":DATA:WAVE:SCREEN:CH1?", ":DATA:WAVE:SCREEN:CH2?"])
        self.assertFalse(second.metadata_updated)
        self.assertEqual(second.metadata_age_ms, 500)
        self.assertEqual(second.record["metadata_refresh_interval_ms"], 1000)
        self.assertEqual(first.record["metadata_age_ms"], 0)
        self.assertFalse(second.is_stopped_capture)

    def test_metadata_timer_refreshes_status_and_enabled_channels(self):
        clock = [100.0]
        client = FakeClient()
        reader = LiveWaveformReader(client, "OWON,HDS25S,test,1.0", clock=lambda: clock[0])
        first = reader.read()
        client.enabled = ("CH2",)
        client.status = "STOP"
        client.commands.clear()
        clock[0] += 1.01
        updated = reader.read()
        self.assertEqual(client.commands, [":TRIGGER:STATUS?", ":DATA:WAVE:SCREEN:HEAD?", ":DATA:WAVE:SCREEN:CH2?"])
        self.assertEqual(set(updated.payloads), {"CH2"})
        self.assertEqual(updated.status, "STOP")
        self.assertEqual(first.status, "AUTO")
        self.assertEqual(first.header["CHANNEL"][0]["DISPLAY"], "ON")
        self.assertFalse(updated.is_stopped_capture)

    def test_invalidating_preview_cache_forces_immediate_metadata_refresh(self):
        clock = [100.0]
        client = FakeClient()
        reader = LiveWaveformReader(client, "OWON,HDS25S,test,1.0", clock=lambda: clock[0])
        reader.read()
        client.commands.clear()
        reader.invalidate()
        reader.read()
        self.assertEqual(client.commands[:2], [":TRIGGER:STATUS?", ":DATA:WAVE:SCREEN:HEAD?"])


if __name__ == "__main__":
    unittest.main()
