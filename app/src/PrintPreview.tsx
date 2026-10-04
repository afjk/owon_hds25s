import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useLocale } from "./locale";
export interface PrintData {
  image: string;
  identity: string;
  mode: string;
  note: string;
  capturedAt: string;
}
export default function PrintPreview({
  data,
  close,
  report,
}: {
  data: PrintData;
  close: () => void;
  report: (e: unknown) => void;
}) {
  const { t } = useLocale();
  const [margins, setMargins] = useState([10, 10, 10, 10]),
    [landscape, setLandscape] = useState(true),
    [printing, setPrinting] = useState(false);
  return (
    <div
      className="modal-backdrop print-modal"
      role="dialog"
      aria-modal="true"
      aria-label={t("印刷プレビュー")}
    >
      <div className="print-controls">
        <h2>{t("印刷プレビュー")}</h2>
        <label>
          <input
            type="checkbox"
            checked={landscape}
            onChange={(e) => setLandscape(e.target.checked)}
          />
          {t("横向き")}
        </label>
        {margins.map((v, i) => (
          <label key={i}>
            {[t("上"), t("右"), t("下"), t("左")][i]}
            {t("余白 mm")}
            <input
              type="number"
              min={0}
              max={40}
              value={v}
              onChange={(e) =>
                setMargins((old) =>
                  old.map((x, j) =>
                    i === j
                      ? Math.min(40, Math.max(0, Number(e.target.value)))
                      : x,
                  ),
                )
              }
            />
          </label>
        ))}
        <button
          disabled={printing}
          onClick={async () => {
            setPrinting(true);
            try {
              await invoke("print_preview", { margins, landscape });
            } catch (e) {
              report(e);
            } finally {
              setPrinting(false);
            }
          }}
        >
          {t("印刷 / PDF保存…")}
        </button>
        <button disabled={printing} onClick={close}>
          {t("閉じる")}
        </button>
      </div>
      <style>{`@media print { @page { size: A4 ${landscape ? "landscape" : "portrait"}; margin: ${margins.map((m) => `${m}mm`).join(" ")}; } }`}</style>
      <div className="print-scroll">
        <article
          className={`print-sheet ${landscape ? "landscape" : "portrait"}`}
          style={{ padding: margins.map((v) => `${v}mm`).join(" ") }}
        >
          <div className="print-content">
            <h1>OWON Scope</h1>
            <p>{data.identity}</p>
            <p>
              {data.capturedAt} · {data.mode}
            </p>
            <img src={data.image} alt={t("保存したPC波形表示")} />
            <p>{data.note}</p>
          </div>
        </article>
      </div>
    </div>
  );
}
