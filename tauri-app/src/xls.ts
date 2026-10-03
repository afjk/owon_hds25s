import { utils, write } from "xlsx";
import type { Frame, ChannelName } from "./model";
export function excelBytes(
  frame: Frame,
  channel: "both" | ChannelName,
): number[] {
  const names =
    channel === "both" ? (["CH1", "CH2"] as ChannelName[]) : [channel];
  const count = Math.max(0, ...names.map((n) => frame.values[n]?.length ?? 0));
  if (count > 65535)
    throw new Error(
      "XLSの上限は見出しを含め65536行です。TXT/CSVをご使用ください。",
    );
  const rows: (string | number | null)[][] = [
    ["byte_index", ...names.map((n) => `${n}_raw`)],
  ];
  for (let i = 0; i < count; i++)
    rows.push([i, ...names.map((n) => frame.values[n]?.[i] ?? null)]);
  const wb = utils.book_new(),
    sheet = utils.aoa_to_sheet(rows);
  sheet["!cols"] = [{ wch: 16 }, ...names.map(() => ({ wch: 16 }))];
  utils.book_append_sheet(wb, sheet, "Raw screen bytes");
  const meta = utils.aoa_to_sheet([
    ["identity", frame.record.identity],
    [
      "units",
      "Uncalibrated signed screen bytes; index is not ADC sample index",
    ],
    ["consistency", frame.record.consistency],
    ["source", "HDS200 screen waveform; original bytes retained in JSON"],
    ["retrieved_at_unix_ms", frame.record.retrieved_at_unix_ms ?? null],
  ]);
  meta["!cols"] = [{ wch: 24 }, { wch: 90 }];
  utils.book_append_sheet(wb, meta, "Acquisition");
  return Array.from(
    new Uint8Array(write(wb, { bookType: "biff8", type: "array" })),
  );
}
