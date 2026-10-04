import { describe, expect, it } from "vitest";
import {
  supportsTrigger,
  triggerDraft,
  triggerValue,
  triggerVolts,
  type TriggerSnapshot,
} from "./trigger";
const snapshot: TriggerSnapshot = {
  source: "CH2",
  coupling: "DC",
  edge: "RISe",
  sweep: "AUTo",
  level: "0.00pV",
  status: "TRIG",
  read_at_unix_ms: 0,
};
describe("generic trigger controls", () => {
  it("only enables the observed model and firmware", () => {
    expect(supportsTrigger("OWON,HDS25S,123,V12.1.0")).toBe(true);
    for (const id of [
      "OTHER,HDS25S,123,V12.1.0",
      "OWON,HDS25S,123,V99",
      "OWON,HDS25,123,V12.1.0",
      "",
    ])
      expect(supportsTrigger(id)).toBe(false);
  });
  it("handles tiny firmware units and scientific notation", () => {
    expect(triggerVolts("0.00pV")).toBe(0);
    expect(triggerVolts("1e3mV")).toBe(1);
    expect(triggerVolts("1000nV")).toBeCloseTo(1e-6);
    expect(triggerDraft(snapshot, "level")).toBe("0");
    expect(triggerDraft(snapshot, "edge")).toBe("RISE");
    expect(triggerDraft(snapshot, "source")).toBe("CH2");
  });
  it("compiles finite, bounded values and discrete options", () => {
    expect(triggerValue("level", "-25", "mV")).toBe("-25mV");
    expect(triggerValue("source", "CH2", "")).toBe("CH2");
    for (const value of ["", "NaN", "Infinity", "0x10", "1;:RUN", "101"])
      expect(() => triggerValue("level", value, "V")).toThrow();
    expect(() => triggerValue("edge", "BOTH", "")).toThrow();
    expect(() => triggerValue("level", "1", "kV")).toThrow();
  });
});
