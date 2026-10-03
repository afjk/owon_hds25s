import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { channelInfo, field, text, type Frame } from "./model";
import { useLocale } from "./locale";

const timebases = [
  "5.0ns",
  "10.0ns",
  "20.0ns",
  "50.0ns",
  "100ns",
  "200ns",
  "500ns",
  "1.0us",
  "2.0us",
  "5.0us",
  "10us",
  "20us",
  "50us",
  "100us",
  "200us",
  "500us",
  "1.0ms",
  "2.0ms",
  "5.0ms",
  "10ms",
  "20ms",
  "50ms",
  "100ms",
  "200ms",
  "500ms",
  "1.0s",
  "2.0s",
  "5.0s",
  "10s",
  "20s",
  "50s",
  "100s",
  "200s",
  "500s",
  "1000s",
];
const scales = [
  "10.0mV",
  "20.0mV",
  "50.0mV",
  "100mV",
  "200mV",
  "500mV",
  "1.00V",
  "2.00V",
  "5.00V",
  "10.0V",
  "20.0V",
  "50.0V",
  "100V",
  "200V",
  "500V",
  "4.00V",
];
const fields: Record<string, [string, string, string[] | null][]> = {
  CH1: [
    ["display", "本体CH表示", ["ON", "OFF"]],
    ["coupling", "結合", ["DC", "AC", "GND"]],
    ["probe", "プローブ倍率", ["1X", "10X", "20X", "100X", "1000X"]],
    ["scale", "感度（プローブ倍率込み）", scales],
    ["offset", "オフセット（SCPI設定値）", null],
  ],
  horizontal: [
    ["scale", "時間軸 / div", timebases],
    ["offset", "水平位置（−10〜10）", null],
  ],
  acquire: [
    ["mode", "取得方式", ["SAMPLE", "PEAK"]],
    ["memory", "メモリ長", ["4K", "8K"]],
  ],
};
fields.CH2 = fields.CH1;
export interface DeviceMeasurements {
  values: Record<string, Record<string, string>>;
  read_at_unix_ms: number;
}
export default function DeviceControls({
  connected,
  busy,
  frame,
  run,
  onNotice,
  onMeasurements,
}: {
  connected: boolean;
  busy: boolean;
  frame: Frame | null;
  run: (action: () => Promise<void>) => Promise<void>;
  onNotice: (message: string) => void;
  onMeasurements: (values: DeviceMeasurements) => void;
}) {
  const { t } = useLocale();
  const [target, setTarget] = useState("CH1"),
    [parameter, setParameter] = useState("coupling"),
    [value, setValue] = useState("DC"),
    [last, setLast] = useState("");
  const selected = fields[target].find(([p]) => p === parameter)!;
  const change = (target: string, parameter: string) => {
    setTarget(target);
    setParameter(parameter);
    const options = fields[target].find(([p]) => p === parameter)![2];
    setValue(options?.[0] ?? (parameter === "level" ? "0V" : "0"));
  };
  const info = target.startsWith("CH")
    ? channelInfo(frame, target as "CH1" | "CH2")
    : field(
        frame?.record.header,
        target === "horizontal"
          ? "TIMEBASE"
          : target === "acquire"
            ? "SAMPLE"
            : "TRIG",
      );
  const headerKey =
    parameter === "memory"
      ? "DEPMEM"
      : parameter === "mode"
        ? "TYPE"
        : parameter === "offset" && target === "horizontal"
          ? "HOFFSET"
          : parameter;
  return (
    <section className="device-controls">
      <h2>
        {t("本体操作")}
        <span>REMOTE CONTROL</span>
      </h2>
      <p className="small">
        {t(
          "本体の設定を変更します。表示設定とは別です。変更前後をSCPIで読み戻します。",
        )}
      </p>
      <label className="row">
        {t("対象")}
        <select
          aria-label={t("本体操作の対象")}
          value={target}
          disabled={busy}
          onChange={(e) => change(e.target.value, fields[e.target.value][0][0])}
        >
          {["CH1", "CH2", "horizontal", "acquire"].map((name, i) => (
            <option key={name} value={name}>
              {["CH1", "CH2", t("時間軸"), t("取得")][i]}
            </option>
          ))}
        </select>
      </label>
      <label className="row">
        {t("項目")}
        <select
          aria-label={t("本体設定項目")}
          value={parameter}
          disabled={busy}
          onChange={(e) => change(target, e.target.value)}
        >
          {fields[target].map(([p, label]) => (
            <option key={p} value={p}>
              {t(label)}
            </option>
          ))}
        </select>
      </label>
      <label className="row">
        {t("変更値")}
        {selected[2] ? (
          <select
            aria-label={t("本体設定の変更値")}
            value={value}
            disabled={busy}
            onChange={(e) => setValue(e.target.value)}
          >
            {selected[2].map((v) => (
              <option key={v}>{v}</option>
            ))}
          </select>
        ) : (
          <input
            aria-label={t("本体設定の変更値")}
            value={value}
            disabled={busy}
            onChange={(e) => setValue(e.target.value)}
          />
        )}
      </label>
      <p className="small">
        {t("ヘッダー参考:")}
        {text(field(info, headerKey))}
        {t("（感度・位置はSCPI読戻しと表現が異なる場合があります）")}
      </p>
      <button
        className="wide"
        disabled={!connected || busy}
        onClick={() =>
          void run(async () => {
            const reply = await invoke<{
              command: string;
              before: string;
              after: string;
              verified: boolean;
            }>("set_device_setting", { setting: { target, parameter, value } });
            const message = `${reply.command}: ${reply.before} → ${reply.after}${reply.verified ? t("（読戻し一致）") : t("（指定値と不一致。本体対応・範囲を確認）")}`;
            setLast(message);
            onNotice(message);
          })
        }
      >
        {t("本体へ適用・読戻し")}
      </button>
      {last && <p className="readback">{last}</p>}
      <button
        className="wide"
        disabled={!connected || busy}
        onClick={() =>
          void run(async () => {
            onMeasurements(
              await invoke<DeviceMeasurements>("read_measurements"),
            );
            onNotice(t("本体測定値を読みました。波形とは別の逐次照会です。"));
          })
        }
      >
        {t("本体の測定値を取得")}
      </button>
      <p className="small">
        {t(
          "未確認コマンドは機種・FWによって非対応の場合があります。RUN/STOP・Auto設定・出力端子は操作しません。",
        )}
      </p>
    </section>
  );
}
