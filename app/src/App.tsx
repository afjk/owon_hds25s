import { useEffect, useRef, useState } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import ScopeCanvas, { type PlotConfig } from "./ScopeCanvas";
import DeviceControls, { type DeviceMeasurements } from "./DeviceControls";
import GeneratorControls from "./GeneratorControls";
import TriggerControls from "./TriggerControls";
import AutosetControls from "./AutosetControls";
import PrintPreview, { type PrintData } from "./PrintPreview";
import { useLocale } from "./locale";
import { restorePlot, storePlot } from "./preferences";
import {
  channelInfo,
  field,
  filename,
  statistics,
  text,
  type Device,
  type Frame,
  type PlotMode,
  type ChannelName,
} from "./model";

const initialPlot: PlotConfig = {
  mode: "wave",
  stacked: false,
  grid: true,
  points: false,
  show: [true, true],
  colors: ["#f1c65c", "#60d7ca"],
  background: "#0b1420",
  gridColor: "#233246",
  inverted: [false, false],
  math: "none",
  fftWindow: "hann",
  fftDb: false,
  cursor: "none",
  cursors: [200, 400, -40, 40],
  reset: 0,
};
const basename = (path: string) => path.split(/[\\/]/).pop() ?? path;
function storedRecent(): string[] {
  try {
    return JSON.parse(localStorage.getItem("owon-recent") ?? "[]")
      .filter((p: unknown) => typeof p === "string")
      .slice(0, 10);
  } catch {
    return [];
  }
}
type Operation =
  | "pause"
  | "once"
  | "save"
  | "open"
  | "capture"
  | "csv"
  | "txt"
  | "xls"
  | "png"
  | "bmp"
  | "gif"
  | "print"
  | "reset";

export default function App() {
  const { t, language, setLanguage } = useLocale();
  const native = isTauri();
  const [devices, setDevices] = useState<Device[]>([]),
    [selected, setSelected] = useState("");
  const [connected, setConnected] = useState(false),
    [paused, setPaused] = useState(false),
    [busy, setBusy] = useState(false),
    [interval, setIntervalMs] = useState(17);
  const [error, setError] = useState(""),
    [notice, setNotice] = useState(""),
    [frame, setFrame] = useState<Frame | null>(null),
    [fps, setFps] = useState(0);
  const [settingsRevision, setSettingsRevision] = useState(0);
  const frameRef = useRef<Frame | null>(null),
    pending = useRef<{ frame: Frame } | null>(null),
    canvasRef = useRef<HTMLCanvasElement | null>(null);
  const [plot, setPlot] = useState(() =>
      restorePlot(initialPlot, localStorage.getItem("owon-display")),
    ),
    [recent, setRecent] = useState(storedRecent);
  const [playlist, setPlaylist] = useState<string[]>([]),
    [playIndex, setPlayIndex] = useState(0),
    [playing, setPlaying] = useState(false),
    [reverse, setReverse] = useState(false),
    [playInterval, setPlayInterval] = useState(500);
  const [folder, setFolder] = useState(""),
    [recording, setRecording] = useState(false),
    [recordInterval, setRecordInterval] = useState(1000),
    [savedCount, setSavedCount] = useState(0);
  const [recordFolder, setRecordFolder] = useState(""),
    [skippedCount, setSkippedCount] = useState(0);
  const [measurements, setMeasurements] = useState<DeviceMeasurements | null>(
      null,
    ),
    [printData, setPrintData] = useState<PrintData | null>(null),
    [about, setAbout] = useState(false);
  const [imageFormat, setImageFormat] = useState<"png" | "bmp" | "gif">("png"),
    [tableFormat, setTableFormat] = useState<"csv" | "txt" | "xls">("csv");
  const playBusy = useRef(false),
    connectionEpoch = useRef(0);
  const capturing = useRef(false);
  const connectionRef = useRef(connected);
  connectionRef.current = connected;
  const [tableChannel, setTableChannel] = useState<"both" | ChannelName>(
    "both",
  );
  const [tablePage, setTablePage] = useState(0);
  const lastRecordingError = useRef<string | null>(null);
  const updatePlot = (patch: Partial<PlotConfig>) =>
    setPlot((old) => ({ ...old, ...patch }));
  useEffect(() => {
    localStorage.setItem("owon-display", storePlot(plot));
  }, [plot]);
  const applyFrame = (f: Frame) => {
    frameRef.current = f;
    setFrame(f);
  };
  const remember = (path: string) => {
    setRecent((old) => {
      const next = [path, ...old.filter((p) => p !== path)].slice(0, 10);
      localStorage.setItem("owon-recent", JSON.stringify(next));
      return next;
    });
  };
  const report = (e: unknown) => {
    setError(String(e));
    setNotice("");
  };
  const run = async (action: () => Promise<void>) => {
    if (busy) return;
    setBusy(true);
    setError("");
    try {
      await action();
    } catch (e) {
      report(e);
    } finally {
      setBusy(false);
    }
  };

  // Consume only the newest preview on the animation clock; text panels update at 4 Hz.
  useEffect(() => {
    let animation = 0,
      lastInfo = 0,
      fpsAt = performance.now(),
      count = 0,
      lastAnimation = performance.now();
    const consume = (now: number) => {
      const next = pending.current;
      if (next) {
        pending.current = null;
        frameRef.current = next.frame;
        count++;
        if (now - lastInfo >= 250) {
          setFrame(next.frame);
          lastInfo = now;
        }
      }
      if (now - fpsAt >= 1000) {
        setFps((count * 1000) / (now - fpsAt));
        count = 0;
        fpsAt = now;
      }
    };
    const tick = (now: number) => {
      lastAnimation = performance.now();
      consume(now);
      animation = requestAnimationFrame(tick);
    };
    animation = requestAnimationFrame(tick);
    const fallback = window.setInterval(() => {
      if (performance.now() - lastAnimation > 50) consume(performance.now());
    }, 17);
    return () => {
      cancelAnimationFrame(animation);
      clearInterval(fallback);
    };
  }, []);
  useEffect(() => {
    if (!connected) return;
    let disposed = false,
      after = 0,
      timer = 0;
    const epoch = connectionEpoch.current;
    const poll = async () => {
      const started = performance.now();
      try {
        const snapshot = await invoke<{
          frame: Frame | null;
          paused: boolean;
          error: string | null;
          recording: {
            active: boolean;
            saved: number;
            skipped: number;
            directory: string | null;
            error: string | null;
          };
        }>("poll_preview", { after });
        if (disposed || epoch !== connectionEpoch.current) return;
        if (snapshot.error) throw new Error(snapshot.error);
        setPaused(snapshot.paused);
        setRecording(snapshot.recording.active);
        setSavedCount(snapshot.recording.saved);
        setSkippedCount(snapshot.recording.skipped);
        setRecordFolder(snapshot.recording.directory ?? "");
        if (
          snapshot.recording.error &&
          snapshot.recording.error !== lastRecordingError.current
        )
          setError(`自動保存: ${snapshot.recording.error}`);
        lastRecordingError.current = snapshot.recording.error;
        if (snapshot.frame) {
          after = snapshot.frame.sequence;
          if (!capturing.current) {
            frameRef.current = snapshot.frame;
            pending.current = { frame: snapshot.frame };
          }
        }
      } catch (e) {
        if (!disposed && epoch === connectionEpoch.current) {
          setConnected(false);
          setRecording(false);
          report(e);
        }
        return;
      }
      timer = window.setTimeout(
        () => void poll(),
        Math.max(1, 17 - (performance.now() - started)),
      );
    };
    void poll();
    return () => {
      disposed = true;
      clearTimeout(timer);
    };
  }, [connected]);
  async function refresh() {
    const found = await invoke<Device[]>("list_devices");
    setDevices(found);
    setSelected((old) =>
      found.some((d) => `${d.bus}:${d.address}` === old)
        ? old
        : found[0]
          ? `${found[0].bus}:${found[0].address}`
          : "",
    );
    if (!found.length)
      setNotice(
        t(
          "USBが見つかりません。本体のUSBをHIDにし、Python版などを切断して再検索してください。",
        ),
      );
  }
  useEffect(() => {
    if (native) void refresh().catch(report);
  }, []);
  async function toggleConnection() {
    if (connected) {
      setRecording(false);
      connectionEpoch.current++;
      pending.current = null;
      await invoke("disconnect_device");
      setConnected(false);
      setPaused(false);
      setNotice(t("USBを切断しました"));
      return;
    }
    const device = devices.find((d) => `${d.bus}:${d.address}` === selected);
    if (!device) throw new Error(t("実機を選択してください"));
    setPlaying(false);
    setMeasurements(null);
    setPlaylist([]);
    connectionEpoch.current++;
    const identity = await invoke<string>("connect_device", {
      bus: device.bus,
      address: device.address,
    });
    setConnected(true);
    setPaused(false);
    setNotice(identity);
    await invoke("set_preview", { paused: false, intervalMs: interval });
  }
  async function openPath(path: string) {
    if (connectionRef.current)
      throw new Error(t("USBを切断してから開いてください"));
    const f = await invoke<Frame>("load_record", { path });
    applyFrame(f);
    setMeasurements(null);
    remember(path);
    setNotice(basename(path));
    updatePlot({ reset: Date.now() });
  }
  async function choosePlaybackFolder() {
    const folder = await open({ directory: true });
    if (typeof folder !== "string") return;
    const paths = await invoke<string[]>("list_recordings", { folder });
    if (!paths.length) throw new Error(t("フォルダーにJSON波形がありません"));
    setPlaying(false);
    await openPath(paths[0]);
    setPlaylist(paths);
    setPlayIndex(0);
  }
  async function chooseOpen() {
    const paths = await open({
      multiple: true,
      filters: [{ name: "OWON JSON", extensions: ["json"] }],
    });
    if (!paths) return;
    const list = Array.isArray(paths) ? paths : [paths];
    if (list.length > 100)
      throw new Error(t("再生リストは100ファイルまでです"));
    setPlaying(false);
    await openPath(list[0]);
    setPlaylist(list);
    setPlayIndex(0);
  }
  async function seek(index: number) {
    if (index < 0 || index >= playlist.length) return;
    await openPath(playlist[index]);
    setPlayIndex(index);
  }
  useEffect(() => {
    if (!playing || playlist.length < 2 || connected) return;
    const timer = window.setInterval(() => {
      if (playBusy.current) return;
      const next = playIndex + (reverse ? -1 : 1);
      if (next < 0 || next >= playlist.length) {
        setPlaying(false);
        return;
      }
      playBusy.current = true;
      void seek(next)
        .catch((e) => {
          report(e);
          setPlaying(false);
        })
        .finally(() => {
          playBusy.current = false;
        });
    }, playInterval);
    return () => clearInterval(timer);
  }, [playing, playlist, playIndex, reverse, playInterval, connected]);
  async function saveFrame(f: Frame) {
    const path = await save({
      defaultPath: filename("json"),
      filters: [{ name: "OWON raw JSON", extensions: ["json"] }],
    });
    if (path) {
      await invoke("save_record", { path, record: f.record });
      remember(path);
      setNotice(`保存しました: ${basename(path)}`);
    }
  }
  // A shared, explicit operation layer: buttons/shortcuts use it; a future AI tool adapter can use the same validated actions.
  async function perform(op: Operation) {
    if (op === "reset") {
      updatePlot({ reset: Date.now() });
      return;
    }
    if (op === "open") {
      await chooseOpen();
      return;
    }
    if (op === "pause") {
      await invoke("set_preview", { paused: !paused, intervalMs: interval });
      return;
    }
    if (op === "once") {
      await invoke("acquire_once");
      return;
    }
    if (op === "capture") {
      await invoke("set_recording", {
        folder: null,
        intervalMs: recordInterval,
      });
      capturing.current = true;
      try {
        const f = await invoke<Frame>("capture_stopped");
        pending.current = null;
        applyFrame(f);
        setPaused(true);
        await saveFrame(f);
      } finally {
        capturing.current = false;
      }
      return;
    }
    const f = frameRef.current;
    if (!f) throw new Error(t("波形がありません"));
    if (op === "save") {
      await saveFrame(f);
      return;
    }
    if (op === "csv" || op === "txt" || op === "xls") {
      // Freeze and encode before the picker; newer USB frames cannot change this export.
      const xls =
        op === "xls"
          ? (await import("./xls")).excelBytes(f, tableChannel)
          : null;
      const path = await save({
        defaultPath: filename(op),
        filters: [{ name: `raw values ${op.toUpperCase()}`, extensions: [op] }],
      });
      if (path) {
        if (xls) await invoke("save_xls", { path, bytes: xls });
        else
          await invoke("export_table", {
            path,
            record: f.record,
            channel: tableChannel,
            format: op,
          });
        setNotice(
          `受信値の${op.toUpperCase()}を保存しました（${tableChannel}）`,
        );
      }
      return;
    }
    if (op === "png" || op === "bmp" || op === "gif" || op === "print") {
      const canvas = canvasRef.current;
      if (!canvas) throw new Error(t("波形表示へ切り替えてください"));
      // Freeze the exact PC rendering before the file picker opens.
      const blob = await new Promise<Blob>((resolve, reject) =>
        canvas.toBlob(
          (b) => (b ? resolve(b) : reject(new Error(t("PNG変換に失敗")))),
          "image/png",
        ),
      );
      const bytes = Array.from(new Uint8Array(await blob.arrayBuffer()));
      if (op === "print") {
        const image = await new Promise<string>((resolve, reject) => {
          const reader = new FileReader();
          reader.onload = () => resolve(String(reader.result));
          reader.onerror = () => reject(reader.error);
          reader.readAsDataURL(blob);
        });
        setPrintData({
          image,
          identity: f.record.identity,
          mode: plot.mode.toUpperCase(),
          capturedAt: new Date().toLocaleString(),
          note: t(
            "PC表示 / 受信byte位置・signed raw値（未校正）。本体画面の画像ではありません。",
          ),
        });
        return;
      }
      const path = await save({
        defaultPath: filename(op),
        filters: [
          { name: `PC waveform image ${op.toUpperCase()}`, extensions: [op] },
        ],
      });
      if (path) {
        await invoke("export_image", { path, bytes, format: op });
        setNotice(
          `PC表示の${op.toUpperCase()}を保存しました（本体画面の画像ではありません）`,
        );
      }
    }
  }
  const performRef = useRef(perform);
  performRef.current = perform;
  const busyRef = useRef(busy);
  busyRef.current = busy;
  useEffect(() => {
    const listener = (e: KeyboardEvent) => {
      if (
        e.target instanceof HTMLInputElement ||
        e.target instanceof HTMLSelectElement ||
        e.target instanceof HTMLTextAreaElement ||
        e.target instanceof HTMLButtonElement ||
        busyRef.current
      )
        return;
      let op: Operation | undefined;
      if ((e.metaKey || e.ctrlKey) && e.key === "o") op = "open";
      else if ((e.metaKey || e.ctrlKey) && e.key === "s") op = "save";
      else if ((e.metaKey || e.ctrlKey) && e.key === "p") op = "print";
      else if (e.code === "Space" && connectionRef.current) op = "pause";
      else if (e.key.toLowerCase() === "r") op = "reset";
      if (op) {
        e.preventDefault();
        void run(() => performRef.current(op!));
      }
    };
    window.addEventListener("keydown", listener);
    return () => window.removeEventListener("keydown", listener);
  }, []);
  const hasData = !!frame,
    header = frame?.record.header,
    timebase = field(header, "TIMEBASE"),
    sample = field(header, "SAMPLE");
  const rowCount = Math.max(
      frame?.values.CH1?.length ?? 0,
      frame?.values.CH2?.length ?? 0,
    ),
    tablePageSize = 500;
  const effectiveTablePage = Math.min(
    tablePage,
    Math.max(0, Math.ceil(rowCount / tablePageSize) - 1),
  );
  const stopped =
    frame?.record.consistency ===
    "manual_stop_observed; no acquisition_id_available";
  const can = (op: Operation) =>
    !busy &&
    native &&
    (op === "open"
      ? !connected
      : op === "pause" || op === "once" || op === "capture"
        ? connected
        : op === "reset"
          ? true
          : hasData);
  const button = (op: Operation, label: string, cls = "") => (
    <button
      className={cls}
      disabled={!can(op)}
      onClick={() => void run(() => perform(op))}
    >
      {label}
    </button>
  );
  return (
    <div className="app">
      <header className="masthead">
        <div className="brand">
          <span className="brandmark">∿</span>
          <div>
            <h1>
              OWON <span>Scope</span>
            </h1>
            <p>HDS200 · Desktop waveform workstation</p>
          </div>
        </div>
        <div className="head-right">
          <select
            aria-label="Language"
            value={language}
            onChange={(e) => setLanguage(e.target.value)}
          >
            <option value="ja">日本語</option>
            <option value="en">English</option>
          </select>
          <button onClick={() => setAbout(true)}>
            {t("ヘルプ / バージョン")}
          </button>
          <span className="version">TAURI / 0.4.1</span>
          <span className={`connection ${connected ? "online" : ""}`}>
            <i />
            {connected ? t("USB 接続中") : t("未接続")}
          </span>
        </div>
      </header>
      <nav className="toolbar" aria-label={t("取得とファイル操作")}>
        <select
          aria-label={t("接続先")}
          value={selected}
          disabled={connected || busy}
          onChange={(e) => setSelected(e.target.value)}
        >
          <option value="">{t("USBデバイスを選択")}</option>
          {devices.map((d) => (
            <option
              key={`${d.bus}:${d.address}`}
              value={`${d.bus}:${d.address}`}
            >
              {d.label}
            </option>
          ))}
        </select>
        <button
          title={t("USBを再検索")}
          disabled={connected || busy || !native}
          onClick={() => void run(refresh)}
        >
          {t("再検索")}
        </button>
        <button
          className={connected ? "" : "primary"}
          disabled={busy || !native || (!connected && !selected)}
          onClick={() => void run(toggleConnection)}
        >
          {connected ? t("切断") : t("接続")}
        </button>
        <span className="divider" />
        {button("pause", paused ? t("▶ 表示再開") : t("Ⅱ 表示停止"))}
        {button("once", t("1回取得"))}
        <span className="divider" />
        {button("save", t("JSON保存"))}
        {button("open", t("開く…"))}
        <button
          disabled={connected || busy || !native}
          onClick={() => void run(choosePlaybackFolder)}
        >
          {t("再生フォルダー…")}
        </button>
        <details className="recent">
          <summary>{t("履歴")}</summary>
          <div>
            {recent.length ? (
              recent.map((p) => (
                <button
                  key={p}
                  disabled={connected || busy}
                  title={p}
                  onClick={() =>
                    void run(async () => {
                      setPlaying(false);
                      await openPath(p);
                      setPlaylist([p]);
                      setPlayIndex(0);
                    })
                  }
                >
                  {basename(p)}
                </button>
              ))
            ) : (
              <small>{t("まだ履歴はありません")}</small>
            )}
          </div>
        </details>
        <span className="toolbar-spacer" />
        <select
          aria-label={t("表の出力形式")}
          value={tableFormat}
          onChange={(e) => setTableFormat(e.target.value as typeof tableFormat)}
        >
          {["csv", "txt", "xls"].map((v) => (
            <option key={v} value={v}>
              {v.toUpperCase()}
            </option>
          ))}
        </select>
        {button(tableFormat, t("表出力"))}
        <select
          aria-label={t("画像形式")}
          value={imageFormat}
          onChange={(e) => setImageFormat(e.target.value as typeof imageFormat)}
        >
          {["png", "bmp", "gif"].map((v) => (
            <option key={v} value={v}>
              {v.toUpperCase()}
            </option>
          ))}
        </select>
        {button(imageFormat, t("画像保存"))}
        {button("print", t("印刷…"))}
      </nav>
      {!native && (
        <div className="message warning">
          {t(
            "ブラウザーの表示確認モードです。USB接続・ファイル操作はTauriアプリで使用できます。模擬波形は表示しません。",
          )}
        </div>
      )}
      {error && (
        <div className="message error" role="alert">
          <span>{error}</span>
          <button onClick={() => setError("")}>{t("閉じる")}</button>
        </div>
      )}
      <main className="workspace">
        <section className="plot-panel">
          <div className="plot-heading">
            <div className="tabs">
              {(
                [
                  ["wave", t("波形")],
                  ["xy", "XY"],
                  ["fft", "FFT"],
                  ["table", t("データ表")],
                ] as [PlotMode, string][]
              ).map(([mode, label]) => (
                <button
                  className={plot.mode === mode ? "active" : ""}
                  key={mode}
                  onClick={() => updatePlot({ mode })}
                >
                  {label}
                </button>
              ))}
            </div>
            <span className="raw-badge">{t("RAW / 未校正")}</span>
            {button("reset", t("表示リセット"))}
          </div>
          <div className="plot-subheading">
            <span>
              <i style={{ background: plot.colors[0] }} />
              CH1
            </span>
            <span>
              <i style={{ background: plot.colors[1] }} />
              CH2
            </span>
            <span className="acquisition-label">
              {!hasData
                ? "WAITING"
                : connected && !paused
                  ? t("LIVE · 逐次取得")
                  : stopped
                    ? t("STOP確認済み")
                    : t("表示停止 / 保存データ")}
            </span>
          </div>
          {plot.mode === "table" ? (
            <div className="table-area">
              <div className="table-tools">
                <label>
                  {t("表示CH")}{" "}
                  <select
                    value={tableChannel}
                    onChange={(e) =>
                      setTableChannel(e.target.value as typeof tableChannel)
                    }
                  >
                    <option value="both">{t("両CH")}</option>
                    <option>CH1</option>
                    <option>CH2</option>
                  </select>
                </label>
                <span>
                  {t("各行は受信バイト。電圧値・ADCサンプルではありません。")}
                </span>
                <button
                  disabled={effectiveTablePage === 0}
                  onClick={() => setTablePage(effectiveTablePage - 1)}
                >
                  {t("前")}
                </button>
                <span>
                  {effectiveTablePage + 1}/
                  {Math.max(1, Math.ceil(rowCount / tablePageSize))}
                </span>
                <button
                  disabled={
                    (effectiveTablePage + 1) * tablePageSize >= rowCount
                  }
                  onClick={() => setTablePage(effectiveTablePage + 1)}
                >
                  {t("次")}
                </button>
              </div>
              <table>
                <thead>
                  <tr>
                    <th>{t("受信位置")}</th>
                    {tableChannel !== "CH2" && <th>CH1 raw</th>}
                    {tableChannel !== "CH1" && <th>CH2 raw</th>}
                  </tr>
                </thead>
                <tbody>
                  {Array.from(
                    {
                      length: Math.min(
                        tablePageSize,
                        rowCount - effectiveTablePage * tablePageSize,
                      ),
                    },
                    (_, row) => {
                      const i = row + effectiveTablePage * tablePageSize;
                      return (
                        <tr key={i}>
                          <td>{i}</td>
                          {tableChannel !== "CH2" && (
                            <td>{frame?.values.CH1?.[i] ?? "—"}</td>
                          )}
                          {tableChannel !== "CH1" && (
                            <td>{frame?.values.CH2?.[i] ?? "—"}</td>
                          )}
                        </tr>
                      );
                    },
                  )}
                </tbody>
              </table>
            </div>
          ) : (
            <ScopeCanvas
              frameRef={frameRef}
              frame={frame}
              canvasRef={canvasRef}
              config={plot}
              onCursors={(cursors) => updatePlot({ cursors })}
            />
          )}
          <div className="plot-footer">
            <span>
              {plot.mode === "fft"
                ? t(
                    "受信byte列のスペクトル。周波数Hz・電圧Vrmsの計測ではありません。",
                  )
                : plot.cursor !== "none"
                  ? t("ドラッグ: カーソル移動 · Shift+ドラッグ: 波形移動")
                  : t(
                      "ドラッグ: 波形移動 · ホイール: X拡大 · Shift+ホイール: Y拡大",
                    )}
            </span>
            <span>
              {frame
                ? `${frame.values.CH1?.length ?? 0} / ${frame.values.CH2?.length ?? 0} bytes`
                : "—"}
            </span>
          </div>
          <section className="measurement-strip">
            {(["CH1", "CH2"] as ChannelName[]).map((name, i) => {
              const stats = statistics(frame?.values[name]);
              return (
                <div key={name}>
                  <h3 style={{ color: plot.colors[i] }}>
                    {name} <small>{t("受信値")}</small>
                  </h3>
                  <dl>
                    <div>
                      <dt>peak-to-peak</dt>
                      <dd>
                        {Number.isFinite(stats.span)
                          ? stats.span.toFixed(0)
                          : "—"}
                      </dd>
                    </div>
                    <div>
                      <dt>{t("平均")}</dt>
                      <dd>
                        {Number.isFinite(stats.mean)
                          ? stats.mean.toFixed(2)
                          : "—"}
                      </dd>
                    </div>
                    <div>
                      <dt>RMS</dt>
                      <dd>
                        {Number.isFinite(stats.rms)
                          ? stats.rms.toFixed(2)
                          : "—"}
                      </dd>
                    </div>
                  </dl>
                </div>
              );
            })}
            <div className="axis-note">
              <h3>{t("時間・電圧換算")}</h3>
              <p>
                {t("受信byteの意味を検証後に有効化。")}
                <br />
                {t("本体のADC rateからΔtを推定しません。")}
              </p>
            </div>
          </section>
          {measurements && (
            <section className="device-measurements">
              <div className="device-measurement-heading">
                <h3>{t("本体の自動測定値")}</h3>
                <span>
                  {new Date(measurements.read_at_unix_ms).toLocaleTimeString()}
                  {t("取得 · 更新は「本体の測定値を取得」")}
                </span>
                <button onClick={() => setMeasurements(null)}>
                  {t("閉じる")}
                </button>
              </div>
              <table>
                <thead>
                  <tr>
                    <th>CH</th>
                    {[
                      t("周波数"),
                      t("周期"),
                      "Vpp",
                      t("最大"),
                      t("最小"),
                      t("平均"),
                    ].map((t) => (
                      <th key={t}>{t}</th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {["CH1", "CH2"].map((ch) => (
                    <tr key={ch}>
                      <th>{ch}</th>
                      {[
                        "FREQUENCY",
                        "PERIOD",
                        "PKPK",
                        "MAX",
                        "MIN",
                        "AVERAGE",
                      ].map((item, i) => {
                        const value = measurements.values[ch]?.[item];
                        return (
                          <td key={item}>
                            {value ?? "—"}
                            {value &&
                            /^[-+]?\d+(\.\d+)?([eE][-+]?\d+)?$/.test(value)
                              ? [" Hz", " s", " V", " V", " V", " V"][i]
                              : ""}
                          </td>
                        );
                      })}
                    </tr>
                  ))}
                </tbody>
              </table>
              <p className="small">
                {t(
                  "本体の測定応答をそのまま表示。CH・項目を順番に照会し、表示波形との同時性は保証しません。",
                )}
              </p>
            </section>
          )}
          {playlist.length > 0 && (
            <div className="playback">
              <button
                disabled={busy || playIndex === 0}
                onClick={() => void run(() => seek(playIndex - 1))}
              >
                ←
              </button>
              <button
                disabled={playlist.length < 2}
                onClick={() => setPlaying((p) => !p)}
              >
                {playing ? t("停止") : t("再生")}
              </button>
              <button
                disabled={busy || playIndex === playlist.length - 1}
                onClick={() => void run(() => seek(playIndex + 1))}
              >
                →
              </button>
              <input
                aria-label={t("再生位置")}
                type="range"
                min={0}
                max={playlist.length - 1}
                value={playIndex}
                onChange={(e) => {
                  setPlaying(false);
                  void run(() => seek(Number(e.target.value)));
                }}
              />
              <span>
                {playIndex + 1}/{playlist.length}
              </span>
              <label>
                <input
                  type="checkbox"
                  checked={reverse}
                  onChange={(e) => setReverse(e.target.checked)}
                />
                {t("逆方向")}
              </label>
              <input
                aria-label={t("再生間隔ms")}
                type="number"
                min={100}
                max={10000}
                value={playInterval}
                onChange={(e) =>
                  setPlayInterval(Math.max(100, Number(e.target.value)))
                }
              />
              <span>ms</span>
            </div>
          )}
        </section>
        <aside className="sidebar">
          <GeneratorControls
            key={connectionEpoch.current}
            connected={connected}
            busy={busy}
            identity={frame?.record.identity ?? ""}
            run={run}
            onNotice={setNotice}
          />
          <TriggerControls
            key={`trigger-${connectionEpoch.current}`}
            connected={connected}
            busy={busy}
            frame={frame}
            settingsRevision={settingsRevision}
            run={run}
            onNotice={setNotice}
          />
          <AutosetControls
            key={`auto-${connectionEpoch.current}`}
            connected={connected}
            busy={busy}
            identity={frame?.record.identity ?? ""}
            run={run}
            onNotice={setNotice}
            onChanged={() => {
              setSettingsRevision((v) => v + 1);
              setMeasurements(null);
            }}
          />
          <DeviceControls
            connected={connected}
            busy={busy}
            frame={frame}
            run={run}
            onNotice={setNotice}
            onMeasurements={setMeasurements}
          />
          <section>
            <h2>
              {t("取得")}
              <span>ACQUISITION</span>
            </h2>
            <label className="row">
              {t("照会の最小間隔")}{" "}
              <select
                value={interval}
                disabled={busy}
                onChange={(e) => {
                  const ms = Number(e.target.value);
                  setIntervalMs(ms);
                  if (connected)
                    void run(async () => {
                      await invoke("set_preview", { paused, intervalMs: ms });
                    });
                }}
              >
                {[17, 33, 100, 500, 1000].map((ms) => (
                  <option key={ms} value={ms}>
                    {ms} ms{ms === 17 ? t("（最速）") : ""}
                  </option>
                ))}
              </select>
            </label>
            <div className="metrics">
              <div>
                <strong>{connected ? fps.toFixed(1) : "—"}</strong>
                <span>{t("表示更新 / 秒")}</span>
              </div>
              <div>
                <strong>
                  {frame && connected ? frame.query_ms.toFixed(0) : "—"}
                  <small> ms</small>
                </strong>
                <span>{t("USB照会")}</span>
              </div>
            </div>
            <p className="small">
              {t(
                "表示停止はMacの取得を止めます。本体のRUN/STOPは変更しません。",
              )}
            </p>
            {button("capture", t("本体STOPを確認して取得・保存"), "wide")}
          </section>
          <section>
            <h2>
              {t("表示")}
              <span>DISPLAY</span>
            </h2>
            {(["CH1", "CH2"] as ChannelName[]).map((name, i) => (
              <div className="channel-row" key={name}>
                <label>
                  <input
                    type="checkbox"
                    checked={plot.show[i]}
                    onChange={(e) => {
                      const show = [...plot.show] as [boolean, boolean];
                      show[i] = e.target.checked;
                      updatePlot({ show });
                    }}
                  />
                  {name}
                </label>
                <input
                  aria-label={`${name}の色`}
                  type="color"
                  value={plot.colors[i]}
                  onChange={(e) => {
                    const colors = [...plot.colors] as [string, string];
                    colors[i] = e.target.value;
                    updatePlot({ colors });
                  }}
                />
                <label>
                  <input
                    type="checkbox"
                    checked={plot.inverted[i]}
                    onChange={(e) => {
                      const inverted = [...plot.inverted] as [boolean, boolean];
                      inverted[i] = e.target.checked;
                      updatePlot({ inverted });
                    }}
                  />
                  {t("反転")}
                </label>
              </div>
            ))}
            <div className="display-options">
              <label>
                <input
                  type="checkbox"
                  checked={plot.stacked}
                  onChange={(e) => updatePlot({ stacked: e.target.checked })}
                />
                {t("上下に分割")}
              </label>
              <label>
                <input
                  type="checkbox"
                  checked={plot.points}
                  onChange={(e) => updatePlot({ points: e.target.checked })}
                />
                {t("点表示")}
              </label>
              <label>
                <input
                  type="checkbox"
                  checked={plot.grid}
                  onChange={(e) => updatePlot({ grid: e.target.checked })}
                />
                {t("グリッド")}
              </label>
            </div>
            <div className="color-row">
              <label>
                {t("背景")}{" "}
                <input
                  aria-label={t("背景色")}
                  type="color"
                  value={plot.background}
                  onChange={(e) => updatePlot({ background: e.target.value })}
                />
              </label>
              <label>
                {t("格子")}{" "}
                <input
                  aria-label={t("格子色")}
                  type="color"
                  value={plot.gridColor}
                  onChange={(e) => updatePlot({ gridColor: e.target.value })}
                />
              </label>
            </div>
          </section>
          <section>
            <h2>
              {t("カーソル")}
              <span>CURSORS</span>
            </h2>
            <select
              className="wide"
              value={plot.cursor}
              disabled={plot.mode !== "wave" || plot.stacked}
              onChange={(e) =>
                updatePlot({ cursor: e.target.value as PlotConfig["cursor"] })
              }
            >
              <option value="none">{t("なし")}</option>
              <option value="x">{t("X（受信位置）")}</option>
              <option value="y">{t("Y（受信値）")}</option>
              <option value="both">X + Y</option>
            </select>
            {plot.stacked && (
              <p className="small">
                {t("カーソルは重ね合わせ表示で使用します。")}
              </p>
            )}
            {plot.cursor !== "none" && (
              <>
                <div className="cursor-inputs">
                  {plot.cursors.map((value, i) => (
                    <label key={i}>
                      {["XA", "XB", "YA", "YB"][i]}
                      <input
                        aria-label={["XA", "XB", "YA", "YB"][i]}
                        type="number"
                        step="0.1"
                        value={Number(value.toFixed(1))}
                        onChange={(e) => {
                          const cursors = [
                            ...plot.cursors,
                          ] as PlotConfig["cursors"];
                          cursors[i] = Number(e.target.value);
                          updatePlot({ cursors });
                        }}
                      />
                    </label>
                  ))}
                </div>
                <div className="cursor-result">
                  <span>
                    ΔX <b>{(plot.cursors[1] - plot.cursors[0]).toFixed(1)}</b>{" "}
                    byte
                  </span>
                  <span>
                    ΔY <b>{(plot.cursors[3] - plot.cursors[2]).toFixed(1)}</b>{" "}
                    raw
                  </span>
                </div>
                <p className="small">{t("Δt / ΔV は校正後に対応します。")}</p>
              </>
            )}
          </section>
          <section>
            <h2>
              {t("解析")}
              <span>ANALYSIS</span>
            </h2>
            <label className="row">
              {t("演算")}{" "}
              <select
                value={plot.math}
                onChange={(e) =>
                  updatePlot({ math: e.target.value as PlotConfig["math"] })
                }
              >
                {[
                  ["none", t("なし")],
                  ["add", "CH1 + CH2"],
                  ["subtract", "CH1 − CH2"],
                  ["reverse", "CH2 − CH1"],
                  ["multiply", "CH1 × CH2"],
                  ["divide", "CH1 / CH2"],
                  ["divide-reverse", "CH2 / CH1"],
                  ["square1", "CH1²"],
                  ["square2", "CH2²"],
                ].map(([v, l]) => (
                  <option key={v} value={v}>
                    {l}
                  </option>
                ))}
              </select>
            </label>
            {plot.mode === "fft" && (
              <>
                <label className="row">
                  {t("窓関数")}{" "}
                  <select
                    value={plot.fftWindow}
                    onChange={(e) =>
                      updatePlot({
                        fftWindow: e.target.value as PlotConfig["fftWindow"],
                      })
                    }
                  >
                    {["rectangular", "hann", "hamming", "blackman"].map((w) => (
                      <option key={w}>{w}</option>
                    ))}
                  </select>
                </label>
                <label>
                  <input
                    type="checkbox"
                    checked={plot.fftDb}
                    onChange={(e) => updatePlot({ fftDb: e.target.checked })}
                  />
                  {t("dB（基準 1 raw RMS）")}
                </label>
                <p className="small">
                  {t(
                    "平均値を除去し、先頭の2の累乗長を使用。画面byte列の解析です。",
                  )}
                </p>
              </>
            )}
          </section>
          <section>
            <h2>
              {t("自動保存")}
              <span>RECORDING</span>
            </h2>
            <div className="folder-row">
              <button
                disabled={!native || busy || recording}
                onClick={() =>
                  void run(async () => {
                    const path = await open({ directory: true });
                    if (typeof path === "string") setFolder(path);
                  })
                }
              >
                {t("保存先…")}
              </button>
              <span title={folder}>
                {folder ? basename(folder) : t("未指定")}
              </span>
            </div>
            <div className="record-row">
              <input
                aria-label={t("自動保存間隔ms")}
                type="number"
                min={500}
                max={60000}
                value={recordInterval}
                disabled={recording}
                onChange={(e) =>
                  setRecordInterval(
                    Math.min(60000, Math.max(500, Number(e.target.value))),
                  )
                }
              />
              <span>ms</span>
              <button
                disabled={!connected || !folder || busy}
                onClick={() =>
                  void run(async () => {
                    const path = await invoke<string | null>("set_recording", {
                      folder: recording ? null : folder,
                      intervalMs: recordInterval,
                    });
                    setRecording(!recording);
                    if (path) {
                      setRecordFolder(path);
                      setNotice(`記録先: ${path}`);
                    }
                  })
                }
              >
                {recording ? t("保存停止") : t("保存開始")}
              </button>
            </div>
            <p className="small">
              {recording
                ? `● ${t("記録中")} · ${savedCount} ${t("ファイル")} · ${skippedCount} ${t("回スキップ（表示停止・未更新）")}`
                : t(
                    "ライブの逐次取得をJSONに記録。同一収録やΔt測定を保証する保存ではありません。",
                  )}
            </p>
            {!recording && recordFolder && (
              <p className="small">
                {t("保存済み")}: {savedCount} {t("ファイル")} · {skippedCount}{" "}
                {t("回スキップ（表示停止・未更新）")}
              </p>
            )}
            {recordFolder && (
              <p className="small" title={recordFolder}>
                {t("記録先:")}
                {basename(recordFolder)}
              </p>
            )}
            <p className="small">
              {t(
                "Rust側で保存。背面でも動作し、表示停止・未更新なら重複保存しません。Macのスリープ中は停止します。",
              )}
            </p>
          </section>
          <section>
            <h2>
              {t("本体情報")}
              <span>DEVICE</span>
            </h2>
            <dl className="device-info">
              <dt>{t("機種")}</dt>
              <dd>{frame?.record.identity ?? "—"}</dd>
              <dt>{t("状態 / 時間軸")}</dt>
              <dd>
                {frame?.record.status_after ?? "—"} /{" "}
                {text(field(timebase, "SCALE"))}
              </dd>
              <dt>{t("ADC rate / メモリ")}</dt>
              <dd>
                {text(field(sample, "SAMPLERATE"))} /{" "}
                {text(field(sample, "DEPMEM"))}
              </dd>
              {(["CH1", "CH2"] as ChannelName[]).map((name) => (
                <div key={name}>
                  <dt>
                    {name}
                    {t("本体設定")}
                  </dt>
                  <dd>
                    {text(field(channelInfo(frame, name), "SCALE"))} /{" "}
                    {text(field(channelInfo(frame, name), "PROBE"))} /{" "}
                    {text(field(channelInfo(frame, name), "COUPLING"))}
                  </dd>
                </div>
              ))}
              <dt>{t("設定情報の経過時間")}</dt>
              <dd>{frame ? `${frame.metadata_age_ms.toFixed(0)} ms` : "—"}</dd>
            </dl>
            <p className="small">
              {t("本体設定の参考表示。グラフの校正には使用していません。")}
            </p>
          </section>
          <section className="roadmap">
            <h2>{t("次の実装")}</h2>
            <p>
              {t("時間・電圧の校正 / BIN互換")}
              <br />
              {t("自然言語AI操作（未実装・API送信なし）")}
            </p>
          </section>
        </aside>
      </main>
      <footer className="statusbar">
        <span>
          {busy
            ? t("処理中…")
            : notice ||
              t("接続を選択してください。旧Python版はそのまま残しています。")}
        </span>
        <span>
          {connected ? "USB BULK · SCPI" : "LOCAL"}{" "}
          <span className="status-separator">/</span> CH1 · CH2
        </span>
      </footer>
      {printData && (
        <PrintPreview
          data={printData}
          close={() => setPrintData(null)}
          report={report}
        />
      )}
      {about && (
        <div
          className="modal-backdrop"
          role="dialog"
          aria-modal="true"
          aria-label={t("ヘルプ / バージョン")}
        >
          <section className="help-modal">
            <h2>OWON Scope 0.4.1</h2>
            <p>Tauri 2 · React · Rust / HDS200 USB</p>
            <p>
              {t(
                "公式PCソフトの機能を独立実装中。OWON公式アプリではありません。",
              )}
            </p>
            <p>{t("CH1／CH2は汎用の入力チャンネルです。")}</p>
            <ul>
              <li>
                {t("Space: 表示停止・再開（本体RUN/STOPではありません）")}
              </li>
              <li>
                {t("Cmd/Ctrl+S: JSON保存 · +O: 開く · +P: 印刷プレビュー")}
              </li>
              <li>
                {t(
                  "R: 表示リセット · ドラッグ: 移動 · ホイール: X拡大 · Shift: Y拡大",
                )}
              </li>
              <li>{t("本体操作: 取得設定を変更し、変更前後の応答を表示")}</li>
              <li>
                {t(
                  "トリガー: 現在値を読取り、1項目ずつ適用・読戻し。Singleの再待受は本体RUN/STOP",
                )}
              </li>
              <li>
                {t("GEN OUT: 1項目ずつ適用。出力ONには接続先の確認が必要")}
              </li>
              <li>
                {t("連続保存: 保存先内に新しいセッションフォルダーを作成")}
              </li>
              <li>
                {t("波形軸は未校正。時間・電圧カーソル、公式BIN互換は未対応")}
              </li>
            </ul>
            <p>
              {t("AI・外部API通信はありません。ファイルを上書きしません。")}
            </p>
            <button onClick={() => setAbout(false)}>{t("閉じる")}</button>
          </section>
        </div>
      )}
    </div>
  );
}
