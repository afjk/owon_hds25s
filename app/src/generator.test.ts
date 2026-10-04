import { describe, it, expect } from "vitest";
import {
  canonicalGeneratorValue,
  generatorDraft,
  generatorQuantity,
  generatorValidation,
  supportsGenerator,
  type GeneratorSnapshot,
} from "./generator";
const snapshot: GeneratorSnapshot = {
  waveform: "SINe",
  frequency: "2.000000e+03",
  period: "5.000000e-04",
  amplitude: "1.000000e+00",
  offset: "0.000000e+00",
  output: "ON",
  load: "INF",
  read_at_unix_ms: 1,
};
describe("GEN OUT UI", () => {
  it("converts units without sending SCPI text", () => {
    expect(canonicalGeneratorValue("frequency", "2", "kHz")).toBe("2000");
    expect(canonicalGeneratorValue("amplitude", "20", "mVpp")).toBe("0.02");
    expect(canonicalGeneratorValue("offset", "-250", "mV")).toBe("-0.25");
    expect(() =>
      canonicalGeneratorValue("frequency", "2;:CHANNEL ON", "kHz"),
    ).toThrow();
    expect(() => canonicalGeneratorValue("offset", "", "V")).toThrow();
    expect(() =>
      canonicalGeneratorValue("amplitude", "Infinity", "Vpp"),
    ).toThrow();
    expect(() => canonicalGeneratorValue("frequency", "2", "V")).toThrow();
  });
  it("uses only HDS25S and supported waveforms", () => {
    expect(supportsGenerator("OWON,HDS25S,00000000,V12.1.0")).toBe(true);
    expect(supportsGenerator("OWON,HDS25,123,V1")).toBe(false);
    expect(() => canonicalGeneratorValue("waveform", "SINC", "")).toThrow();
  });
  it("checks waveform-specific limits before applying", () => {
    expect(generatorValidation(snapshot, "frequency", "10000000")).toBeNull();
    expect(
      generatorValidation(
        { ...snapshot, waveform: "SQUare" },
        "frequency",
        "2000001",
      ),
    ).not.toBeNull();
    expect(generatorValidation(snapshot, "frequency", "0.09")).not.toBeNull();
    expect(
      generatorValidation(
        { ...snapshot, frequency: "2000000" },
        "waveform",
        "RAMP",
      ),
    ).not.toBeNull();
  });
  it("checks voltage envelope and leaves OFF available", () => {
    expect(generatorValidation(snapshot, "offset", "2")).toBeNull();
    expect(generatorValidation(snapshot, "offset", "2.01")).not.toBeNull();
    expect(generatorValidation(snapshot, "amplitude", "0.019")).not.toBeNull();
    expect(
      generatorValidation({ ...snapshot, load: "50" }, "frequency", "2000"),
    ).not.toBeNull();
    expect(
      generatorValidation(
        { ...snapshot, load: "50", waveform: "SINC" },
        "output",
        "OFF",
      ),
    ).toBeNull();
  });
  it("never configures a generator while output is OFF", () => {
    for (const [parameter, value] of [
      ["waveform", "SINE"],
      ["frequency", "2000"],
      ["amplitude", "1"],
      ["offset", "0"],
    ] as const) {
      expect(
        generatorValidation({ ...snapshot, output: "OFF" }, parameter, value),
      ).not.toBeNull();
    }
    expect(
      generatorValidation({ ...snapshot, output: "OFF" }, "output", "ON"),
    ).toBeNull();
  });
  it("loads current settings and displays linked period", () => {
    expect(generatorDraft(snapshot, "frequency")).toEqual({
      value: "2",
      unit: "kHz",
    });
    expect(generatorDraft(snapshot, "waveform")).toEqual({
      value: "SINE",
      unit: "",
    });
    expect(generatorQuantity(snapshot.frequency, "frequency")).toBe("2 kHz");
    expect(generatorQuantity(snapshot.period, "period")).toBe("500 µs");
    expect(generatorQuantity(snapshot.amplitude, "amplitude")).toBe("1 Vpp");
  });
});
