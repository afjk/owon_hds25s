import { it, expect } from "vitest";
import { restorePlot, storePlot } from "./preferences";
import type { PlotConfig } from "./ScopeCanvas";
const defaults = {
  background: "#0b1420",
  colors: ["#f1c65c", "#60d7ca"],
  grid: true,
  show: [true, true],
  fftWindow: "hann",
  reset: 0,
  mode: "wave",
} as PlotConfig;
it("restores only validated PC display preferences, never device settings or view state", () => {
  const result = restorePlot(
    defaults,
    JSON.stringify({
      background: "url(http://x)",
      grid: false,
      show: ["yes", false],
      colors: ["#ffffff", "#000000"],
      mode: "fft",
      reset: 999,
      device: "ON",
    }),
  );
  expect(result.background).toBe(defaults.background);
  expect(result.grid).toBe(false);
  expect(result.show).toEqual(defaults.show);
  expect(result.mode).toBe("wave");
  expect(result.colors).toEqual(["#ffffff", "#000000"]);
  expect(restorePlot(defaults, "{broken")).toEqual(defaults);
  expect(JSON.parse(storePlot(result)).reset).toBeUndefined();
});
