export type GeneratorParameter =
  "waveform" | "frequency" | "amplitude" | "offset" | "output";
export interface GeneratorSnapshot {
  waveform: string;
  frequency: string;
  period: string;
  amplitude: string;
  offset: string;
  output: string;
  load: string;
  read_at_unix_ms: number;
}
export interface GeneratorReply {
  command: string;
  before: GeneratorSnapshot;
  after: GeneratorSnapshot;
  verified: boolean;
  preserved: boolean;
}
export const waveforms = ["SINE", "SQUARE", "RAMP", "PULSE"] as const;
export const frequencyLimits: Record<string, number> = {
  SINE: 10e6,
  SQUARE: 2e6,
  RAMP: 1e6,
  PULSE: 5e6,
};
export function supportsGenerator(identity: string) {
  return identity.split(",")[1]?.trim().toUpperCase() === "HDS25S";
}
export function unitsFor(parameter: GeneratorParameter): string[] {
  return parameter === "frequency"
    ? ["Hz", "kHz", "MHz"]
    : parameter === "amplitude"
      ? ["Vpp", "mVpp"]
      : parameter === "offset"
        ? ["V", "mV"]
        : [""];
}
export function canonicalGeneratorValue(
  parameter: GeneratorParameter,
  value: string,
  unit: string,
) {
  value = value.trim();
  if (parameter === "waveform") {
    const v = value.toUpperCase();
    if (!waveforms.some((w) => w === v)) throw new Error("未対応の波形種です");
    return v;
  }
  if (parameter === "output") {
    if (!["ON", "OFF"].includes(value.toUpperCase()))
      throw new Error("出力はON/OFFです");
    return value.toUpperCase();
  }
  if (
    !unitsFor(parameter).includes(unit) ||
    !/^[+-]?(?:\d+\.?\d*|\.\d+)(?:e[+-]?\d+)?$/i.test(value)
  )
    throw new Error("有限の数値と正しい単位を入力してください");
  const n =
    Number(value) *
    ({ Hz: 1, kHz: 1000, MHz: 1e6, Vpp: 1, mVpp: 0.001, V: 1, mV: 0.001 }[
      unit
    ] ?? NaN);
  if (!Number.isFinite(n))
    throw new Error("有限の数値と正しい単位を入力してください");
  return String(n);
}
export function generatorValidation(
  snapshot: GeneratorSnapshot,
  parameter: GeneratorParameter,
  value: string,
): string | null {
  if (parameter === "output" && value === "OFF") return null;
  if (parameter !== "output" && snapshot.output.toUpperCase() !== "ON")
    return "設定変更は、接続先を確認して出力ONにした後で行ってください";
  const next = { ...snapshot, [parameter]: value };
  const max = frequencyLimits[next.waveform.toUpperCase()];
  if (!max) return "未対応の波形種です";
  const frequency = Number(next.frequency),
    amplitude = Number(next.amplitude),
    offset = Number(next.offset);
  if (![frequency, amplitude, offset].every(Number.isFinite))
    return "本体の数値応答を確認してください";
  if (frequency < 0.1 || frequency > max)
    return "周波数が波形種の許可範囲外です";
  if (amplitude < 0.02 || amplitude > 5) return "振幅は0.02〜5 Vppです";
  if (Math.abs(offset) + amplitude / 2 > 2.5 + 1e-12)
    return "安全制限: |オフセット| + 振幅/2 ≤ 2.5 V";
  if (next.load.toUpperCase() !== "INF")
    return "負荷INF以外は設定変更未対応です";
  if (!["ON", "OFF"].includes(next.output.toUpperCase()))
    return "出力状態が不明です";
  return null;
}
export function generatorDraft(
  snapshot: GeneratorSnapshot,
  parameter: GeneratorParameter,
) {
  const raw = snapshot[parameter],
    n = Number(raw);
  if (parameter === "waveform" || parameter === "output")
    return { value: raw.toUpperCase(), unit: "" };
  if (parameter === "frequency" && Number.isFinite(n)) {
    const factor = n >= 1e6 ? 1e6 : n >= 1000 ? 1000 : 1;
    return {
      value: String(n / factor),
      unit: factor === 1e6 ? "MHz" : factor === 1000 ? "kHz" : "Hz",
    };
  }
  return {
    value: Number.isFinite(n) ? String(n) : raw,
    unit: unitsFor(parameter)[0],
  };
}
export function generatorQuantity(
  raw: string,
  kind: "frequency" | "period" | "amplitude" | "offset",
) {
  const n = Number(raw);
  if (!Number.isFinite(n)) return raw;
  const magnitude = Math.abs(n);
  const [factor, unit] =
    kind === "frequency"
      ? magnitude >= 1e6
        ? [1e6, "MHz"]
        : magnitude >= 1000
          ? [1000, "kHz"]
          : [1, "Hz"]
      : kind === "period"
        ? magnitude >= 1
          ? [1, "s"]
          : magnitude >= 1e-3
            ? [1e-3, "ms"]
            : magnitude >= 1e-6
              ? [1e-6, "µs"]
              : [1e-9, "ns"]
        : [1, kind === "amplitude" ? "Vpp" : "V"];
  return `${Number((n / Number(factor)).toPrecision(8))} ${unit}`;
}
