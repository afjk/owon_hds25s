import {
  createContext,
  useContext,
  useState,
  useEffect,
  type ReactNode,
} from "react";
const english: Record<string, string> = {
  指定値: "Requested value",
  Auto設定操作: "Auto setup controls",
  Auto設定: "Auto setup",
  Auto用の現在設定を読取り: "Read settings for Auto setup",
  "Auto用の現在設定を読みました。設定は変更していません。":
    "Settings for Auto setup read; no changes made.",
  "Auto設定は実機検証待ちです。現在は設定読取りのみ使用できます":
    "Auto setup is awaiting hardware validation. Settings readout is available.",
  "入力に合わせて感度・時間軸・トリガーなどをまとめて変更します。トリガーのAutoモードとは別です。":
    "Adjusts sensitivity, timebase and trigger to the input. Separate from Auto trigger mode.",
  読取り時点の本体設定: "Instrument settings at read time",
  "本体設定が変わり、自動では元に戻らないことを確認しました":
    "I understand instrument settings will change without automatic rollback",
  確認してAuto設定: "Confirm Auto setup",
  "Auto前 → Auto後": "Before Auto → After Auto",
  "Auto送信後の設定を読取りました。変更一覧を確認してください。":
    "Read settings after sending Auto. Check the changes.",
  "Auto後の設定が未確認です。本体を確認し、再読取りしてください。":
    "Settings after Auto are unverified. Check the instrument and read again.",
  "設定の変化はありません。専用の実行応答がないため、送信だけで動作成功とは判定しません。":
    "No setting changes observed. With no dedicated acknowledgement, sending alone does not confirm execution.",
  "丸め対策（実験的）：送信値を±100 µV補正。指定値との一致を読戻しで確認":
    "Rounding workaround (experimental): adjust the transmitted value by ±100 µV, then verify against the requested value",
  トリガー: "Trigger",
  連動するトリガーレベル: "Linked trigger level",
  "トリガ源・結合の切替で本体のレベル値も変わる場合があります。変更後のレベルも確認してください。":
    "Changing source or trigger coupling may select another instrument level. Check the resulting level too.",
  "源・結合・エッジ・モードの変更とSingle停止を実機確認済み。レベルは本体の量子化で指定値と異なる場合があります。":
    "Source, coupling, edge and mode changes, and Single STOP are hardware-tested. Instrument quantization may change the requested level.",
  "トリガー: 現在値を読取り、1項目ずつ適用・読戻し。Singleの再待受は本体RUN/STOP":
    "Trigger: read current settings, apply one parameter and verify. Re-arm Single with the instrument RUN/STOP.",
  トリガー操作: "Trigger controls",
  トリガレベル: "Trigger level",
  本体状態: "Instrument state",
  トリガー現在値を読取り: "Read current trigger",
  トリガー設定項目: "Trigger parameter",
  トリガー変更値: "Trigger new value",
  トリガーレベル変更値: "Trigger new level",
  トリガーレベル単位: "Trigger level units",
  "トリガーへ適用・読戻し": "Apply trigger / read back",
  "トリガーの現在値を読みました。設定は変更していません。":
    "Trigger settings read; no settings were changed.",
  Singleで取得状態が変わる可能性を確認しました:
    "I understand Single may change the acquisition state",
  "Auto: 条件なしでも更新。Normal: 条件成立時に更新。Single: 1回の取得で停止。":
    "Auto: updates without a trigger. Normal: updates on a trigger. Single: stops after one acquisition.",
  "Singleの再待受は本体のRUN/STOPで行ってください。Macの「1回取得」は待受操作ではありません。":
    "Re-arm Single with RUN/STOP on the instrument. Acquire once on the Mac does not arm a trigger.",
  "設定は1項目ずつ適用。本体の手動変更を検出すると書込みせず再読取りを求めます。":
    "One parameter per apply. Manual instrument changes are detected; read again before writing.",
  "HDS25S V12.1.0の応答済みコマンドを使用。レベルは本体の範囲・分解能に制限されます。":
    "Uses observed HDS25S V12.1.0 commands. Level is limited by instrument range and resolution.",
  "トリガー操作は現在OWON HDS25S V12.1.0のみ対応しています":
    "Trigger controls currently support OWON HDS25S V12.1.0 only",
  未対応のトリガー設定値です: "Unsupported trigger setting",
  "トリガーレベルは±100 V以内です。本体の範囲・分解能にも制限されます。":
    "Trigger level must be within ±100 V, and is also limited by instrument range and resolution.",
  "GEN OUT操作": "GEN OUT controls",
  出力: "Output",
  未確認: "Not checked",
  現在値を読取り: "Read current settings",
  波形種: "Waveform type",
  振幅: "Amplitude",
  オフセット: "Offset",
  "負荷（読取りのみ）": "Load (read only)",
  読取り時点の設定: "settings at read time",
  "最終値。現在の設定は未確認": "last values; current settings are unknown",
  変更する項目: "Parameter to change",
  "GEN OUT設定項目": "GEN OUT parameter",
  "GEN OUT波形種": "GEN OUT waveform",
  "GEN OUT変更値": "GEN OUT new value",
  "GEN OUT単位": "GEN OUT units",
  "GEN OUTへ適用・読戻し": "Apply to GEN OUT / read back",
  "GEN OUTの現在値を読みました。出力状態は変更していません。":
    "GEN OUT settings read; output state was not changed.",
  "（読戻し不一致。再読取りして確認）":
    " (readback differs; read again to check)",
  "指定値または他の設定が一致しません。自動で戻さず、再読取りしてください。":
    "Requested or other settings differ. No automatic rollback; read again to check.",
  出力OFF: "Output OFF",
  "出力ON…": "Output ON…",
  "GEN OUT出力ONの確認": "Confirm GEN OUT output ON",
  "GEN OUT端子から信号が出ます。接続先と上の設定を確認してください。":
    "The GEN OUT port will emit a signal. Check the connected circuit and the settings above.",
  接続先と設定を確認しました: "I checked the connected circuit and settings",
  確認して出力ON: "Confirm and enable output",
  キャンセル: "Cancel",
  "USB接続後に現在値を読みます。設定は自動適用しません。":
    "Current settings are read after USB connection. Settings are never applied automatically.",
  "GEN OUT操作は現在HDS25Sのみ対応しています。":
    "GEN OUT controls currently support HDS25S only.",
  "振幅0.02〜5 Vpp。アプリ安全制限: |オフセット| + 振幅/2 ≤ 2.5 V。負荷INFのみ。":
    "Amplitude 0.02–5 Vpp. App safety limit: |offset| + amplitude/2 ≤ 2.5 V. INF load only.",
  "出力ON/OFFは独立操作です。任意波形・パルス幅・デューティ・負荷切替は未対応。":
    "Output ON/OFF is separate. Arbitrary waveforms, pulse width, duty cycle and load switching are not supported.",
  "周波数変更・出力ON/OFFの読戻しは実機確認済み。全設定範囲・端子実電圧は検証未完了です。":
    "Frequency changes and output ON/OFF readback are hardware-tested. Full ranges and terminal voltages are not yet verified.",
  有限の数値と正しい単位を入力してください:
    "Enter a finite number with valid units",
  未対応の波形種です: "Unsupported waveform type",
  "出力はON/OFFです": "Output must be ON/OFF",
  本体の数値応答を確認してください: "Check the instrument's numeric responses",
  周波数が波形種の許可範囲外です:
    "Frequency exceeds this waveform's permitted range",
  "振幅は0.02〜5 Vppです": "Amplitude must be 0.02–5 Vpp",
  "安全制限: |オフセット| + 振幅/2 ≤ 2.5 V":
    "Safety limit: |offset| + amplitude/2 ≤ 2.5 V",
  負荷INF以外は設定変更未対応です: "Settings changes require INF load",
  出力状態が不明です: "Output state is unknown",
  "設定変更は、接続先を確認して出力ONにした後で行ってください":
    "Check the connected circuit and enable output explicitly before changing settings",
  "本実機では波形指定で出力がONになりました。設定変更は出力ON中のみ許可します。":
    "This instrument enabled output when selecting a waveform. Configuration changes require output to be ON already.",
  "GEN OUT: 1項目ずつ適用。出力ONには接続先の確認が必要":
    "GEN OUT: apply one parameter at a time; enabling output requires a connection check",
  "HDS25S V12.1.0では公開仕様のトリガ照会が非応答のため、トリガ変更は無効です。":
    "Trigger control is disabled: the documented query did not respond on HDS25S V12.1.0.",
  "ヘルプ / バージョン": "Help / About",
  "USB 接続中": "USB connected",
  未接続: "Disconnected",
  取得とファイル操作: "Acquisition and files",
  接続先: "USB device",
  USBデバイスを選択: "Select USB device",
  USBを再検索: "Scan USB devices",
  再検索: "Refresh",
  切断: "Disconnect",
  接続: "Connect",
  "▶ 表示再開": "▶ Resume preview",
  "Ⅱ 表示停止": "Ⅱ Pause preview",
  "1回取得": "Acquire once",
  JSON保存: "Save JSON",
  "開く…": "Open…",
  "再生フォルダー…": "Playback folder…",
  履歴: "Recent",
  まだ履歴はありません: "No recent files",
  表の出力形式: "Table export format",
  表出力: "Export table",
  画像形式: "Image format",
  画像保存: "Save image",
  "印刷…": "Print…",
  閉じる: "Close",
  波形: "Waveform",
  データ表: "Data table",
  "RAW / 未校正": "RAW / uncalibrated",
  表示リセット: "Reset view",
  "LIVE · 逐次取得": "LIVE · sequential reads",
  STOP確認済み: "STOP checked",
  "表示停止 / 保存データ": "Paused / saved data",
  表示CH: "Channels",
  両CH: "Both channels",
  前: "Previous",
  次: "Next",
  受信位置: "Byte index",
  受信値: "Raw values",
  平均: "Mean",
  "時間・電圧換算": "Time / voltage conversion",
  本体の自動測定値: "Instrument measurements",
  "取得 · 更新は「本体の測定値を取得」":
    "read · Refresh with Read measurements",
  周波数: "Frequency",
  周期: "Period",
  最大: "Maximum",
  最小: "Minimum",
  停止: "Stop",
  再生: "Play",
  再生位置: "Playback position",
  逆方向: "Reverse",
  再生間隔ms: "Playback interval ms",
  取得: "Acquisition",
  照会の最小間隔: "Minimum query interval",
  "（最速）": " (fastest)",
  "表示更新 / 秒": "Preview updates / s",
  USB照会: "USB query",
  "本体STOPを確認して取得・保存": "Capture checked STOP / save",
  表示: "Display",
  反転: "Invert",
  上下に分割: "Stack channels",
  点表示: "Points",
  グリッド: "Grid",
  背景: "Background",
  背景色: "Background color",
  格子: "Grid",
  格子色: "Grid color",
  カーソル: "Cursors",
  なし: "None",
  "X（受信位置）": "X (byte index)",
  "Y（受信値）": "Y (raw value)",
  解析: "Analysis",
  演算: "Math",
  窓関数: "Window",
  "dB（基準 1 raw RMS）": "dB (re 1 raw RMS)",
  自動保存: "Recording",
  "保存先…": "Folder…",
  未指定: "Not selected",
  自動保存間隔ms: "Recording interval ms",
  保存停止: "Stop recording",
  保存開始: "Start recording",
  記録中: "Recording",
  ファイル: "files",
  保存済み: "Saved",
  "回スキップ（表示停止・未更新）": "skips (paused / no new frame)",
  "記録先:": "Session:",
  本体情報: "Instrument",
  機種: "Identity",
  "状態 / 時間軸": "Status / timebase",
  "ADC rate / メモリ": "ADC rate / memory",
  本体設定: "Instrument settings",
  設定情報の経過時間: "Metadata age",
  次の実装: "Remaining work",
  "時間・電圧の校正 / BIN互換":
    "Time / voltage calibration · BIN compatibility",
  "自然言語AI操作（未実装・API送信なし）":
    "AI control (not implemented; no API calls)",
  "処理中…": "Working…",
  本体CH表示: "Instrument channel display",
  結合: "Coupling",
  プローブ倍率: "Probe attenuation",
  "感度（プローブ倍率込み）": "Scale (including probe)",
  "オフセット（SCPI設定値）": "Offset (SCPI units)",
  "時間軸 / div": "Timebase / div",
  "水平位置（−10〜10）": "Horizontal offset (−10 to 10)",
  取得方式: "Acquisition mode",
  メモリ長: "Memory depth",
  トリガ源: "Trigger source",
  トリガ結合: "Trigger coupling",
  エッジ: "Edge",
  トリガモード: "Trigger mode",
  "トリガレベル（V / mV）": "Trigger level (V / mV)",
  本体操作: "Instrument control",
  対象: "Target",
  本体操作の対象: "Control target",
  時間軸: "Timebase",
  トリガ: "Trigger",
  項目: "Parameter",
  本体設定項目: "Instrument parameter",
  変更値: "New value",
  本体設定の変更値: "New instrument value",
  "ヘッダー参考:": "Header reference:",
  "（読戻し一致）": " (readback matches)",
  "（指定値と不一致。本体対応・範囲を確認）":
    " (readback differs; check support / range)",
  "本体へ適用・読戻し": "Apply to instrument / read back",
  本体の測定値を取得: "Read instrument measurements",
  印刷プレビュー: "Print preview",
  横向き: "Landscape",
  上: "Top",
  右: "Right",
  下: "Bottom",
  左: "Left",
  "余白 mm": "margin mm",
  "印刷 / PDF保存…": "Print / Save PDF…",
  保存したPC波形表示: "Frozen PC waveform rendering",
  "USBが見つかりません。本体のUSBをHIDにし、Python版などを切断して再検索してください。":
    "No USB instrument found. Select HID on the instrument, close other USB apps and refresh.",
  USBを切断しました: "USB disconnected",
  実機を選択してください: "Select an instrument",
  USBを切断してから開いてください: "Disconnect USB before opening files",
  フォルダーにJSON波形がありません: "No JSON waveforms in this folder",
  再生リストは100ファイルまでです:
    "Select up to 100 files; use folder playback for larger recordings",
  波形がありません: "No waveform",
  波形表示へ切り替えてください: "Switch to a plot view",
  PNG変換に失敗: "PNG encoding failed",
  "各行は受信バイト。電圧値・ADCサンプルではありません。":
    "Rows are screen bytes, not voltage values or ADC samples.",
  "受信byte列のスペクトル。周波数Hz・電圧Vrmsの計測ではありません。":
    "Spectrum of screen bytes; not a measurement in Hz or Vrms.",
  "ドラッグ: カーソル移動 · Shift+ドラッグ: 波形移動":
    "Drag: cursors · Shift+drag: pan",
  "ドラッグ: 波形移動 · ホイール: X拡大 · Shift+ホイール: Y拡大":
    "Drag: pan · Wheel: X zoom · Shift+wheel: Y zoom",
  "受信byteの意味を検証後に有効化。":
    "Available after verifying the screen-byte encoding.",
  "本体のADC rateからΔtを推定しません。":
    "ADC rate is not used to infer screen-byte timing.",
  "表示停止はMacの取得を止めます。本体のRUN/STOPは変更しません。":
    "Pause stops PC queries, not the instrument's RUN/STOP.",
  "カーソルは重ね合わせ表示で使用します。":
    "Cursors require overlaid channels.",
  "Δt / ΔV は校正後に対応します。": "Δt / ΔV requires verified calibration.",
  "平均値を除去し、先頭の2の累乗長を使用。画面byte列の解析です。":
    "DC removed; leading power-of-two length. Screen-byte analysis only.",
  "ライブの逐次取得をJSONに記録。同一収録やΔt測定を保証する保存ではありません。":
    "Records sequential live reads to JSON; channel alignment is unverified.",
  "Rust側で保存。背面でも動作し、表示停止・未更新なら重複保存しません。Macのスリープ中は停止します。":
    "Native Rust recording runs in background. Paused or unchanged frames are skipped. System sleep suspends recording.",
  "本体設定の参考表示。グラフの校正には使用していません。":
    "Instrument metadata; not used to calibrate this plot.",
  "接続を選択してください。旧Python版はそのまま残しています。":
    "Select an instrument. The original Python app is preserved.",
  "本体の設定を変更します。表示設定とは別です。変更前後をSCPIで読み戻します。":
    "Changes instrument settings, separately from PC display. SCPI readback checks before and after.",
  "（感度・位置はSCPI読戻しと表現が異なる場合があります）":
    " (scale / offset may differ from SCPI readback representation)",
  "本体測定値を読みました。波形とは別の逐次照会です。":
    "Instrument measurements read sequentially, separately from the waveform.",
  "未確認コマンドは機種・FWによって非対応の場合があります。RUN/STOP・Auto設定・出力端子は操作しません。":
    "Support varies by model / firmware. No RUN/STOP, autoset or output control.",
  "本体の測定応答をそのまま表示。CH・項目を順番に照会し、表示波形との同時性は保証しません。":
    "Instrument responses shown unchanged. Channels / items are queried sequentially, not synchronized with the plot.",
  "PC表示 / 受信byte位置・signed raw値（未校正）。本体画面の画像ではありません。":
    "PC rendering / byte index and signed raw values (uncalibrated). Not an instrument screenshot.",
  "ブラウザーの表示確認モードです。USB接続・ファイル操作はTauriアプリで使用できます。模擬波形は表示しません。":
    "Browser layout preview. USB and files require the Tauri app. No simulated waveform.",
  "公式PCソフトの機能を独立実装中。OWON公式アプリではありません。":
    "Independent implementation of PC software features; not an official OWON application.",
  "CH1／CH2は汎用の入力チャンネルです。":
    "CH1 and CH2 are general-purpose input channels.",
  "Space: 表示停止・再開（本体RUN/STOPではありません）":
    "Space: pause / resume PC preview (not instrument RUN/STOP)",
  "Cmd/Ctrl+S: JSON保存 · +O: 開く · +P: 印刷プレビュー":
    "Cmd/Ctrl+S: save JSON · +O: open · +P: print preview",
  "R: 表示リセット · ドラッグ: 移動 · ホイール: X拡大 · Shift: Y拡大":
    "R: reset view · Drag: pan · Wheel: X zoom · Shift: Y zoom",
  "本体操作: 取得設定を変更し、変更前後の応答を表示":
    "Instrument control: change acquisition settings with before / after readback",
  "連続保存: 保存先内に新しいセッションフォルダーを作成":
    "Recording: creates a new session subfolder",
  "波形軸は未校正。時間・電圧カーソル、公式BIN互換は未対応":
    "Uncalibrated axes. Physical-unit cursors and official BIN compatibility pending.",
  "AI・外部API通信はありません。ファイルを上書きしません。":
    "No AI or external API traffic. Existing files are never overwritten.",
};
const Context = createContext({
  language: "ja",
  setLanguage: (_language: string) => {},
  t: (text: string) => text,
});
export function LocaleProvider({ children }: { children: ReactNode }) {
  const [language, update] = useState(() =>
    localStorage.getItem("owon-language") === "en" ? "en" : "ja",
  );
  useEffect(() => {
    document.documentElement.lang = language;
  }, [language]);
  const setLanguage = (value: string) => {
    const lang = value === "en" ? "en" : "ja";
    localStorage.setItem("owon-language", lang);
    update(lang);
    document.documentElement.lang = lang;
  };
  return (
    <Context.Provider
      value={{
        language,
        setLanguage,
        t: (s) => (language === "en" ? (english[s] ?? s) : s),
      }}
    >
      {children}
    </Context.Provider>
  );
}
export const useLocale = () => useContext(Context);
