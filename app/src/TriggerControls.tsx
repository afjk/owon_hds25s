import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useLocale } from "./locale";
import { field, text, type Frame } from "./model";
import {
  supportsTrigger,
  triggerDraft,
  triggerOptions,
  triggerValue,
  type TriggerParameter,
  type TriggerReply,
  type TriggerSnapshot,
} from "./trigger";

export default function TriggerControls({
  connected,
  busy,
  frame,
  settingsRevision,
  run,
  onNotice,
}: {
  connected: boolean;
  busy: boolean;
  frame: Frame | null;
  settingsRevision: number;
  run: (action: () => Promise<void>) => Promise<void>;
  onNotice: (message: string) => void;
}) {
  const { t } = useLocale();
  const identity = frame?.record.identity ?? "";
  const supported = supportsTrigger(identity);
  const [snapshot, setSnapshot] = useState<TriggerSnapshot | null>(null);
  const [valid, setValid] = useState(false);
  const [parameter, setParameter] = useState<TriggerParameter>("source");
  const [draft, setDraft] = useState("");
  const [unit, setUnit] = useState("V");
  const [confirmed, setConfirmed] = useState(false);
  const [rounding, setRounding] = useState(false);
  const [last, setLast] = useState("");
  const [failure, setFailure] = useState("");
  const epoch = useRef(0);
  useEffect(() => {
    epoch.current++;
    setSnapshot(null);
    setValid(false);
    setDraft("");
    setLast("");
    setFailure("");
    setConfirmed(false);
    setRounding(false);
    return () => {
      epoch.current++;
    };
  }, [connected, identity, settingsRevision]);
  const read = () =>
    run(async () => {
      const generation = epoch.current;
      setFailure("");
      setConfirmed(false);
      try {
        const next = await invoke<TriggerSnapshot>("read_trigger");
        if (generation !== epoch.current) return;
        setSnapshot(next);
        setValid(true);
        setDraft(triggerDraft(next, parameter));
        setUnit("V");
        onNotice(t("トリガーの現在値を読みました。設定は変更していません。"));
      } catch (e) {
        if (generation === epoch.current) {
          setValid(false);
          setFailure(String(e));
        }
        throw e;
      }
    });
  let value = "",
    validation = "";
  try {
    value = triggerValue(parameter, draft, unit);
  } catch (e) {
    validation = e instanceof Error ? e.message : String(e);
  }
  const single = parameter === "sweep" && value === "SINGLE";
  const ready = connected && supported && !busy && !!snapshot;
  const apply = () =>
    run(async () => {
      if (!snapshot || !valid || validation || (single && !confirmed)) return;
      const generation = epoch.current;
      setFailure("");
      try {
        const reply = await invoke<TriggerReply>("set_trigger_setting", {
          setting: {
            parameter,
            value,
            expected: snapshot,
            confirm_single: single && confirmed,
            rounding_workaround: parameter === "level" && rounding,
          },
        });
        if (generation !== epoch.current) return;
        setSnapshot(reply.after);
        setValid(reply.verified);
        setDraft(triggerDraft(reply.after, parameter));
        setUnit("V");
        setConfirmed(false);
        const linked = reply.linked_level_changed
          ? ` / ${t("連動するトリガーレベル")}: ${reply.before.level} → ${reply.after.level}`
          : "";
        const requested = reply.rounding_workaround
          ? ` / ${t("指定値")}: ${reply.requested_value}`
          : "";
        const message = `${reply.command}: ${reply.before[parameter]} → ${reply.after[parameter]} ${reply.verified ? t("（読戻し一致）") : t("（読戻し不一致。再読取りして確認）")}${linked}${requested}`;
        setLast(message);
        onNotice(message);
        if (!reply.verified)
          setFailure(
            t(
              "指定値または他の設定が一致しません。自動で戻さず、再読取りしてください。",
            ),
          );
      } catch (e) {
        if (generation === epoch.current) {
          setValid(false);
          setFailure(String(e));
          setConfirmed(false);
        }
        throw e;
      }
    });
  const items = field(field(frame?.record.header, "Trig"), "Items");
  const labels: Record<TriggerParameter, string> = {
    source: "トリガ源",
    coupling: "トリガ結合",
    edge: "エッジ",
    sweep: "トリガモード",
    level: "トリガレベル",
  };
  return (
    <section className="trigger-controls" aria-label={t("トリガー操作")}>
      <h2>
        {t("トリガー")} <span>EDGE TRIGGER</span>
      </h2>
      <div className="generator-status">
        <span className="generator-badge">
          {t("本体状態")}: {text(frame?.record.status_after)}
        </span>
        <button
          disabled={!connected || !supported || busy}
          onClick={() => void read()}
        >
          {t("トリガー現在値を読取り")}
        </button>
      </div>
      {parameter === "level" && (
        <label className="check">
          <input
            type="checkbox"
            checked={rounding}
            disabled={!ready}
            onChange={(e) => setRounding(e.target.checked)}
          />
          {t(
            "丸め対策（実験的）：送信値を±100 µV補正。指定値との一致を読戻しで確認",
          )}
        </label>
      )}
      {snapshot ? (
        <>
          <dl className="generator-summary">
            {(Object.keys(labels) as TriggerParameter[]).map((p) => (
              <div key={p}>
                <dt>{t(labels[p])}</dt>
                <dd>
                  {snapshot[p]}
                  {p === "level" && !/[vV]$/.test(snapshot[p]) ? " V" : ""}
                </dd>
              </div>
            ))}
          </dl>
          <p className="small">
            {new Date(snapshot.read_at_unix_ms).toLocaleTimeString()}{" "}
            {valid ? t("読取り時点の設定") : t("最終値。現在の設定は未確認")}
          </p>
        </>
      ) : (
        <p className="small">
          {t("ヘッダー参考:")} {text(field(items, "Channel"))} /{" "}
          {text(field(items, "Edge"))} / {text(field(items, "Sweep"))} /{" "}
          {text(field(items, "Level"))}
        </p>
      )}
      <label className="row">
        {t("変更する項目")}
        <select
          aria-label={t("トリガー設定項目")}
          value={parameter}
          disabled={!ready}
          onChange={(e) => {
            const p = e.target.value as TriggerParameter;
            setParameter(p);
            setDraft(snapshot ? triggerDraft(snapshot, p) : "");
            setUnit("V");
            setConfirmed(false);
          }}
        >
          {(Object.keys(labels) as TriggerParameter[]).map((p) => (
            <option key={p} value={p}>
              {t(labels[p])}
            </option>
          ))}
        </select>
      </label>
      <div className="generator-value">
        {triggerOptions[parameter] ? (
          <select
            aria-label={t("トリガー変更値")}
            disabled={!ready}
            value={draft}
            onChange={(e) => {
              setDraft(e.target.value);
              setConfirmed(false);
            }}
          >
            {!triggerOptions[parameter]?.includes(draft) && (
              <option value={draft} disabled>
                {draft || "—"}
              </option>
            )}
            {triggerOptions[parameter]!.map((v) => (
              <option key={v}>{v}</option>
            ))}
          </select>
        ) : (
          <>
            <input
              aria-label={t("トリガーレベル変更値")}
              type="number"
              step="any"
              value={draft}
              disabled={!ready}
              onChange={(e) => setDraft(e.target.value)}
            />
            <select
              aria-label={t("トリガーレベル単位")}
              disabled={!ready}
              value={unit}
              onChange={(e) => setUnit(e.target.value)}
            >
              {["V", "mV", "uV"].map((v) => (
                <option key={v}>{v}</option>
              ))}
            </select>
          </>
        )}
      </div>
      {single && (
        <label className="check trigger-confirm">
          <input
            type="checkbox"
            checked={confirmed}
            disabled={!ready}
            onChange={(e) => setConfirmed(e.target.checked)}
          />
          {t("Singleで取得状態が変わる可能性を確認しました")}
        </label>
      )}
      <button
        className="wide"
        disabled={!ready || !valid || !!validation || (single && !confirmed)}
        onClick={() => void apply()}
      >
        {t("トリガーへ適用・読戻し")}
      </button>
      {validation && snapshot && <p className="small">{t(validation)}</p>}
      {last && <p className="readback">{last}</p>}
      {failure && (
        <p className="error" role="alert">
          {failure}
        </p>
      )}
      <p className="small">
        {t(
          "Auto: 条件なしでも更新。Normal: 条件成立時に更新。Single: 1回の取得で停止。",
        )}
      </p>
      <p className="small">
        {t(
          "Singleの再待受は本体のRUN/STOPで行ってください。Macの「1回取得」は待受操作ではありません。",
        )}
      </p>
      <p className="small">
        {t(
          "設定は1項目ずつ適用。本体の手動変更を検出すると書込みせず再読取りを求めます。",
        )}
      </p>
      <p className="small">
        {t(
          "トリガ源・結合の切替で本体のレベル値も変わる場合があります。変更後のレベルも確認してください。",
        )}
      </p>
      <p className="small">
        {t(
          "源・結合・エッジ・モードの変更とSingle停止を実機確認済み。レベルは本体の量子化で指定値と異なる場合があります。",
        )}
      </p>
      <p className="small">
        {supported
          ? t(
              "HDS25S V12.1.0の応答済みコマンドを使用。レベルは本体の範囲・分解能に制限されます。",
            )
          : t("トリガー操作は現在OWON HDS25S V12.1.0のみ対応しています")}
      </p>
    </section>
  );
}
