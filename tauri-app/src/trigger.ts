export type TriggerParameter =
  "source" | "coupling" | "edge" | "sweep" | "level";
export interface TriggerSnapshot {
  source: string;
  coupling: string;
  edge: string;
  sweep: string;
  level: string;
  status: string;
  read_at_unix_ms: number;
}
export interface TriggerReply {
  command: string;
  before: TriggerSnapshot;
  after: TriggerSnapshot;
  verified: boolean;
  preserved: boolean;
  linked_level_changed: boolean;
  readback_stable: boolean;
  readback_reads: number;
  requested_value: string;
  rounding_workaround: boolean;
}
export const triggerOptions: Record<TriggerParameter, string[] | null> = {
  source: ["CH1", "CH2"],
  coupling: ["DC", "AC"],
  edge: ["RISE", "FALL"],
  sweep: ["AUTO", "NORMAL", "SINGLE"],
  level: null,
};
export function supportsTrigger(identity: string) {
  const p = identity
    .toUpperCase()
    .split(",")
    .map((s) => s.trim());
  return (
    p.length === 4 && p[0] === "OWON" && p[1] === "HDS25S" && p[3] === "V12.1.0"
  );
}
export function triggerVolts(value: string): number | null {
  const m = value
    .trim()
    .match(
      /^([-+]?(?:\d+(?:\.\d*)?|\.\d+)(?:e[-+]?\d+)?)\s*(pV|nV|uV|mV|V)?$/i,
    );
  if (!m) return null;
  const factors: Record<string, number> = {
    PV: 1e-12,
    NV: 1e-9,
    UV: 1e-6,
    MV: 1e-3,
    V: 1,
  };
  const v = Number(m[1]) * factors[(m[2] ?? "V").toUpperCase()];
  return Number.isFinite(v) ? v : null;
}
export function triggerDraft(snapshot: TriggerSnapshot, p: TriggerParameter) {
  return p === "level"
    ? String(triggerVolts(snapshot.level) ?? "")
    : snapshot[p].toUpperCase();
}
export function triggerValue(p: TriggerParameter, draft: string, unit: string) {
  if (p !== "level") {
    if (!triggerOptions[p]?.includes(draft))
      throw new Error("未対応のトリガー設定値です");
    return draft;
  }
  // Reject empty, hexadecimal and nonfinite numeric fields before applying.
  if (
    !/^[+-]?(?:\d+(?:\.\d*)?|\.\d+)(?:e[+-]?\d+)?$/i.test(draft.trim()) ||
    !["V", "mV", "uV"].includes(unit)
  )
    throw new Error("有限の数値と正しい単位を入力してください");
  const value = `${draft.trim()}${unit}`;
  const volts = triggerVolts(value);
  if (volts === null || Math.abs(volts) > 100)
    throw new Error(
      "トリガーレベルは±100 V以内です。本体の範囲・分解能にも制限されます。",
    );
  return value;
}
