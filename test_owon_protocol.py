import json
import struct
import unittest

from owon_protocol import ProtocolError, QueryClient, capture_stopped


def frame(payload):
    return struct.pack("<I", len(payload)) + payload


class FakeTransport:
    def __init__(self, chunks):
        self.chunks = list(chunks)
        self.writes = []

    def write(self, data, timeout_ms):
        self.writes.append(data)

    def read(self, size, timeout_ms):
        if not self.chunks:
            raise TimeoutError("truncated transfer")
        return self.chunks.pop(0)


class ProtocolTests(unittest.TestCase):
    def test_fragmented_header_and_binary_newlines_are_preserved(self):
        payload = bytes([0, 10, 13, 127, 128, 255])
        packet = frame(payload)
        transport = FakeTransport([packet[:1], packet[1:3], packet[3:7], packet[7:]])
        self.assertEqual(QueryClient(transport).query(":DATA:WAVE:SCREEN:CH1?"), payload)

    def test_little_endian_length_larger_than_one_usb_packet(self):
        payload = bytes(range(256)) * 3
        packet = frame(payload)
        transport = FakeTransport([packet[i:i + 64] for i in range(0, len(packet), 64)])
        self.assertEqual(QueryClient(transport).query(":DATA:WAVE:SCREEN:CH2?"), payload)

    def test_fragmented_text_response(self):
        client = QueryClient(FakeTransport([b"OWON,HDS", b"25S,example,1.0\r", b"\n"]))
        self.assertEqual(client.text("*IDN?"), "OWON,HDS25S,example,1.0")

    def test_invalid_or_truncated_response_invalidates_session(self):
        for chunks, error in (([struct.pack("<I", 0)], ProtocolError), ([b"\xff" * 4], ProtocolError), ([frame(b"123")[:-1]], TimeoutError), ([frame(b"123") + b"extra"], ProtocolError)):
            with self.subTest(chunks=chunks):
                client = QueryClient(FakeTransport(chunks))
                with self.assertRaises(error):
                    client.query(":DATA:WAVE:SCREEN:CH1?")
                with self.assertRaises(ProtocolError):
                    client.query("*IDN?")

    def test_settings_and_command_injection_cannot_be_sent(self):
        transport = FakeTransport([])
        client = QueryClient(transport)
        for command in ("*RST", ":CH1:DISPLAY ON", "*IDN?;*RST", "*IDN?\n*RST"):
            with self.assertRaises(ValueError):
                client.query(command)
        self.assertEqual(transport.writes, [])

    def test_stopped_two_channel_capture_preserves_payload(self):
        head = {"channel": [{"name": "ch1", "display": "on"}, {"name": "ch2", "display": "on"}]}
        transport = FakeTransport([b"OWON,HDS25S\n", b"STOP\n", frame(json.dumps(head).encode()), frame(b"\x80\n"), frame(b"\x00\xff"), frame(json.dumps(head).encode()), b"STOP\n"])
        result = capture_stopped(QueryClient(transport))
        self.assertEqual(result["channels"]["CH1"]["payload_hex"], "800a")
        self.assertEqual(result["channels"]["CH2"]["payload_hex"], "00ff")
        self.assertIn("not_yet_verified", result["calibration"])

    def test_running_scope_is_rejected_before_waveform_queries(self):
        transport = FakeTransport([b"OWON,HDS25S\n", b"AUTO\n"])
        with self.assertRaises(ProtocolError):
            capture_stopped(QueryClient(transport))
        self.assertEqual(len(transport.writes), 2)

    def test_capture_rejects_resumed_acquisition_and_changed_header(self):
        head = {"CHANNEL": [{"NAME": "CH1", "DISPLAY": "ON"}, {"NAME": "CH2", "DISPLAY": "ON"}]}
        for after, status in ((head, b"AUTO\n"), ({**head, "RUNSTATUS": "AUTO"}, b"STOP\n")):
            transport = FakeTransport([b"OWON,HDS25S\n", b"STOP\n", frame(json.dumps(head).encode()), frame(b"12"), frame(b"34"), frame(json.dumps(after).encode()), status])
            with self.assertRaises(ProtocolError):
                capture_stopped(QueryClient(transport))


if __name__ == "__main__":
    unittest.main()
