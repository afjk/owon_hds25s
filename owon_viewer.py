"""OWON HDS25S native macOS screen-waveform viewer."""

from collections import deque
from copy import deepcopy
from datetime import datetime
from pathlib import Path
import sys
import time

import numpy as np
import pyqtgraph as pg
from PySide6.QtCore import Qt, QTimer, Slot
from PySide6.QtGui import QFont
from PySide6.QtWidgets import (
    QApplication, QComboBox, QFileDialog, QFrame, QHBoxLayout, QLabel, QLayout, QMainWindow,
    QMessageBox, QPushButton, QScrollArea, QVBoxLayout, QWidget,
)

from owon_waveform import ScreenFrame, channel_metadata, load_capture, optional_field, save_capture
from owon_worker import AcquisitionWorker


ROOT = Path(__file__).resolve().parent
COLORS = {"CH1": "#f0cf63", "CH2": "#58d3df"}
ASSIGNMENTS = (
    {"CH1": "audio", "CH2": "light"},
    {"CH1": "light", "CH2": "audio"},
)
PURPOSES = {"audio": "マイク MAX4466", "light": "光センサー TEMT6000"}
STYLE = """
QMainWindow, QWidget { background: #111821; color: #e4eaf1; }
QLabel { background: transparent; }
QLabel#title { font-size: 24px; font-weight: 600; }
QLabel#muted { color: #92a1b4; }
QLabel#badge { background: #202c3a; border-radius: 9px; padding: 7px 12px; }
QLabel#notice { background: #1b2938; color: #b9cde1; border-radius: 8px; padding: 12px; }
QLabel#error { background: #3b2528; color: #ffc2b3; border-radius: 8px; padding: 12px; }
QLabel#metric { font-size: 42px; font-weight: 500; }
QFrame#card { background: #18222e; border: 1px solid #2b3847; border-radius: 10px; }
QPushButton { background: #263547; border: 1px solid #3b4e63; border-radius: 7px;
              padding: 9px 14px; font-weight: 500; }
QPushButton:hover { background: #32485f; }
QPushButton:pressed { background: #405a73; }
QPushButton:disabled { color: #677485; background: #1b2633; border-color: #293646; }
QPushButton#primary { background: #24586a; border-color: #397a8d; }
QPushButton#primary:hover { background: #2c6c80; }
QStatusBar { background: #111821; color: #9aa9bb; }
QToolTip { color: #e4eaf1; background: #263547; border: 1px solid #3b4e63; }
QComboBox { background: #263547; border: 1px solid #3b4e63; border-radius: 5px; padding: 7px; }
QComboBox QAbstractItemView { background: #263547; selection-background-color: #3b5972; }
QScrollArea { border: none; background: transparent; }
QScrollBar:vertical { background: #18222e; width: 7px; }
QScrollBar::handle:vertical { background: #42566a; border-radius: 3px; min-height: 24px; }
QScrollBar::add-line:vertical, QScrollBar::sub-line:vertical { height: 0; }
"""


def label(text, name=None, wrap=False):
    item = QLabel(text)
    item.setTextFormat(Qt.TextFormat.PlainText)
    item.setWordWrap(wrap)
    if name:
        item.setObjectName(name)
    return item


class ChannelPlot(QFrame):
    def __init__(self, name, purpose, parent=None):
        super().__init__(parent)
        self.setObjectName("card")
        self.name = name
        layout = QVBoxLayout(self)
        layout.setContentsMargins(14, 10, 14, 8)
        row = QHBoxLayout()
        self.heading = label(f"{name}  ·  {purpose}")
        self.heading.setStyleSheet(f"color: {COLORS[name]}; font-size: 15px; font-weight: 600;")
        row.addWidget(self.heading)
        row.addStretch()
        self.info = label("未取得", "muted")
        row.addWidget(self.info)
        layout.addLayout(row)
        self.plot = pg.PlotWidget(background="#18222e")
        self.plot.setMinimumHeight(145)
        self.plot.showGrid(x=True, y=True, alpha=0.18)
        self.plot.setLabel("left", "画面値（signed byte）", color="#92a1b4")
        self.plot.setLabel("bottom", "受信位置（時間未校正）", color="#92a1b4")
        self.plot.setMenuEnabled(False)
        self.plot.hideButtons()
        self.plot.setYRange(-128, 127, padding=0)
        self.plot.setXRange(0, 599, padding=0)
        self.plot.getViewBox().setLimits(yMin=-140, yMax=140)
        self.curve = self.plot.plot(pen=pg.mkPen(COLORS[name], width=1))
        self.x_positions = np.empty(0)
        self.info_key = None
        layout.addWidget(self.plot)

    def set_frame(self, frame, metadata):
        raw = frame.payloads.get(self.name, b"")
        values = np.frombuffer(raw, dtype=np.int8)
        if len(self.x_positions) != len(values):
            self.x_positions = np.arange(len(values))
        self.curve.setData(self.x_positions, values)
        info_key = tuple(str(optional_field(metadata, key)) for key in ("SCALE", "COUPLING", "PROBE")) + (len(raw),)
        if self.info_key == info_key:
            return
        self.info_key = info_key
        if raw:
            self.info.setText(
                f"{optional_field(metadata, 'SCALE')} /div  ·  "
                f"{optional_field(metadata, 'COUPLING')}  ·  "
                f"probe {optional_field(metadata, 'PROBE')}  ·  {len(raw)} bytes"
            )
        else:
            self.info.setText("チャンネルOFF / 受信データなし")


class MainWindow(QMainWindow):
    def __init__(self, auto_connect=True, worker_factory=AcquisitionWorker):
        super().__init__()
        self.setWindowTitle("OWON HDS25S · 波形ビューア")
        self.resize(1190, 800)
        self.setMinimumSize(960, 680)
        self.worker_factory = worker_factory
        self.worker = None
        self.ready = False
        self.paused = False
        self.closing = False
        self.capture_pending = False
        self.capture_path = None
        self.capture_assignments = None
        self.current_frame = None
        self.frame_count = 0
        self.arrivals = deque(maxlen=60)
        self._last_info_update = 0.0
        self.failed_message = None

        root = QWidget()
        self.setCentralWidget(root)
        layout = QVBoxLayout(root)
        layout.setContentsMargins(24, 20, 24, 14)
        layout.setSpacing(14)
        heading = QHBoxLayout()
        title_column = QVBoxLayout()
        title_column.addWidget(label("OWON HDS25S", "title"))
        title_column.addWidget(label("USB WAVEFORM VIEWER  /  光と音のタイミング計測", "muted"))
        heading.addLayout(title_column)
        heading.addStretch()
        self.connection_label = label("未接続", "badge")
        heading.addWidget(self.connection_label)
        layout.addLayout(heading)

        toolbar = QHBoxLayout()
        self.connect_button = QPushButton("接続")
        self.connect_button.setObjectName("primary")
        self.connect_button.clicked.connect(self.toggle_connection)
        self.pause_button = QPushButton("表示を一時停止")
        self.pause_button.clicked.connect(self.toggle_pause)
        self.capture_button = QPushButton("停止波形を保存…")
        self.capture_button.setToolTip("本体のRUN/STOPで停止したCH1・CH2を再取得し、元の受信データを保存します")
        self.capture_button.clicked.connect(self.begin_capture)
        self.open_button = QPushButton("保存データを開く…")
        self.open_button.clicked.connect(self.open_capture)
        self.fit_button = QPushButton("表示をリセット")
        self.fit_button.clicked.connect(self.reset_view)
        for button in (self.connect_button, self.pause_button, self.capture_button, self.open_button, self.fit_button):
            toolbar.addWidget(button)
        toolbar.addStretch()
        layout.addLayout(toolbar)

        self.notice = label(
            "初版：受信データをそのまま表示しています。時間・電圧への変換とΔt計測は、既知信号での確認後に有効化します。",
            "notice", True,
        )
        layout.addWidget(self.notice)
        self.error_label = label("", "error", True)
        self.error_label.hide()
        layout.addWidget(self.error_label)

        content = QHBoxLayout()
        content.setSpacing(16)
        plots = QVBoxLayout()
        plots.setSpacing(14)
        self.channels = {
            name: ChannelPlot(name, PURPOSES[role])
            for name, role in ASSIGNMENTS[0].items()
        }
        self.channels["CH2"].plot.setXLink(self.channels["CH1"].plot)
        for channel in self.channels.values():
            plots.addWidget(channel, 1)
        content.addLayout(plots, 1)

        sidebar = QWidget()
        side = QVBoxLayout(sidebar)
        side.setSizeConstraint(QLayout.SizeConstraint.SetMinimumSize)
        side.setContentsMargins(0, 0, 0, 0)
        side.setSpacing(14)
        measurement = QFrame()
        measurement.setObjectName("card")
        measure_layout = QVBoxLayout(measurement)
        measure_layout.setContentsMargins(18, 18, 18, 18)
        measure_layout.addWidget(label("AV時間差  Δt", "muted"))
        measure_layout.addWidget(label("—", "metric"))
        self.delta_direction = label("CH1 音 − CH2 光", "muted")
        measure_layout.addWidget(self.delta_direction)
        measure_layout.addSpacing(12)
        measure_layout.addWidget(label("時間軸の確認待ち", wrap=True))
        measure_layout.addWidget(label("正の値＝音が遅い", "muted"))
        side.addWidget(measurement)

        assignment_card = QFrame()
        assignment_card.setObjectName("card")
        assignment_layout = QVBoxLayout(assignment_card)
        assignment_layout.setContentsMargins(18, 14, 18, 14)
        assignment_layout.addWidget(label("入力の割り当て", "muted"))
        self.assignment_combo = QComboBox()
        self.assignment_combo.addItems(("CH1: マイク / CH2: 光", "CH1: 光 / CH2: マイク"))
        self.assignment_combo.setToolTip("グラフと計測対象の対応だけを変更します。本体の設定や配線は変えません")
        self.assignment_combo.currentIndexChanged.connect(self.update_assignment)
        assignment_layout.addWidget(self.assignment_combo)
        side.addWidget(assignment_card)

        device = QFrame()
        device.setObjectName("card")
        device_layout = QVBoxLayout(device)
        device_layout.setContentsMargins(18, 18, 18, 18)
        device_layout.addWidget(label("取得情報", "muted"))
        self.mode_label = label("データなし", wrap=True)
        self.scope_status_label = label("本体状態   —", wrap=True)
        self.scope_status_label.setToolTip("プレビューの本体状態・設定情報は約1秒ごとに照会します。停止保存では改めて照会・検証します")
        self.timebase_label = label("本体時間軸   —", wrap=True)
        self.adc_label = label("ADC rate   —", "muted", True)
        self.performance_label = label("USB取得   —", "muted", True)
        self.identity_label = label("機種情報   —", "muted", True)
        for item in (self.mode_label, self.scope_status_label, self.timebase_label, self.adc_label, self.performance_label, self.identity_label):
            device_layout.addWidget(item)
            device_layout.addSpacing(5)
        side.addWidget(device)
        side.addWidget(label(
            "保存するには\n① 本体でCH1・CH2をON\n② 本体のRUN/STOPで停止\n③「停止波形を保存」\n\nMacの表示停止は、本体の収録停止とは別です。", "muted", True,
        ))
        side.addStretch()
        sidebar_scroll = QScrollArea()
        sidebar_scroll.setFixedWidth(258)
        sidebar_scroll.setWidgetResizable(True)
        sidebar_scroll.setHorizontalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        sidebar_scroll.setWidget(sidebar)
        content.addWidget(sidebar_scroll)
        layout.addLayout(content, 1)
        self.footer = label("USB: 5345:1234  ·  読み取り専用  ·  本体の設定は変更しません", "muted")
        layout.addWidget(self.footer)
        self.statusBar().showMessage("接続待ち")
        self.update_controls()
        if auto_connect:
            QTimer.singleShot(150, self.start_connection)

    def update_controls(self):
        busy = self.worker is not None
        self.connect_button.setText("切断" if busy else "接続")
        self.connect_button.setEnabled(not self.closing)
        self.pause_button.setText("表示再開" if self.paused else "表示を一時停止")
        self.pause_button.setEnabled(self.ready and not self.capture_pending and not self.closing)
        self.capture_button.setEnabled(self.ready and not self.capture_pending and not self.closing)
        self.assignment_combo.setEnabled(not self.capture_pending and not self.closing)
        self.open_button.setEnabled(not busy and not self.closing)
        self.open_button.setToolTip("USBを切断してから保存データを開けます" if busy else "保存したJSONをオフラインで表示します")

    @Slot()
    def toggle_connection(self):
        if self.worker is None:
            self.start_connection()
        else:
            self.ready = False
            self.worker.request_stop()
            self.connection_label.setText("切断中…")
            self.update_controls()
            self.connect_button.setEnabled(False)

    @Slot()
    def start_connection(self):
        if self.worker is not None or self.closing:
            return
        self.error_label.hide()
        self.failed_message = None
        self.ready = False
        self.paused = False
        self.arrivals.clear()
        self.frame_count = 0
        self.connection_label.setText("接続中…")
        self.mode_label.setText("接続中 / 既存表示は更新されません")
        worker = self.worker_factory(self)
        self.worker = worker
        worker.opened.connect(self.on_opened)
        worker.preview.connect(self.on_preview)
        worker.captured.connect(self.on_captured)
        worker.capture_failed.connect(self.on_capture_failed)
        worker.failed.connect(self.on_failed)
        worker.finished.connect(self.on_finished)
        worker.start()
        self.update_controls()

    @Slot(str)
    def on_opened(self, identity):
        if self.worker is None or self.closing or self.worker.stopping:
            return
        self.ready = True
        self.connection_label.setText("● USB接続")
        self.identity_label.setText(identity)
        self.statusBar().showMessage("接続成功。波形を取得中…")
        self.update_controls()

    @Slot()
    def on_preview(self):
        worker = self.sender()
        if worker is not self.worker:
            return
        frame = worker.take_latest_preview()
        if frame is None:
            return
        if not self.ready or self.paused or self.closing:
            return
        self.frame_count += 1
        self.arrivals.append(time.monotonic())
        self.display_frame(frame, "ライブ表示（CH間の収録一致は未検証）")

    def display_frame(self, frame, mode):
        self.current_frame = frame
        metadata = channel_metadata(frame.header)
        for name, channel in self.channels.items():
            channel.set_frame(frame, metadata.get(name, {}))
        now = time.monotonic()
        live = mode.startswith("ライブ")
        # Waveforms update every frame; text/layout work is limited to 4 Hz.
        if live and now - self._last_info_update < 0.25:
            return
        self._last_info_update = now
        self.mode_label.setText(mode)
        self.scope_status_label.setText(f"本体状態   {frame.status}")
        time_info = optional_field(frame.header, "TIMEBASE", {})
        if isinstance(time_info, dict):
            scale = optional_field(time_info, "SCALE")
        else:
            scale = time_info
        self.timebase_label.setText(f"本体時間軸   {scale} /div")
        sample_info = optional_field(frame.header, "SAMPLE", {})
        self.adc_label.setText(f"ADC rate   {optional_field(sample_info, 'SAMPLERATE')}")
        rate = (len(self.arrivals) - 1) / (self.arrivals[-1] - self.arrivals[0]) if len(self.arrivals) > 1 else 0
        if live:
            lag_ms = max(0, (now - frame.received_monotonic) * 1000) if frame.received_monotonic else 0
            self.performance_label.setText(
                f"波形更新   {rate:.1f} 回/秒\nUSB照会   {frame.query_ms:.0f} ms\n"
                f"表示待ち   {lag_ms:.0f} ms\n設定情報   約1秒ごと"
            )
            self.statusBar().showMessage(f"USB取得 {frame.sequence} 回  ·  表示 {self.frame_count} 回  ·  本体の収録停止とは別です")
        else:
            self.performance_label.setText(f"取得時刻（Mac / UTC）\n{frame.record.get('retrieved_at_utc', '—')}")
        self.identity_label.setText(frame.identity)

    @Slot(int)
    def update_assignment(self, index):
        mapping = ASSIGNMENTS[index]
        for name, channel in self.channels.items():
            channel.heading.setText(f"{name}  ·  {PURPOSES[mapping[name]]}")
        audio = next(name for name, role in mapping.items() if role == "audio")
        light = next(name for name, role in mapping.items() if role == "light")
        self.delta_direction.setText(f"{audio} 音 − {light} 光")

    @Slot()
    def toggle_pause(self):
        if not self.ready:
            return
        self.paused = not self.paused
        self.worker.set_paused(self.paused)
        self.arrivals.clear()
        if self.paused:
            self.mode_label.setText("Macの表示を一時停止（本体は別動作）")
            self.performance_label.setText("USBプレビュー照会を停止\n本体の収録停止とは別です")
            self.statusBar().showMessage("USBプレビュー照会を一時停止しました。本体の収録設定は変えていません")
        else:
            self.mode_label.setText("ライブ表示を再開中…")
        self.update_controls()

    @Slot()
    def begin_capture(self):
        if not self.ready or self.capture_pending:
            return
        default = ROOT / "captures" / datetime.now().strftime("capture-%Y%m%d-%H%M%S-%f.json")
        filename, _ = QFileDialog.getSaveFileName(self, "停止波形の保存先（本体をRUN/STOPで停止してください）", str(default), "波形データ (*.json)")
        if not filename:
            return
        if not filename.lower().endswith(".json"):
            filename += ".json"
        if Path(filename).exists():
            QMessageBox.warning(self, "保存先を変更してください", "既存の測定ファイルは上書きしません。別のファイル名を選んでください。")
            return
        self.capture_path = Path(filename)
        self.capture_assignments = dict(ASSIGNMENTS[self.assignment_combo.currentIndex()])
        self.capture_pending = True
        self.worker.request_capture()
        self.statusBar().showMessage("本体の停止状態を確認して、両チャンネルを取得中…")
        self.update_controls()

    @Slot(object)
    def on_captured(self, frame):
        record = deepcopy(frame.record)
        record["sensor_assignments"] = self.capture_assignments
        frame = ScreenFrame.from_record(record, frame.query_ms)
        self.capture_pending = False
        self.paused = True
        self.arrivals.clear()
        self.display_frame(frame, "停止波形（取得前後のSTOP・設定を確認）")
        try:
            output = save_capture(frame, self.capture_path)
        except (OSError, ValueError) as exc:
            self.show_error(f"保存できませんでした: {exc}")
            self.statusBar().showMessage("取得データは画面に残っています。保存に失敗しました")
        else:
            self.error_label.hide()
            self.footer.setText(f"保存済み: {output}")
            self.statusBar().showMessage("両チャンネルの元データを保存しました。ライブ表示に戻るには「表示再開」")
        self.capture_path = None
        self.capture_assignments = None
        self.update_controls()

    @Slot(str)
    def on_capture_failed(self, message):
        self.capture_pending = False
        self.capture_path = None
        self.capture_assignments = None
        self.show_error(message.replace("capture", "保存"))
        self.statusBar().showMessage("保存していません。本体の停止状態・CH設定を確認してください")
        self.update_controls()

    @Slot()
    def open_capture(self):
        if self.worker is not None:
            return
        filename, _ = QFileDialog.getOpenFileName(self, "保存した波形を開く", str(ROOT / "captures"), "波形データ (*.json)")
        if not filename:
            return
        try:
            frame = load_capture(filename)
            mapping = frame.record.get("sensor_assignments")
            if mapping in ASSIGNMENTS:
                self.assignment_combo.setCurrentIndex(ASSIGNMENTS.index(mapping))
            self.display_frame(frame, "保存データ / オフライン表示")
        except (OSError, ValueError, RuntimeError) as exc:
            self.show_error(f"開けませんでした: {exc}")
            return
        self.error_label.hide()
        self.reset_view()
        self.footer.setText(f"読み込み: {filename}")
        self.statusBar().showMessage("保存した元データを表示しています。USB接続は不要です")

    @Slot(str)
    def on_failed(self, message):
        self.failed_message = message
        self.ready = False
        self.show_error(message + "。応答しない場合はUSBを抜き差しして「接続」を押してください。")
        self.connection_label.setText("通信エラー")
        self.mode_label.setText("更新停止 / 表示データは最新ではありません")
        self.update_controls()

    @Slot()
    def on_finished(self):
        old_worker = self.worker
        self.worker = None
        self.ready = False
        self.capture_pending = False
        self.capture_path = None
        self.capture_assignments = None
        if old_worker is not None:
            old_worker.deleteLater()
        if not self.failed_message:
            self.connection_label.setText("未接続")
            if self.current_frame is not None:
                self.mode_label.setText("USB切断 / 最終取得データを表示")
            self.performance_label.setText("USBプレビュー照会を停止")
        self.update_controls()
        if self.closing:
            QTimer.singleShot(0, self.close)
        elif self.failed_message:
            self.statusBar().showMessage("通信を終了しました。接続状態を確認して再接続できます")
        else:
            self.statusBar().showMessage("USBを解放しました。再接続または保存データの表示ができます")

    def show_error(self, message):
        self.error_label.setText(message)
        self.error_label.show()

    @Slot()
    def reset_view(self):
        length = max((len(data) for data in self.current_frame.payloads.values()), default=600) if self.current_frame else 600
        self.channels["CH1"].plot.setXRange(0, max(1, length - 1), padding=0)
        for channel in self.channels.values():
            channel.plot.setYRange(-128, 127, padding=0)

    def closeEvent(self, event):
        if self.worker is not None:
            self.closing = True
            self.ready = False
            self.worker.request_stop()
            self.update_controls()
            self.statusBar().showMessage("USBを解放して終了します…")
            event.ignore()
        else:
            event.accept()


def main():
    app = QApplication(sys.argv)
    app.setApplicationName("OWON HDS25S Viewer")
    app.setOrganizationName("OWON Waveform Project")
    app.setStyle("Fusion")
    app.setStyleSheet(STYLE)
    app.setFont(QFont("Hiragino Sans", 12))
    pg.setConfigOptions(antialias=False)
    window = MainWindow()
    window.show()
    return app.exec()


if __name__ == "__main__":
    raise SystemExit(main())
