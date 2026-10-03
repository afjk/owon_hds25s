import { describe, it, expect } from "vitest";
import { read, utils } from "xlsx";
import { excelBytes } from "./xls";
import type { Frame } from "./model";
const frame = {
  values: { CH1: [89, -117, 0], CH2: [12, 19] },
  record: { identity: "OWON,HDS25S", consistency: "live" },
} as unknown as Frame;
describe("real BIFF8 XLS export", () => {
  it("roundtrips numeric signed bytes, missing rows and channel selection", () => {
    const bytes = excelBytes(frame, "both");
    expect(bytes.slice(0, 8)).toEqual([208, 207, 17, 224, 161, 177, 26, 225]);
    const wb = read(new Uint8Array(bytes), { type: "array" });
    expect(
      utils.sheet_to_json(wb.Sheets[wb.SheetNames[0]], {
        header: 1,
        defval: null,
      }),
    ).toEqual([
      ["byte_index", "CH1_raw", "CH2_raw"],
      [0, 89, 12],
      [1, -117, 19],
      [2, 0, null],
    ]);
    const one = read(new Uint8Array(excelBytes(frame, "CH2")), {
      type: "array",
    });
    expect(one.Sheets[one.SheetNames[0]]["!ref"]).toBe("A1:B3");
  });
});
