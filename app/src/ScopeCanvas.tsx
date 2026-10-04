import { useEffect, useRef, useState, type RefObject } from "react";
import {
  mathValues,
  statistics,
  spectrum,
  type Frame,
  type PlotMode,
  type CursorMode,
  type MathOp,
  type WindowFn,
} from "./model";

export interface PlotConfig {
  mode: PlotMode;
  stacked: boolean;
  grid: boolean;
  points: boolean;
  show: [boolean, boolean];
  colors: [string, string];
  background: string;
  gridColor: string;
  inverted: [boolean, boolean];
  math: MathOp;
  fftWindow: WindowFn;
  fftDb: boolean;
  cursor: CursorMode;
  cursors: [number, number, number, number];
  reset: number;
}
export default function ScopeCanvas({
  frameRef,
  frame,
  config,
  onCursors,
  canvasRef,
}: {
  frameRef: RefObject<Frame | null>;
  frame: Frame | null;
  config: PlotConfig;
  onCursors: (c: PlotConfig["cursors"]) => void;
  canvasRef: RefObject<HTMLCanvasElement | null>;
}) {
  const host = useRef<HTMLDivElement>(null);
  const [size, setSize] = useState([800, 500]);
  const [view, setView] = useState<{
    x: [number, number];
    y: [number, number];
  } | null>(null);
  const drag = useRef<{
    x: number;
    y: number;
    view: { x: [number, number]; y: [number, number] };
    cursor: number | null;
  } | null>(null);
  const configRef = useRef(config);
  configRef.current = config;
  const viewRef = useRef(view);
  viewRef.current = view;
  const sizeRef = useRef(size);
  sizeRef.current = size;
  const repaint = useRef<() => void>(() => {});
  useEffect(() => {
    setView(null);
  }, [config.reset, config.mode, config.fftDb, config.math]);
  useEffect(() => {
    const ob = new ResizeObserver(([entry]) =>
      setSize([entry.contentRect.width, entry.contentRect.height]),
    );
    if (host.current) ob.observe(host.current);
    return () => ob.disconnect();
  }, []);
  const bounds = () => {
    const f = frameRef.current,
      c = configRef.current,
      n = Math.max(f?.values.CH1?.length ?? 0, f?.values.CH2?.length ?? 0, 2);
    if (viewRef.current) return viewRef.current;
    if (c.mode === "xy")
      return {
        x: [-128, 128] as [number, number],
        y: [-128, 128] as [number, number],
      };
    if (c.mode === "fft")
      return {
        x: [0, 0.5] as [number, number],
        y: (c.fftDb ? [-100, 45] : [0, 128]) as [number, number],
      };
    let range: [number, number] = [-128, 128];
    if (c.math !== "none" && f) {
      const m = mathValues(f.values.CH1, f.values.CH2, c.math).filter(
        Number.isFinite,
      );
      if (m.length) {
        const stats = statistics(m);
        range = [Math.min(-128, stats.min), Math.max(128, stats.max)];
      }
    }
    return { x: [0, n - 1] as [number, number], y: range };
  };
  useEffect(() => {
    let id = 0,
      previous = "",
      previousFrame: Frame | null = null;
    const paint = () => {
      const canvas = canvasRef.current,
        f = frameRef.current,
        c = configRef.current,
        [width, height] = sizeRef.current;
      const key = JSON.stringify([c, viewRef.current, width, height]);
      if (
        !canvas ||
        width < 1 ||
        height < 1 ||
        (previous === key && previousFrame === f)
      )
        return;
      previous = key;
      previousFrame = f;
      const dpr = window.devicePixelRatio || 1;
      canvas.width = Math.round(width * dpr);
      canvas.height = Math.round(height * dpr);
      const ctx = canvas.getContext("2d")!;
      ctx.scale(dpr, dpr);
      ctx.fillStyle = c.background;
      ctx.fillRect(0, 0, width, height);
      const l = 54,
        r = width - 20,
        t = 30,
        b = height - 38,
        w = r - l,
        h = b - t,
        v = bounds();
      const X = (x: number) => l + ((x - v.x[0]) / (v.x[1] - v.x[0])) * w;
      const Y = (y: number, channel = 0) => {
        if (c.stacked && c.mode === "wave") {
          const top = t + (channel * h) / 2;
          return top + h / 2 - (((y - v.y[0]) / (v.y[1] - v.y[0])) * h) / 2;
        }
        return b - ((y - v.y[0]) / (v.y[1] - v.y[0])) * h;
      };
      ctx.font = "11px ui-monospace, SFMono-Regular, monospace";
      if (c.grid) {
        ctx.strokeStyle = c.gridColor;
        ctx.lineWidth = 0.6;
        ctx.beginPath();
        for (let i = 0; i <= 10; i++) {
          const x = l + (w * i) / 10;
          ctx.moveTo(x, t);
          ctx.lineTo(x, b);
        }
        for (let i = 0; i <= 8; i++) {
          const y = t + (h * i) / 8;
          ctx.moveTo(l, y);
          ctx.lineTo(r, y);
        }
        ctx.stroke();
      }
      ctx.fillStyle = "#8193a9";
      for (let i = 0; i <= 5; i++) {
        const x = v.x[0] + ((v.x[1] - v.x[0]) * i) / 5;
        ctx.fillText(
          c.mode === "fft" ? x.toFixed(2) : x.toFixed(0),
          X(x) - 8,
          b + 20,
        );
      }
      for (let i = 0; i <= 4; i++) {
        const y = v.y[0] + ((v.y[1] - v.y[0]) * i) / 4;
        ctx.fillText(y.toFixed(0), 10, Y(y) - 2);
      }
      ctx.fillText(
        c.mode === "xy"
          ? "X: CH1 raw / Y: CH2 raw"
          : c.mode === "fft"
            ? "cycles / byte · " + (c.fftDb ? "dB re 1 raw RMS" : "raw RMS")
            : "screen byte index · Y: raw int8",
        l,
        height - 5,
      );
      ctx.save();
      ctx.beginPath();
      ctx.rect(l, t, w, h);
      ctx.clip();
      const line = (xs: number[], ys: number[], color: string, channel = 0) => {
        ctx.strokeStyle = color;
        ctx.fillStyle = color;
        ctx.lineWidth = 1.5;
        ctx.beginPath();
        let pen = false;
        for (let i = 0; i < ys.length; i++) {
          const x = X(xs[i]),
            y = Y(ys[i], channel);
          if (!Number.isFinite(x) || !Number.isFinite(y)) {
            pen = false;
            continue;
          }
          if (c.points) ctx.fillRect(x - 1, y - 1, 2, 2);
          else {
            if (pen) ctx.lineTo(x, y);
            else ctx.moveTo(x, y);
            pen = true;
          }
        }
        if (!c.points) ctx.stroke();
      };
      if (f) {
        const a = (f.values.CH1 ?? []).map((x) => (c.inverted[0] ? -x : x)),
          d = (f.values.CH2 ?? []).map((x) => (c.inverted[1] ? -x : x));
        if (c.mode === "xy") {
          const n = Math.min(a.length, d.length);
          if (c.show[0] && c.show[1])
            line(a.slice(0, n), d.slice(0, n), c.colors[1]);
        } else {
          [a, d].forEach((values, ch) => {
            if (!c.show[ch]) return;
            const ys =
              c.mode === "fft"
                ? spectrum(values, c.fftWindow, c.fftDb)
                : values;
            const xs = ys.map((_, i) =>
              c.mode === "fft" ? i / (2 * (ys.length - 1)) : i,
            );
            line(xs, ys, c.colors[ch], ch);
          });
          if (c.mode === "wave" && c.math !== "none") {
            const ys = mathValues(a, d, c.math);
            line(
              ys.map((_, i) => i),
              ys,
              "#bb9af7",
            );
          }
        }
      }
      if (c.cursor !== "none" && c.mode === "wave" && !c.stacked) {
        ctx.setLineDash([5, 5]);
        ctx.lineWidth = 1;
        if (c.cursor === "x" || c.cursor === "both")
          c.cursors.slice(0, 2).forEach((value, i) => {
            ctx.strokeStyle = i ? "#d9afff" : "#e9edf7";
            ctx.beginPath();
            ctx.moveTo(X(value), t);
            ctx.lineTo(X(value), b);
            ctx.stroke();
            ctx.fillStyle = ctx.strokeStyle;
            ctx.fillText(i ? "B" : "A", X(value) + 5, t + 16);
          });
        if (c.cursor === "y" || c.cursor === "both")
          c.cursors.slice(2).forEach((value, i) => {
            ctx.strokeStyle = i ? "#d9afff" : "#e9edf7";
            ctx.beginPath();
            ctx.moveTo(l, Y(value));
            ctx.lineTo(r, Y(value));
            ctx.stroke();
          });
        ctx.setLineDash([]);
      }
      ctx.restore();
      if (!f) {
        ctx.fillStyle = "#bdc9d8";
        ctx.font = "18px system-ui";
        ctx.textAlign = "center";
        ctx.fillText(
          "実機に接続、または保存データを開いてください",
          width / 2,
          height / 2,
        );
        ctx.textAlign = "left";
      }
      if (c.mode === "xy" && (!f?.values.CH1 || !f?.values.CH2)) {
        ctx.fillStyle = "#d7aa61";
        ctx.fillText("XY表示には両CHのデータが必要です", l, t + 20);
      }
    };
    repaint.current = paint;
    const tick = () => {
      id = requestAnimationFrame(tick);
      paint();
    };
    tick();
    const fallback = window.setInterval(paint, 17);
    return () => {
      cancelAnimationFrame(id);
      clearInterval(fallback);
    };
  }, []);
  // WKWebView may suspend rAF while occluded. Still paint a loaded file or a changed view immediately.
  useEffect(() => {
    repaint.current();
  }, [frame, config, size, view]);
  const coords = (e: React.PointerEvent<HTMLCanvasElement>) => {
    const rect = e.currentTarget.getBoundingClientRect();
    return { x: e.clientX - rect.left, y: e.clientY - rect.top };
  };
  const moveCursor = (index: number, p: { x: number; y: number }) => {
    const [w, h] = sizeRef.current,
      v = bounds(),
      c = [...configRef.current.cursors] as PlotConfig["cursors"];
    c[index] =
      index < 2
        ? v.x[0] + ((p.x - 54) / (w - 74)) * (v.x[1] - v.x[0])
        : v.y[1] - ((p.y - 30) / (h - 68)) * (v.y[1] - v.y[0]);
    onCursors(c);
  };
  return (
    <div className="canvas-host" ref={host}>
      <canvas
        aria-label="波形表示。ドラッグで移動、ホイールで拡大。カーソル有効時はドラッグでカーソルを移動"
        ref={canvasRef}
        onPointerDown={(e) => {
          const p = coords(e),
            v = bounds(),
            [w, h] = sizeRef.current,
            c = configRef.current;
          let cursor: number | null = null;
          if (
            c.cursor !== "none" &&
            c.mode === "wave" &&
            !c.stacked &&
            !e.shiftKey
          ) {
            const candidates = [
              ...(c.cursor === "x" || c.cursor === "both" ? [0, 1] : []),
              ...(c.cursor === "y" || c.cursor === "both" ? [2, 3] : []),
            ];
            cursor = candidates.reduce((best, i) => {
              const dist = (k: number) =>
                k < 2
                  ? Math.abs(
                      54 +
                        ((c.cursors[k] - v.x[0]) / (v.x[1] - v.x[0])) *
                          (w - 74) -
                        p.x,
                    )
                  : Math.abs(
                      30 +
                        ((v.y[1] - c.cursors[k]) / (v.y[1] - v.y[0])) *
                          (h - 68) -
                        p.y,
                    );
              return dist(i) < dist(best) ? i : best;
            }, candidates[0]);
            moveCursor(cursor, p);
          }
          drag.current = { ...p, view: v, cursor };
          e.currentTarget.setPointerCapture(e.pointerId);
        }}
        onPointerMove={(e) => {
          const start = drag.current;
          if (!start) return;
          const p = coords(e);
          if (start.cursor !== null) {
            moveCursor(start.cursor, p);
            return;
          }
          const [w, h] = sizeRef.current,
            dx =
              ((p.x - start.x) / (w - 74)) *
              (start.view.x[1] - start.view.x[0]),
            dy =
              ((p.y - start.y) / (h - 68)) *
              (start.view.y[1] - start.view.y[0]);
          setView({
            x: [start.view.x[0] - dx, start.view.x[1] - dx],
            y: [start.view.y[0] + dy, start.view.y[1] + dy],
          });
        }}
        onPointerUp={() => {
          drag.current = null;
        }}
        onPointerCancel={() => {
          drag.current = null;
        }}
        onWheel={(e) => {
          const v = bounds(),
            factor = e.deltaY > 0 ? 1.15 : 1 / 1.15,
            axis = e.shiftKey ? "y" : "x",
            span = v[axis][1] - v[axis][0];
          if (span * factor < 0.0001 || span * factor > 1e8) return;
          const mid = (v[axis][0] + v[axis][1]) / 2;
          setView({
            ...v,
            [axis]: [mid - (span * factor) / 2, mid + (span * factor) / 2],
          });
        }}
      />
    </div>
  );
}
