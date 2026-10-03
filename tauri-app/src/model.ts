export type ChannelName = "CH1" | "CH2";
export type JsonObject = Record<string, unknown>;
export interface WaveRecord extends JsonObject {
  schema_version: number;
  source: string;
  identity: string;
  status_after: string;
  consistency: string;
  header: JsonObject;
  channels: Record<string, { byte_count: number; payload_hex: string }>;
  sensor_assignments?: Record<ChannelName, "audio" | "light">;
}
export interface Frame {
  record: WaveRecord;
  values: Partial<Record<ChannelName, number[]>>;
  query_ms: number;
  metadata_age_ms: number;
  sequence: number;
}
export interface Device {
  bus: number;
  address: number;
  label: string;
}
export type PlotMode = "wave" | "xy" | "fft" | "table";
export type CursorMode = "none" | "x" | "y" | "both";
export type MathOp =
  | "none"
  | "add"
  | "subtract"
  | "reverse"
  | "multiply"
  | "divide"
  | "divide-reverse"
  | "square1"
  | "square2";
export function field(value: unknown, key: string): unknown {
  if (!value || typeof value !== "object" || Array.isArray(value))
    return undefined;
  return Object.entries(value).find(
    ([k]) => k.toLowerCase() === key.toLowerCase(),
  )?.[1];
}
export function text(value: unknown): string {
  return value === undefined || value === null ? "—" : String(value);
}
export function channelInfo(
  frame: Frame | null,
  channel: ChannelName,
): unknown {
  const channels = field(frame?.record.header, "CHANNEL");
  return Array.isArray(channels)
    ? channels.find((c) => text(field(c, "NAME")).toUpperCase() === channel)
    : undefined;
}
export function statistics(values: number[] = []) {
  if (!values.length)
    return { min: NaN, max: NaN, mean: NaN, rms: NaN, span: NaN };
  let min = Infinity,
    max = -Infinity,
    sum = 0,
    square = 0;
  for (const v of values) {
    min = Math.min(min, v);
    max = Math.max(max, v);
    sum += v;
    square += v * v;
  }
  return {
    min,
    max,
    mean: sum / values.length,
    rms: Math.sqrt(square / values.length),
    span: max - min,
  };
}
export function mathValues(
  a: number[] = [],
  b: number[] = [],
  op: MathOp,
): number[] {
  if (op === "none") return [];
  const len =
    op === "square1"
      ? a.length
      : op === "square2"
        ? b.length
        : Math.min(a.length, b.length);
  return Array.from({ length: len }, (_, i) => {
    const x = a[i],
      y = b[i];
    switch (op) {
      case "add":
        return x + y;
      case "subtract":
        return x - y;
      case "reverse":
        return y - x;
      case "multiply":
        return x * y;
      case "divide":
        return y === 0 ? NaN : x / y;
      case "divide-reverse":
        return x === 0 ? NaN : y / x;
      case "square1":
        return x * x;
      case "square2":
        return y * y;
    }
  });
}
export type WindowFn = "rectangular" | "hann" | "hamming" | "blackman";
/** Radix-2 FFT. Frequency is cycles/received-byte, NOT Hz. Amplitude is raw units. */
export function spectrum(
  values: number[],
  windowFn: WindowFn,
  db: boolean,
): number[] {
  if (values.length < 2) return [];
  const n = 2 ** Math.floor(Math.log2(values.length));
  const mean = values.slice(0, n).reduce((a, b) => a + b, 0) / n;
  const re = new Float64Array(n),
    im = new Float64Array(n);
  let coherent = 0;
  for (let i = 0; i < n; i++) {
    const phase = (2 * Math.PI * i) / (n - 1);
    const w =
      windowFn === "hann"
        ? (1 - Math.cos(phase)) / 2
        : windowFn === "hamming"
          ? 0.54 - 0.46 * Math.cos(phase)
          : windowFn === "blackman"
            ? 0.42 - 0.5 * Math.cos(phase) + 0.08 * Math.cos(2 * phase)
            : 1;
    re[i] = (values[i] - mean) * w;
    coherent += w;
  }
  for (let i = 1, j = 0; i < n; i++) {
    let bit = n >> 1;
    for (; j & bit; bit >>= 1) j ^= bit;
    j ^= bit;
    if (i < j) [re[i], re[j]] = [re[j], re[i]];
  }
  for (let length = 2; length <= n; length <<= 1) {
    for (let i = 0; i < n; i += length)
      for (let j = 0; j < length / 2; j++) {
        const angle = (-2 * Math.PI * j) / length,
          k = i + j,
          l = k + length / 2;
        const tr = re[l] * Math.cos(angle) - im[l] * Math.sin(angle),
          ti = re[l] * Math.sin(angle) + im[l] * Math.cos(angle);
        re[l] = re[k] - tr;
        im[l] = im[k] - ti;
        re[k] += tr;
        im[k] += ti;
      }
  }
  return Array.from({ length: n / 2 + 1 }, (_, i) => {
    const magnitude =
      (Math.hypot(re[i], im[i]) * (i === 0 || i === n / 2 ? 1 : 2)) /
      coherent /
      (i === 0 || i === n / 2 ? 1 : Math.SQRT2);
    return db ? 20 * Math.log10(Math.max(1e-9, magnitude)) : magnitude;
  });
}
export function filename(extension: string) {
  return `owon-${new Date().toISOString().replace(/[:.]/g, "-")}.${extension}`;
}
