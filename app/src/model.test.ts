import { describe, it, expect } from "vitest";
import { field, statistics, mathValues, spectrum } from "./model";
describe("raw waveform analysis", () => {
  it("reads vendor key casing", () =>
    expect(field({ TiMeBaSe: 3 }, "TIMEBASE")).toBe(3));
  it("computes raw statistics", () =>
    expect(statistics([-2, 0, 2])).toMatchObject({
      min: -2,
      max: 2,
      mean: 0,
      span: 4,
    }));
  it("handles missing data", () => expect(statistics([]).span).toBeNaN());
  it("math uses matched lengths", () =>
    expect(mathValues([1, 2, 3], [3, 4], "subtract")).toEqual([-2, -2]));
  it("does not create infinity on division by zero", () =>
    expect(mathValues([1], [0], "divide")[0]).toBeNaN());
  it("squares a single channel", () =>
    expect(mathValues([], [-3, 4], "square2")).toEqual([9, 16]));
  it("FFT locates bin and normalizes RMS", () => {
    const s = spectrum(
      Array.from(
        { length: 512 },
        (_, i) => 8 * Math.sin((2 * Math.PI * 16 * i) / 512),
      ),
      "rectangular",
      false,
    );
    expect(s.indexOf(Math.max(...s))).toBe(16);
    expect(s[16]).toBeCloseTo(8 / Math.SQRT2, 8);
  });
  it("FFT removes DC", () =>
    expect(Math.max(...spectrum(Array(64).fill(5), "hann", false))).toBe(0));
  it("normalizes the Nyquist bin separately", () => {
    const s = spectrum(
      Array.from({ length: 64 }, (_, i) => (i % 2 ? -7 : 7)),
      "rectangular",
      false,
    );
    expect(s[32]).toBeCloseTo(7, 8);
  });
});
