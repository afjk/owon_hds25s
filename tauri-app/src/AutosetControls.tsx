import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useLocale } from "./locale";
import { supportsTrigger, type TriggerSnapshot } from "./trigger";
import type { GeneratorSnapshot } from "./generator";
interface Snapshot {
  scope: Record<string, string>;
  trigger: TriggerSnapshot;
  generator: GeneratorSnapshot;
}
interface Reply {
  before: Snapshot;
  after: Snapshot;
  changes: { field: string; before: string; after: string }[];
  readback_stable: boolean;
  generator_preserved: boolean;
  effect_observed: boolean;
}
export default function AutosetControls({
  connected,
  busy,
  identity,
  run,
  onNotice,
  onChanged,
}: {
  connected: boolean;
  busy: boolean;
  identity: string;
  run: (f: () => Promise<void>) => Promise<void>;
  onNotice: (s: string) => void;
  onChanged: () => void;
}) {
  const { t } = useLocale();
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null);
  const [available, setAvailable] = useState(false);
  const [confirm, setConfirm] = useState(false);
  const [reply, setReply] = useState<Reply | null>(null);
  const [failure, setFailure] = useState("");
  const epoch = useRef(0);
  useEffect(() => {
    const generation = ++epoch.current;
    setSnapshot(null);
    setReply(null);
    setConfirm(false);
    setFailure("");
    setAvailable(false);
    if (connected)
      void invoke<boolean>("autoset_available")
        .then((v) => {
          if (epoch.current === generation) setAvailable(v);
        })
        .catch(() => {});
    return () => {
      epoch.current++;
    };
  }, [connected, identity]);
  const read = () =>
    run(async () => {
      const generation = epoch.current;
      setFailure("");
      setConfirm(false);
      try {
        const next = await invoke<Snapshot>("read_autoset_settings");
        if (epoch.current !== generation) return;
        setSnapshot(next);
        onNotice(t("Auto用の現在設定を読みました。設定は変更していません。"));
      } catch (e) {
        if (epoch.current === generation) {
          setSnapshot(null);
          setFailure(String(e));
        }
        throw e;
      }
    });
  const apply = () =>
    run(async () => {
      if (!available || !snapshot || !confirm) return;
      const generation = epoch.current;
      const expected = snapshot;
      setConfirm(false);
      setFailure("");
      setSnapshot(null);
      try {
        const result = await invoke<Reply>("autoset_device", {
          request: { expected, confirm: true },
        });
        if (epoch.current !== generation) return;
        setReply(result);
        onChanged();
        const ok = result.readback_stable && result.generator_preserved;
        const message = ok
          ? t("Auto送信後の設定を読取りました。変更一覧を確認してください。")
          : t("Auto後の設定が未確認です。本体を確認し、再読取りしてください。");
        if (!ok) setFailure(message);
        onNotice(message);
      } catch (e) {
        if (epoch.current === generation) {
          onChanged();
          setFailure(String(e));
        }
        throw e;
      }
    });
  const ready = connected && !busy && supportsTrigger(identity);
  return (
    <section aria-label={t("Auto設定操作")}>
      <h2>
        {t("Auto設定")} <span>AUTO SETUP</span>
      </h2>
      <p className="small">
        {t(
          "入力に合わせて感度・時間軸・トリガーなどをまとめて変更します。トリガーのAutoモードとは別です。",
        )}
      </p>
      <button disabled={!ready} onClick={() => void read()}>
        {t("Auto用の現在設定を読取り")}
      </button>
      {!available && (
        <p className="small">
          {t("Auto設定は実機検証待ちです。現在は設定読取りのみ使用できます")}
        </p>
      )}
      {snapshot && (
        <details>
          <summary>{t("読取り時点の本体設定")}</summary>
          <dl className="generator-summary">
            {Object.entries(snapshot.scope).map(([k, v]) => (
              <div key={k}>
                <dt>{k}</dt>
                <dd>{v}</dd>
              </div>
            ))}
            {(["source", "coupling", "edge", "sweep", "level"] as const).map(
              (k) => (
                <div key={k}>
                  <dt>trigger.{k}</dt>
                  <dd>{snapshot.trigger[k]}</dd>
                </div>
              ),
            )}
            <div>
              <dt>GEN OUT</dt>
              <dd>{snapshot.generator.output}</dd>
            </div>
          </dl>
        </details>
      )}
      {available && (
        <label className="check">
          <input
            type="checkbox"
            checked={confirm}
            disabled={!ready || !snapshot}
            onChange={(e) => setConfirm(e.target.checked)}
          />
          {t("本体設定が変わり、自動では元に戻らないことを確認しました")}
        </label>
      )}
      <button
        disabled={!ready || !available || !snapshot || !confirm}
        onClick={() => void apply()}
      >
        {t("確認してAuto設定")}
      </button>
      {reply && (
        <>
          <p className="small">{t("Auto前 → Auto後")}</p>
          <dl className="generator-summary">
            {reply.changes.map((c) => (
              <div key={c.field}>
                <dt>{c.field}</dt>
                <dd>
                  {c.before} → {c.after}
                </dd>
              </div>
            ))}
          </dl>
          {!reply.effect_observed && (
            <p className="small">
              {t(
                "設定の変化はありません。専用の実行応答がないため、送信だけで動作成功とは判定しません。",
              )}
            </p>
          )}
        </>
      )}
      {failure && (
        <p className="control-error" role="alert">
          {failure}
        </p>
      )}
    </section>
  );
}
