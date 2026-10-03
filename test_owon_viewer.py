"""Offscreen tests of our own UI/worker; no USB or desktop input."""

import os
os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

from pathlib import Path
from dataclasses import replace
import tempfile
import threading
import time
import unittest

try:
    from PySide6.QtWidgets import QApplication
    from owon_viewer import ASSIGNMENTS, MainWindow, STYLE
    from owon_worker import AcquisitionWorker
except ImportError:
    QApplication = None

from owon_waveform import ScreenFrame, load_capture
from test_owon_waveform import FakeClient, stopped_record


@unittest.skipIf(QApplication is None, "GUI依存パッケージは未導入です")
class ViewerTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.app = QApplication.instance() or QApplication([])
        cls.app.setStyleSheet(STYLE)

    def setUp(self):
        self.client = FakeClient()
        self.close_threads = []
        self.factory_threads = []

        def client_factory():
            self.factory_threads.append(threading.get_ident())
            return self.client, lambda: self.close_threads.append(threading.get_ident())

        self.window = MainWindow(auto_connect=False, worker_factory=lambda parent: AcquisitionWorker(parent, client_factory, 0.03))

    def wait_for(self, predicate, timeout=3):
        deadline = time.monotonic() + timeout
        while not predicate() and time.monotonic() < deadline:
            self.app.processEvents()
            time.sleep(0.005)
        self.app.processEvents()
        self.assertTrue(predicate(), "Qtイベントの待機がタイムアウトしました")

    def tearDown(self):
        self.window.close()
        self.wait_for(lambda: self.window.worker is None)
        self.window.deleteLater()
        self.app.processEvents()

    def test_preview_pause_resume_disconnect_and_reconnect(self):
        self.window.start_connection()
        self.wait_for(lambda: self.window.frame_count >= 5)
        self.assertEqual(self.window.current_frame.payloads["CH1"], bytes.fromhex("007f80ff"))
        self.assertNotEqual(self.factory_threads[0], threading.get_ident())
        self.window.toggle_pause()
        count = self.window.frame_count
        self.wait_for(lambda: self.window.worker._paused.is_set())
        deadline = time.monotonic() + 0.15
        self.wait_for(lambda: time.monotonic() > deadline)
        self.assertEqual(self.window.frame_count, count)
        self.window.toggle_pause()
        self.wait_for(lambda: self.window.frame_count > count)
        self.window.toggle_connection()
        self.wait_for(lambda: self.window.worker is None)
        self.assertEqual(self.close_threads, self.factory_threads)
        self.window.start_connection()
        self.wait_for(lambda: self.window.frame_count >= 2)

    def test_stopped_capture_is_saved_with_current_sensor_assignment(self):
        self.window.start_connection()
        self.wait_for(lambda: self.window.frame_count >= 2)
        self.client.status = "STOP"
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "stopped.json"
            self.window.capture_path = path
            self.window.capture_assignments = dict(ASSIGNMENTS[0])
            self.window.capture_pending = True
            self.window.worker.request_capture()
            self.wait_for(path.exists)
            self.assertEqual(load_capture(path).record["sensor_assignments"], {"CH1": "audio", "CH2": "light"})
            self.assertTrue(self.window.paused)
            self.assertFalse(self.window.capture_pending)
            self.assertTrue(self.window.current_frame.is_stopped_capture)

    def test_running_capture_is_rejected_without_stopping_preview(self):
        self.window.start_connection()
        self.wait_for(lambda: self.window.frame_count >= 2)
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "must-not-exist.json"
            self.window.capture_path = path
            self.window.capture_assignments = dict(ASSIGNMENTS[0])
            self.window.capture_pending = True
            self.window.worker.request_capture()
            self.wait_for(lambda: not self.window.capture_pending)
            self.assertFalse(path.exists())
            self.assertFalse(self.window.paused)
            count = self.window.frame_count
            self.wait_for(lambda: self.window.frame_count > count)

    def test_channel_assignment_updates_direction_without_usb_commands(self):
        before = list(self.client.commands)
        self.assertIn("マイク", self.window.channels["CH1"].heading.text())
        self.assertEqual(self.window.delta_direction.text(), "CH1 音 − CH2 光")
        self.window.assignment_combo.setCurrentIndex(1)
        self.assertIn("光センサー", self.window.channels["CH1"].heading.text())
        self.assertEqual(self.window.delta_direction.text(), "CH2 音 − CH1 光")
        self.assertEqual(self.client.commands, before)

    def test_connection_error_does_not_leave_a_running_thread(self):
        self.window.worker_factory = lambda parent: AcquisitionWorker(parent, lambda: (_ for _ in ()).throw(RuntimeError("fixture: unplugged")))
        self.window.start_connection()
        self.wait_for(lambda: self.window.worker is None)
        self.assertFalse(self.window.ready)
        self.assertIn("fixture: unplugged", self.window.error_label.text())
        self.assertEqual(self.window.connect_button.text(), "接続")

    def test_pending_preview_notification_keeps_latest_frame_only(self):
        worker = AcquisitionWorker(self.window)
        notifications = []
        worker.preview.connect(lambda: notifications.append(True))
        frame = ScreenFrame.from_record(stopped_record())
        for sequence in (1, 2, 3):
            worker._publish_preview(replace(frame, sequence=sequence))
        self.assertEqual(len(notifications), 1)
        self.assertEqual(worker.take_latest_preview().sequence, 3)
        self.assertIsNone(worker.take_latest_preview())
        worker._publish_preview(replace(frame, sequence=4))
        self.assertEqual(len(notifications), 2)
        self.assertEqual(worker.take_latest_preview().sequence, 4)

    def test_resume_refreshes_cached_channel_settings(self):
        self.window.start_connection()
        self.wait_for(lambda: self.window.frame_count >= 3)
        self.window.toggle_pause()
        self.client.enabled = ("CH2",)
        self.window.toggle_pause()
        self.wait_for(lambda: set(self.window.current_frame.payloads) == {"CH2"})
        self.assertEqual(self.window.current_frame.header["CHANNEL"][0]["DISPLAY"], "OFF")


if __name__ == "__main__":
    unittest.main()
