import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useLocale } from "./locale";
import {
  canonicalGeneratorValue,
  generatorDraft,
  generatorQuantity,
  generatorValidation,
  supportsGenerator,
  unitsFor,
  waveforms,
  type GeneratorParameter,
  type GeneratorReply,
  type GeneratorSnapshot,
} from "./generator";

export default function GeneratorControls({
  connected,
  busy,
  identity,
  run,
  onNotice,
}: {
  connected: boolean;
  busy: boolean;
  identity: string;
  run: (action: () => Promise<void>) => Promise<void>;
  onNotice: (message: string) => void;
}) {
  const { t } = useLocale();
  const supported = supportsGenerator(identity);
  const [snapshot, setSnapshot] = useState<GeneratorSnapshot | null>(null),
    [valid, setValid] = useState(false);
  const [parameter, setParameter] = useState<GeneratorParameter>("frequency"),
    [draft, setDraft] = useState({ value: "", unit: "kHz" });
  const [last, setLast] = useState(""),
    [failure, setFailure] = useState("");
  const [confirmOn, setConfirmOn] = useState(false),
    [acknowledged, setAcknowledged] = useState(false);
  const generation = useRef(0),
    attempted = useRef(false);
  useEffect(() => {
    generation.current++;
    attempted.current = false;
    setSnapshot(null);
    setValid(false);
    setLast("");
    setFailure("");
    setConfirmOn(false);
    setAcknowledged(false);
    return () => {
      generation.current++;
    };
  }, [connected, identity]);
  const read = () =>
    run(async () => {
      const epoch = generation.current;
      setConfirmOn(false);
      setAcknowledged(false);
      setFailure("");
      try {
        const next = await invoke<GeneratorSnapshot>("read_generator");
        if (epoch !== generation.current) return;
        setSnapshot(next);
        setValid(true);
        setDraft(generatorDraft(next, parameter));
        onNotice(
          t("GEN OUTの現在値を読みました。出力状態は変更していません。"),
        );
      } catch (e) {
        if (epoch === generation.current) {
          setValid(false);
          setFailure(String(e));
        }
        throw e;
      }
    });
  // Wait until the connection operation has released the app's busy flag.
  // Only one initial read; failed reads are never automatically retried.
  useEffect(() => {
    if (connected && supported && !busy && !attempted.current) {
      attempted.current = true;
      void read();
    }
  }, [connected, supported, identity, busy]);
  const apply = (p: GeneratorParameter, value: string, confirmOutput = false) =>
    run(async () => {
      if (!snapshot || !connected) return;
      const epoch = generation.current;
      setConfirmOn(false);
      setAcknowledged(false);
      setFailure("");
      try {
        const reply = await invoke<GeneratorReply>("set_generator_setting", {
          setting: {
            parameter: p,
            value,
            expected: snapshot,
            confirm_output: confirmOutput,
          },
        });
        if (epoch !== generation.current) return;
        setSnapshot(reply.after);
        setValid(reply.verified);
        setDraft(generatorDraft(reply.after, parameter));
        const message = `${reply.command}: ${reply.before[p]} → ${reply.after[p]} ${reply.verified ? t("（読戻し一致）") : t("（読戻し不一致。再読取りして確認）")}`;
        setLast(message);
        onNotice(message);
        if (!reply.verified)
          setFailure(
            t(
              "指定値または他の設定が一致しません。自動で戻さず、再読取りしてください。",
            ),
          );
      } catch (e) {
        if (epoch === generation.current) {
          setValid(false);
          setFailure(String(e));
        }
        throw e;
      }
    });
  let converted = "",
    validation = "";
  if (snapshot) {
    try {
      converted = canonicalGeneratorValue(parameter, draft.value, draft.unit);
      validation = generatorValidation(snapshot, parameter, converted) ?? "";
    } catch (e) {
      validation = e instanceof Error ? e.message : String(e);
    }
  }
  const ready = connected && supported && !busy && !!snapshot;
  const canEnable =
    ready &&
    valid &&
    snapshot?.output.toUpperCase() !== "ON" &&
    !generatorValidation(snapshot!, "output", "ON");
  const changeParameter = (next: GeneratorParameter) => {
    setParameter(next);
    setConfirmOn(false);
    setAcknowledged(false);
    setDraft(
      snapshot
        ? generatorDraft(snapshot, next)
        : { value: "", unit: unitsFor(next)[0] },
    );
  };
  return (
    <section className="generator-controls" aria-label={t("GEN OUT操作")}>
      <h2>
        GEN OUT <span>WAVEFORM GENERATOR</span>
      </h2>
      <div className="generator-status">
        <span
          className={`generator-badge ${snapshot && valid ? (snapshot.output.toUpperCase() === "ON" ? "output-on" : "output-off") : "output-unknown"}`}
        >
          {t("出力")}{" "}
          {snapshot && valid ? snapshot.output.toUpperCase() : t("未確認")}
        </span>
        <button
          disabled={!connected || !supported || busy}
          onClick={() => void read()}
        >
          {t("現在値を読取り")}
        </button>
      </div>
      {snapshot && (
        <>
          <dl className="generator-summary">
            <div>
              <dt>{t("波形種")}</dt>
              <dd>{snapshot.waveform}</dd>
            </div>
            <div>
              <dt>{t("周波数")}</dt>
              <dd>{generatorQuantity(snapshot.frequency, "frequency")}</dd>
            </div>
            <div>
              <dt>{t("周期")}</dt>
              <dd>{generatorQuantity(snapshot.period, "period")}</dd>
            </div>
            <div>
              <dt>{t("振幅")}</dt>
              <dd>{generatorQuantity(snapshot.amplitude, "amplitude")}</dd>
            </div>
            <div>
              <dt>{t("オフセット")}</dt>
              <dd>{generatorQuantity(snapshot.offset, "offset")}</dd>
            </div>
            <div>
              <dt>{t("負荷（読取りのみ）")}</dt>
              <dd>{snapshot.load}</dd>
            </div>
          </dl>
          <p className="small">
            {new Date(snapshot.read_at_unix_ms).toLocaleTimeString()}{" "}
            {valid ? t("読取り時点の設定") : t("最終値。現在の設定は未確認")}
          </p>
        </>
      )}
      <label className="row">
        {t("変更する項目")}
        <select
          aria-label={t("GEN OUT設定項目")}
          value={parameter}
          disabled={!ready}
          onChange={(e) =>
            changeParameter(e.target.value as GeneratorParameter)
          }
        >
          {(
            [
              "frequency",
              "waveform",
              "amplitude",
              "offset",
            ] as GeneratorParameter[]
          ).map((p, i) => (
            <option key={p} value={p}>
              {[t("周波数"), t("波形種"), t("振幅"), t("オフセット")][i]}
            </option>
          ))}
        </select>
      </label>
      <div className="generator-value">
        {parameter === "waveform" ? (
          <select
            aria-label={t("GEN OUT波形種")}
            value={draft.value}
            disabled={!ready}
            onChange={(e) => setDraft({ value: e.target.value, unit: "" })}
          >
            {!waveforms.some((w) => w === draft.value) && (
              <option value={draft.value} disabled>
                {draft.value || "—"}
              </option>
            )}
            {waveforms.map((w) => (
              <option key={w}>{w}</option>
            ))}
          </select>
        ) : (
          <>
            <input
              aria-label={t("GEN OUT変更値")}
              type="number"
              step="any"
              value={draft.value}
              disabled={!ready}
              onChange={(e) => setDraft({ ...draft, value: e.target.value })}
            />
            <select
              aria-label={t("GEN OUT単位")}
              value={draft.unit}
              disabled={!ready}
              onChange={(e) => setDraft({ ...draft, unit: e.target.value })}
            >
              {unitsFor(parameter).map((u) => (
                <option key={u}>{u}</option>
              ))}
            </select>
          </>
        )}
      </div>
      {validation && valid && (
        <p className="generator-warning" role="status">
          {t(validation)}
        </p>
      )}
      <button
        className="wide"
        disabled={!ready || !valid || !!validation}
        onClick={() => void apply(parameter, converted)}
      >
        {t("GEN OUTへ適用・読戻し")}
      </button>
      {last && (
        <p className="readback" role="status">
          {last}
        </p>
      )}
      {failure && (
        <p className="generator-warning" role="alert">
          {failure}
        </p>
      )}
      <div className="generator-output-buttons">
        <button
          className="output-stop"
          disabled={!ready}
          onClick={() => void apply("output", "OFF")}
        >
          {t("出力OFF")}
        </button>
        <button
          disabled={!canEnable}
          onClick={() => {
            setConfirmOn(true);
            setAcknowledged(false);
          }}
        >
          {t("出力ON…")}
        </button>
      </div>
      {confirmOn && (
        <div
          className="generator-confirm"
          role="group"
          aria-label={t("GEN OUT出力ONの確認")}
        >
          <p>
            {t(
              "GEN OUT端子から信号が出ます。接続先と上の設定を確認してください。",
            )}
          </p>
          <label>
            <input
              type="checkbox"
              checked={acknowledged}
              disabled={busy}
              onChange={(e) => setAcknowledged(e.target.checked)}
            />
            {t("接続先と設定を確認しました")}
          </label>
          <div>
            <button
              className="output-enable"
              disabled={!acknowledged || !canEnable}
              onClick={() => void apply("output", "ON", true)}
            >
              {t("確認して出力ON")}
            </button>
            <button
              disabled={busy}
              onClick={() => {
                setConfirmOn(false);
                setAcknowledged(false);
              }}
            >
              {t("キャンセル")}
            </button>
          </div>
        </div>
      )}
      {!connected && (
        <p className="small">
          {t("USB接続後に現在値を読みます。設定は自動適用しません。")}
        </p>
      )}
      {connected && !supported && (
        <p className="generator-warning">
          {t("GEN OUT操作は現在HDS25Sのみ対応しています。")}
        </p>
      )}
      <p className="small">
        {t(
          "本実機では波形指定で出力がONになりました。設定変更は出力ON中のみ許可します。",
        )}
      </p>
      <p className="small generator-limits">
        {t(
          "振幅0.02〜5 Vpp。アプリ安全制限: |オフセット| + 振幅/2 ≤ 2.5 V。負荷INFのみ。",
        )}
      </p>
      <p className="small">
        {t(
          "出力ON/OFFは独立操作です。任意波形・パルス幅・デューティ・負荷切替は未対応。",
        )}
      </p>
      <p className="small">
        {t(
          "周波数変更・出力ON/OFFの読戻しは実機確認済み。全設定範囲・端子実電圧は検証未完了です。",
        )}
      </p>
    </section>
  );
}
