import type { PlotConfig } from "./ScopeCanvas";
export function restorePlot(
  defaults: PlotConfig,
  encoded: string | null,
): PlotConfig {
  try {
    const saved = JSON.parse(encoded ?? "null");
    if (!saved || typeof saved !== "object") return defaults;
    const plot = { ...defaults };
    for (const key of ["stacked", "grid", "points", "fftDb"] as const)
      if (typeof saved[key] === "boolean") plot[key] = saved[key];
    for (const key of ["show", "inverted"] as const)
      if (
        Array.isArray(saved[key]) &&
        saved[key].length === 2 &&
        saved[key].every((v: unknown) => typeof v === "boolean")
      )
        plot[key] = [...saved[key]] as [boolean, boolean];
    for (const key of ["background", "gridColor"] as const)
      if (typeof saved[key] === "string" && /^#[0-9a-f]{6}$/i.test(saved[key]))
        plot[key] = saved[key];
    if (
      Array.isArray(saved.colors) &&
      saved.colors.length === 2 &&
      saved.colors.every(
        (v: unknown) => typeof v === "string" && /^#[0-9a-f]{6}$/i.test(v),
      )
    )
      plot.colors = [...saved.colors] as [string, string];
    if (
      ["rectangular", "hann", "hamming", "blackman"].includes(saved.fftWindow)
    )
      plot.fftWindow = saved.fftWindow;
    return plot;
  } catch {
    return defaults;
  }
}
export function storePlot(plot: PlotConfig): string {
  const {
    stacked,
    grid,
    points,
    fftDb,
    show,
    inverted,
    background,
    gridColor,
    colors,
    fftWindow,
  } = plot;
  return JSON.stringify({
    stacked,
    grid,
    points,
    fftDb,
    show,
    inverted,
    background,
    gridColor,
    colors,
    fftWindow,
  });
}
