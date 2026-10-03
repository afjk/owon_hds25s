"""All USB access is owned by one worker thread; no instrument mutations."""

from queue import Empty, Queue
from dataclasses import replace
import threading
import time

from PySide6.QtCore import QThread, Signal

from owon_probe import BulkTransport, usb_modules
from owon_protocol import QueryClient, capture_stopped
from owon_waveform import LiveWaveformReader, ScreenFrame, stamp_record


def open_client():
    core, util, backend = usb_modules()
    matches = list(core.find(find_all=True, idVendor=0x5345, idProduct=0x1234, backend=backend))
    if not matches:
        raise RuntimeError("HDS25Sが見つかりません。本体をUSB HID・オシロスコープモードにし、ケーブルを確認してください")
    if len(matches) != 1:
        raise RuntimeError("対応USB機器が複数あります。初版ではHDS25Sを1台だけ接続してください")
    transport = BulkTransport(matches[0], core, util)
    return QueryClient(transport), transport.close


class AcquisitionWorker(QThread):
    opened = Signal(str)
    preview = Signal()
    captured = Signal(object)
    capture_failed = Signal(str)
    failed = Signal(str)

    def __init__(self, parent=None, client_factory=open_client, interval=1 / 60, metadata_interval=1.0):
        super().__init__(parent)
        if interval < 0 or metadata_interval < 0:
            raise ValueError("poll intervals must not be negative")
        self.client_factory = client_factory
        self.interval = interval
        self.metadata_interval = metadata_interval
        self._stop = threading.Event()
        self._paused = threading.Event()
        self._wake = threading.Event()
        self._refresh = threading.Event()
        self._jobs = Queue()
        self._preview_lock = threading.Lock()
        self._latest_preview = None
        self._preview_pending = False

    def _publish_preview(self, frame):
        # At most one notification is queued. A busy UI receives the latest frame,
        # rather than replaying an ever-growing queue of obsolete waveforms.
        with self._preview_lock:
            self._latest_preview = frame
            notify = not self._preview_pending
            self._preview_pending = True
        if notify:
            self.preview.emit()

    def take_latest_preview(self):
        with self._preview_lock:
            frame = self._latest_preview
            self._latest_preview = None
            self._preview_pending = False
            return frame

    def request_stop(self):
        self._stop.set()
        self._wake.set()

    @property
    def stopping(self):
        return self._stop.is_set()

    def set_paused(self, paused):
        if paused:
            self._paused.set()
        else:
            self._paused.clear()
            self._refresh.set()
        self._wake.set()

    def request_capture(self):
        self._jobs.put("capture")
        self._wake.set()

    def run(self):
        close = None
        try:
            client, close = self.client_factory()
            if self._stop.is_set():
                return
            identity = client.text("*IDN?")
            if "HDS" not in identity.upper():
                raise RuntimeError(f"HDSオシロスコープとして識別できません: {identity}")
            self.opened.emit(identity)
            reader = LiveWaveformReader(client, identity, self.metadata_interval)
            sequence = 0
            while not self._stop.is_set():
                self._wake.clear()
                try:
                    job = self._jobs.get_nowait()
                except Empty:
                    job = None
                if job == "capture":
                    started = time.monotonic()
                    try:
                        record = stamp_record(capture_stopped(client))
                        frame = ScreenFrame.from_record(record, (time.monotonic() - started) * 1000)
                    except Exception as exc:
                        self.capture_failed.emit(str(exc))
                        if client.failed:
                            raise
                    else:
                        # Keep the stopped capture on screen until explicit resume.
                        self._paused.set()
                        reader.invalidate()
                        self.captured.emit(frame)
                    continue
                if self._paused.is_set():
                    self._wake.wait(0.1)
                    continue
                started = time.monotonic()
                if self._refresh.is_set():
                    self._refresh.clear()
                    reader.invalidate()
                frame = reader.read()
                sequence += 1
                frame = replace(frame, sequence=sequence)
                if not self._stop.is_set():
                    self._publish_preview(frame)
                self._wake.wait(max(0, self.interval - (time.monotonic() - started)))
        except Exception as exc:
            if not self._stop.is_set():
                self.failed.emit(str(exc))
        finally:
            if close is not None:
                try:
                    close()
                except Exception as exc:
                    if not self._stop.is_set():
                        self.failed.emit(f"USB終了処理: {exc}")
