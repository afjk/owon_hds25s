# OWON Scope API仕様書

Tauri版 OWON Scope 0.4.1のアプリ内APIと、HDS25Sで実機確認したUSB操作をまとめます。自然言語操作やUI拡張を行う際の契約と、未検証の操作を区別するための文書です。

確認日：2026年10月3日。実機：HDS25S／V12.1.0。環境：macOS、Apple Silicon。

`research/`・`tmp/`の実機生記録と第三者配布物はローカル保管のため、公開リポジトリには含みません。
CH1／CH2は汎用入力です。センサー種別・用途を固定しません。Windowsでの実機接続は未検証です。

GEN OUTの7項目の照会、1 kHz→2 kHzの周波数変更、出力ON/OFF操作を実機確認しています。0.4ではトリガー専用UIと2 APIを追加し、源・結合・エッジ・モードの変更／復元、SingleのSTOPをGEN OUT入力で確認しました。レベルの量子化と源・結合に連動する値も記録しています。HDS25S V12.1.0のみ、競合検出付きの専用APIで書込みを許可します。同じSINEの指定でも本体がOFF→ONになる挙動を観測したため、GEN OUTの設定変更は出力ON中のみ許可します。全設定範囲は実機検証未完了です。

## APIの呼出し方法

HTTP／RESTサーバーではなく、Tauri WebViewからRustへ呼ぶ内部IPCです。外部アプリやAIから直接接続できる公開ネットワークAPIはありません。

```ts
import { invoke } from '@tauri-apps/api/core';

type DeviceInfo = { bus: number; address: number; label: string };
const devices = await invoke<DeviceInfo[]>('list_devices');
// 接続先はUIで選択したdevices内の1台を使う。bus/addressは固定しない。
```

- コマンド名は `snake_case`。引数オブジェクトは `intervalMs` などの `camelCase`。
- 戻り値の構造体フィールドは `query_ms` などの `snake_case`。設定オブジェクトは `target`、`parameter`、`value`。
- Rustの `Result<T, String>` は成功時に `T`、失敗時にPromiseを文字列でrejectします。`()` はJSONの `null`。正式なエラーコード体系はありません。
- USB操作は1本のワーカースレッドで直列実行します。別アプリ・診断CLIと同時に実機を開かないでください。
- 一部APIの成功は「キューへの投入成功」です。実機取得の完了は `poll_preview` の新しいフレームやエラーを確認します。

登録元：[main.rs](../src-tauri/src/main.rs)。以下の26個が現在の登録済みAPIの全てです。Auto実行は検証待ちで無効です。

## Auto設定（0.4.1、実行は検証待ち）

| API | 引数 | 戻り値 | 本体への影響 |
| --- | --- | --- | --- |
| `autoset_available` | なし | `boolean`（現在false） | USB操作なし |
| `read_autoset_settings` | なし | `{scope: Record<string,string>, trigger: TriggerSnapshot, generator: GeneratorSnapshot}` | 14項目の入力・水平・取得設定、トリガ6項目、GEN7項目を逐次読取り。設定は変更しない。返信待ち32秒 |
| `autoset_device` | `{request: {expected: 上記Snapshot, confirm: boolean}}` | AutoReply | 現在Rust側で書込みを拒否。将来有効化時は全設定の競合検出→固定`:AUT .`を1回送信→最大4回の読戻し安定確認。GEN書込み・自動復元なし |

AutoReplyは`before`／`after`、変更一覧`changes: {field,before,after}[]`、`readback_stable`、`readback_reads`、`generator_preserved`、`effect_observed`を返します。設定が同じ場合、専用応答がないためAutoの実行成功とは断定しません。

実機へのAuto送信前の復元事前テストで、CH2の100 V/div再指定が200 V/divへ変わりました。別表記100Vでの復元も不一致だったため、その後の実機書込みを停止しました。GENはOFFです。記録は`research/hardware/20261004-auto-restore-preflight.json`と`20261004-auto-original-scale-restore.json`。保存済み元設定と一致する復元手順が確認できるまでAuto実行を有効化しません。

2026-10-04の追確認：ユーザーが本体でCH2を100 V/divへ戻した後、読取専用の全設定照会で入力・水平・取得14項目、トリガ設定5項目、GEN設定7項目が元の記録と一致しました。記録：`research/hardware/20261004-after-manual-scale-restore-read.json`。本体操作による復元は確認済みですが、遠隔復元の不一致の原因はまだ未特定です。Auto実行の制限は解除していません。

トリガーSettingには任意の`rounding_workaround?: boolean`（省略false）を追加。Level専用で、非ゼロの送信電圧に符号付き100 µVを加えます。±100 V上限を超える補正は送信前に拒否。Replyには`requested_value`と`rounding_workaround`を追加し、`verified`は補正前の指定値に対して比較します。160 mVは維持できましたが、320 mVは160 mV読戻しで不一致。解決を保証する補正ではありません。記録は`research/hardware/20261004-trigger-rounding-workaround.json`。

参考は[seritoolsのAuto・丸め対策実装](https://github.com/seritools/owowon/blob/d66a5227799fcdb8e9a18cf93afbd9fba084ae76/src/device.rs)。コマンド・手法を参考にした独立実装で、リポジトリのコードやバイナリはアプリへコピーしていません。

## 接続と波形取得

| API | 引数 | 成功戻り値 | 本体への影響と注意 |
| --- | --- | --- | --- |
| `list_devices` | なし | `DeviceInfo[]` | USB列挙のみ。VID `0x5345`／PID `0x1234`で絞り込み。空配列も正常な戻り値 |
| `connect_device` | `{bus: number, address: number}` | 識別文字列 | USBを開き `*IDN?` を照会。その後、連続読取りを開始。本体設定は書き換えない |
| `disconnect_device` | なし | `null` | PCの記録・USBワーカーを終了。本体のRUN/STOPは変えない。未接続でも成功 |
| `set_preview` | `{paused: boolean, intervalMs: number}` | `null` | PCの連続照会だけ停止／再開。間隔は整数17〜10000 ms。キュー受付の応答 |
| `acquire_once` | なし | `null` | 設定ヘッダーを更新し現在の波形を1回読む要求。キュー受付の応答。本体のSINGLEトリガ操作ではない |
| `poll_preview` | `{after: number}` | `Preview` | PCの最新データを取り出す。これ自体はUSB照会しない。`sequence <= after`なら `frame: null` |
| `capture_stopped` | なし | `Frame` | 本体STOP・両CH ON・取得前後の設定一致・波形長一致を検査して読む。アプリから本体を停止しない |

`bus`／`address`はRustの `u8`（0〜255）。整数で指定します。接続中の二重接続は拒否されます。抜き差し後は再列挙してください。

`set_preview`／`acquire_once`は状態反映を待ちません。例えば `paused: true` の受付直後に、進行中の1回の取得が完了することがあります。キューに大量の単発要求を投入しないでください。

`capture_stopped`は成功・失敗のどちらでもPCの連続照会を停止します。返信待ち上限は16秒ですが、USB操作を取り消す仕組みではありません。取得結果は直接返され、ライブの最新メールボックスを置き換えるAPIではありません。保存には別途 `save_record` を呼びます。UIは先に自動保存を停止しますが、`capture_stopped` 単独には記録停止の処理がありません。

### 戻り値の型

```ts
type Frame = {
  record: ScreenRecord;
  values: Partial<Record<'CH1' | 'CH2', number[]>>; // signed int8: -128〜127
  query_ms: number;         // PC側で測った今回の取得所要時間
  metadata_age_ms: number;  // キャッシュした本体情報の経過時間
  sequence: number;         // ライブでは接続内の取得順。ファイル読込／STOP取得は0
};

type RecordingStatus = {
  active: boolean;
  saved: number;
  skipped: number;
  directory: string | null;
  error: string | null;
};

type Preview = {
  frame: Frame | null;
  paused: boolean;
  error: string | null;      // 取得失敗はこのフィールドにも現れる
  recording: RecordingStatus;
};

type ScreenRecord = {
  schema_version: 1;
  source: 'owon_hds200_screen';
  identity: string;
  status_before: string;
  status_after: string;
  header: Record<string, unknown>; // 本体から受信したJSON
  channels: Partial<Record<'CH1' | 'CH2', {
    byte_count: number;
    payload_hex: string;           // 受信ペイロードのhex、フレーム長の4バイトは除く
  }>>;
  consistency: string;
  calibration: 'sample_encoding_and_time_axis_not_yet_verified';
  retrieved_at_unix_ms?: number;   // 現在の保存処理で付加。旧STOP JSONでは省略可
  retrieved_at_note?: string;
  sensor_assignments?: {CH1: 'audio' | 'light'; CH2: 'audio' | 'light'};
  metadata_age_ms?: number;        // ライブで付加
};
```

`consistency` はライブで `live_sequential_queries; channel_acquisition_alignment_unverified`、STOP確認済みで `manual_stop_observed; no acquisition_id_available` です。

0.4以降の新規取得では `sensor_assignments` を付加しません。過去の入力割当てが入った保存ファイルは読込み可能ですが、表示・測定CHを固定するためには使用しません。

ライブの `status_before`／`status_after` はキャッシュされた同じ状態を保存したもので、各フレームの前後2回の検査ではありません。STOP取得のみ前後検査を行います。本体情報はライブで約1秒キャッシュされ、単発・表示切替・本体設定操作後に無効化されます。

受信バイト位置はADCサンプル番号ではなく、値は校正済みのVでもありません。600バイトの意味と時間軸変換は未検証です。CH間は逐次照会で同一収録IDがないため、この `Frame` から精度保証されたΔtを算出するAPIはまだありません。`retrieved_at_unix_ms` はMacの取得時刻であり、光／音のイベント時刻ではありません。

## 本体設定の書込み

`set_device_setting({setting: {target, parameter, value}})` → `SettingReply`。
**このAPIは実機設定を書き換えます。読取りだけのAPIではありません。**

```ts
type Setting = { target: string; parameter: string; value: string };
type SettingReply = {
  command: string;  // 送信した設定コマンド。末尾改行を含まない
  before: string;   // 変更前の照会応答
  after: string;    // 変更後の照会応答
  verified: boolean; // 要求値との比較結果。falseも正常戻り値
};
```

変更前照会 → 設定送信 → 変更後照会の順に実行します。単位・一部省略表記を正規化して比較しますが、完全なSCPI数値／単位パーサーではありません。16秒の返信待ち上限があります。書込み後のタイムアウトでは **設定が既に反映されている可能性があります**。エラーや `verified: false` でも自動で元へ戻しません。確認せず再試行しないでください。

`target`／`parameter`は表の綴りと大小文字で指定します。`value`は文字列で、前後の空白を除去し、列挙値は大小文字を無視して検証します。空文字、非ASCII、`;`、改行、`?`、`:`などを含む値は拒否します。

| target | parameter | valueの許可範囲 | 現在コンパイルされるSCPIパス |
| --- | --- | --- | --- |
| `CH1`／`CH2` | `display` | `ON`／`OFF` | `:CH<n>:DISPLAY` |
| `CH1`／`CH2` | `coupling` | `AC`／`DC`／`GND` | `:CH<n>:COUPLING` |
| `CH1`／`CH2` | `probe` | `1X`／`10X`／`20X`／`100X`／`1000X` | `:CH<n>:PROBE` |
| `CH1`／`CH2` | `scale` | 下記の感度一覧 | `:CH<n>:SCALE` |
| `CH1`／`CH2` | `offset` | 有限の数値文字列、−200〜200 | `:CH<n>:OFFSET` |
| `horizontal` | `scale` | 下記の時間軸一覧 | `:HORIZONTAL:SCALE` |
| `horizontal` | `offset` | 有限の数値文字列、−10〜10 | `:HORIZONTAL:OFFSET` |
| `acquire` | `mode` | `SAMPLE`／`PEAK` | `:ACQUIRE:MODE` |
| `acquire` | `memory` | `4K`／`8K` | `:ACQUIRE:DEPMEM` |

これは **アプリ側の許可範囲** です。全ての値をHDS25Sで書込み確認したという意味ではありません。オフセットのSCPI単位も未検証で、ヘッダー内の `OFFSET` と同じ尺度と決めつけないでください。ヘッダーから読めたCH1の `20X` は現在の書込み許可値にはありません。

感度の全15候補：

```text
10.0mV 20.0mV 50.0mV 100mV 200mV 500mV 1.00V 2.00V
4.00V 5.00V 10.0V 20.0V 50.0V 100V 200V 500V
```

時間軸の全35候補：

```text
5.0ns 10.0ns 20.0ns 50.0ns 100ns 200ns 500ns
1.0us 2.0us 5.0us 10us 20us 50us 100us 200us 500us
1.0ms 2.0ms 5.0ms 10ms 20ms 50ms 100ms 200ms 500ms
1.0s 2.0s 5.0s 10s 20s 50s 100s 200s 500s 1000s
```

### トリガー専用API（0.4）

旧 `set_device_setting` の `target: 'trigger'` は拒否します。次の専用APIを使ってください。

| API | 引数 | 成功戻り値 | 本体への影響 |
| --- | --- | --- | --- |
| `read_trigger` | なし | `TriggerSnapshot` | 5項目＋状態の逐次照会のみ。設定を書かない。返信待ち18秒 |
| `set_trigger_setting` | `{setting: TriggerSetting}` | `TriggerReply` | 5項目の競合検査→1項目書込み→5項目と状態の読戻し。返信待ち32秒 |

```ts
type TriggerSnapshot = {
  source: string; coupling: string; edge: string; sweep: string; level: string;
  status: string; read_at_unix_ms: number;
};
type TriggerSetting = {
  parameter: 'source' | 'coupling' | 'edge' | 'sweep' | 'level';
  value: string;
  expected: TriggerSnapshot; // 直前のread_trigger／設定結果。書込み前に5項目を比較
  confirm_single?: boolean; // sweep SINGLEに必須
};
type TriggerReply = {
  command: string; before: TriggerSnapshot; after: TriggerSnapshot;
  verified: boolean; preserved: boolean;
  linked_level_changed: boolean; // source/couplingと連動したレベルの変化
  readback_stable: boolean; readback_reads: number; // 最大4回、2回連続一致
};
```

| parameter | 許可値 | 現在のSCPIパス |
| --- | --- | --- |
| `source` | `CH1`／`CH2` | `:TRIGGER:SOURCE` |
| `coupling` | `AC`／`DC` | `:TRIGGER:COUPLING` |
| `edge` | `RISE`／`FALL` | `:TRIGGER:SINGLE:EDGE` |
| `sweep` | `AUTO`／`NORMAL`／`SINGLE` | `:TRIGGER:SWEEP` |
| `level` | 有限値、絶対値100 V以下。V／mV／uV／nV／pVまたは単位なし | `:TRIGGER:SINGLE:EDGE:LEVEL` |

レベルのnV／pVはファームウェアの微小・ゼロ応答を正規化するための対応で、その分解能を保証しません。UI入力はV／mV／uVのみ。本体の量子化による差も不一致として通知し、自動補正しません。

型番・メーカー・FWをチェックし、現在は `OWON,HDS25S,…,V12.1.0` だけ許可します。`expected` と実機の5項目が違えば書込みなしで終了します。時刻と状態は設定の比較から除外します。書込み後は50msの待機と5項目＋状態の照会を最大4回行い、2回連続で設定が一致すると `readback_stable` がtrueになります。書込みは1回のみです。

`preserved` は指定項目以外の4項目がすべて保持された場合のみtrue。`verified` は安定した指定値一致と他項目の保持を要求しますが、source／coupling操作に限り、実機で観測した連動レベルの変化を許容し、`linked_level_changed` とbefore／afterで明示します。この場合 `verified: true, preserved: false` になり得ます。それ以外の連動変化は許容しません。エラー・不一致時は再読取りを要求し、自動再試行／復元はしません。

接続・ファイル読込で設定を自動適用しません。SINGLEの選択は取得状態を変える可能性があり、再待受を保証するAPIではありません。本体RUN/STOPで再待受してください。Macの表示停止／1回取得は本体の停止／待受操作ではありません。GEN OUTを両CHへ接続した1kHz正弦波で、源・結合・エッジ・モードの変更、SingleのSTOP、停止中2回の両CHペイロード一致、Autoへの復元と再開を確認しました。

現在のCH1 4V/div／20X設定ではレベルの320mV指定→160mV、160mV指定→0V、200mv指定→160mVを観測しました。量子化による切り下げと考えられますが、全設定での換算則は未確定です。レベルの指定値と読戻しが異なれば `verified: false` のまま通知し、自動補正しません。試験後は元の5項目とGEN OFFを復元済みです。実機記録とレベル復元の生記録はローカル保管（公開対象外）です。

設定の独立した読取りAPIは現在ありません。UIは取得ヘッダーを表示します。`set_device_setting` を設定読取りの代わりに使わないでください。

実装：[control.rs](../crates/owon-core/src/control.rs)、[worker.rs](../src-tauri/src/worker.rs)。0.2実装時の書込み確認はCH1結合のDC→DCのみで、今回の診断ではそれも実行していません。

## 本体の自動測定値

`read_measurements`（引数なし）→ 以下の形。32秒の返信待ち上限があります。

```ts
type MeasurementResult = {
  values: Record<'CH1' | 'CH2', Record<
    'FREQUENCY' | 'PERIOD' | 'PKPK' | 'MAX' | 'MIN' | 'AVERAGE', string
  >>;
  read_at_unix_ms: number;
};
```

SCPIは `:MEASUREMENT:CH<n>:<項目>?`。CH1の6項目、CH2の6項目の順に照会します。周波数はHz、周期はs、Vpp／最大／最小／平均はVとして扱いますが、数値は本体の文字列を保存します。完了時のMac時刻を付加します。波形との同時性や、各測定項目間の同時性は保証しません。周波数／周期のゼロ応答を、入力の物理的な周波数ゼロと断定しないでください。

12項目の照会は0.2実装時に実機確認済み。今回も診断後にアプリで読取り成功を確認しました。

## GEN OUTの操作API

HDS25Sのみ対応します。機種をUSBワーカー内の識別文字列で確認し、他機種へGEN OUT照会を送りません。通常の波形取得と同じワーカーで直列実行します。

| API | 引数 | 成功戻り値 | 本体への影響 |
| --- | --- | --- | --- |
| `read_generator` | なし | `GeneratorSnapshot` | 7項目の読取りのみ。返信待ち20秒 |
| `set_generator_setting` | `{setting: GeneratorSetting}` | `GeneratorReply` | 1項目を変更し、全7項目を前後で読み戻す。返信待ち36秒。失敗時の自動再試行・巻戻しなし |

```ts
type GeneratorSnapshot = {
  waveform: string;
  frequency: string;  // Hz、本体の生文字列
  period: string;     // s
  amplitude: string;  // Vpp
  offset: string;     // V
  output: string;     // ON / OFF。不明な応答もそのまま残す
  load: string;       // 実機応答はINF。boolean化しない
  read_at_unix_ms: number;
};
type GeneratorSetting = {
  parameter: 'waveform' | 'frequency' | 'amplitude' | 'offset' | 'output';
  value: string;                  // 数値は単位なしのHz / Vpp / V
  expected: GeneratorSnapshot;    // 最後に読んだ設定全体
  confirm_output?: boolean;      // 省略はfalse。出力ONのみtrueを要求
};
type GeneratorReply = {
  command: string;
  before: GeneratorSnapshot;
  after: GeneratorSnapshot;
  verified: boolean;             // 指定値の一致と、非指定項目の保持
  preserved: boolean;            // 非指定項目の保持
};
```

| parameter | SCPI書込み | アプリの許可範囲 |
| --- | --- | --- |
| `waveform` | `:FUNCTION <値>` | `SINE`／`SQUARE`／`RAMP`／`PULSE`。変更後の波形種に現在周波数が収まる必要がある |
| `frequency` | `:FUNCTION:FREQUENCY <Hz>` | 最小0.1 Hz。SINE最大10 MHz、SQUARE最大2 MHz、RAMP最大1 MHz、PULSE最大5 MHz |
| `amplitude` | `:FUNCTION:AMPLITUDE <Vpp>` | 0.02〜5 Vpp、下記の電圧包絡制限内 |
| `offset` | `:FUNCTION:OFFSET <V>` | 下記の電圧包絡制限内 |
| `output` | `:CHANNEL ON`／`:CHANNEL OFF` | ONは `confirm_output: true` を要求。OFFは電圧／波形種／負荷の制限を受けない |

周波数・振幅の範囲は[OWONのHDS25S仕様](https://www.owon.com.hk/products_owon_hds200_series_digital_oscilloscope)を参照し、これにアプリ独自の保守的な制限 `|offset| + amplitude/2 ≤ 2.5 V` を加えています。これは機器の絶対定格や接続先回路の安全保証ではありません。OFF以外の変更は負荷応答が `INF` の場合に限り、負荷切替は行いません。

送信直前に読み直した全7項目と `expected` を比較します。本体の手動変更などで異なっていれば書込みせず、再読取りを求めます。ただし明示的な出力OFFは、他の設定が変わっていても許可します。読取り時刻自体は一致条件に含めません。数値表記違いは数値として比較します。周波数変更では周期の連動変更を許可し、他の項目は保持を検査します。

HDS25S／V12.1.0の実機では、出力OFF中に同じ波形種の `:FUNCTION SINE` を送るだけでも出力ONになりました。前後の全項目検査で不一致を検出し、独立した `:CHANNEL OFF` 操作でOFFへ戻したことを読み戻しています。このため、波形種・周波数・振幅・オフセットは、前回値と送信直前値の両方がONの場合のみ許可します。設定変更のためにアプリが自動でONにする処理はありません。OFF中に設定だけを変更する用途は、現在の安全制限では対応しません。

UIは接続後に現在値を1回読み、設定値を自動適用しません。設定変更は1項目ずつ「GEN OUTへ適用・読戻し」で送ります。出力ONは別ボタンから確認チェックと確認ボタンを経由します。出力OFFは独立ボタンです。書込み後の不一致・エラーでは設定状態を未確認にし、再読取りするまで通常の変更を無効にします。通信失敗の場合はUSB再接続が必要です。

原子性はありません。本体が書込みを受け取った後に応答が途切れると、APIがrejectしても設定は反映済みかもしれません。自動で出力OFFや元設定へ戻す処理はありません。ユーザーが接続先回路を確認してから操作してください。

最終アプリ切替の際に波形の受信長0エラーを観測し、新規接続では出力OFFを読みました。切断・接続処理にOFF送信やUSBリセットはありませんが、状態変化の原因は未確定です。再接続による本体出力の保持は保証せず、必ず読み直してください。ユーザーの指示で再びONを適用した最終読取り（22:10:14）は全項目保持・読戻し一致で、CH1の正弦波も表示されています。

実装：[generator.rs](../crates/owon-core/src/generator.rs)、[GeneratorControls.tsx](../src/GeneratorControls.tsx)。負荷設定、任意波形アップロード、パルス幅・デューティのAPIはありません。2 kHz変更と0.3 UIでの同値周波数・SINE指定・ON/OFF操作、安全制限の実機確認は[README](../README.md)に要約しています。生記録はローカル保管（公開対象外）です。別波形、振幅、オフセットについてはコマンド生成・範囲制限を自動テストで確認した段階で、全実機設定値の成功を保証しません。

## PC側の記録とファイル操作

これらは本体設定を変更しません。保存先はPCのパス文字列です。保存は既存ファイルを上書きせず、失敗時には途中ファイルが残る可能性があります。

| API | 引数 | 成功戻り値 | 制限と意味 |
| --- | --- | --- | --- |
| `set_recording` | `{folder: string \| null, intervalMs: number}` | 新規セッションフォルダーのパス／停止時 `null` | 接続必須。開始時は既存フォルダー、整数500〜60000 ms。`folder: null`で停止し、処理完了を待つ。内部返信待ち10秒 |
| `list_recordings` | `{folder: string}` | `string[]` | 直下の通常ファイルで拡張子が小文字 `.json` のもの。最大10000件、パスの文字列順。再帰なし、シンボリックリンクを除外 |
| `load_record` | `{path: string}` | `Frame` | JSON最大5 MiB、形式と整合性を検証。UIはUSB切断を要求するが、IPC自体にその条件はない |
| `save_record` | `{path: string, record: ScreenRecord}` | `null` | 形式を検証しJSON保存、最大5 MiB。`Frame`全体ではなく `frame.record` を渡す |
| `export_table` | `{path, record, channel, format}` | `null` | `channel`: `both`／`CH1`／`CH2`、`format`: `csv`／`txt`。CSVまたはタブ区切り。全行、出力最大32 MiB |
| `export_image` | `{path: string, bytes: number[], format: string}` | `null` | `format`: `png`／`bmp`／`gif`。入力はPNG。デコードしてRGB画像に再符号化。入力5 MiB、4096×4096、デコード64 MiB、出力32 MiBまで |
| `save_xls` | `{path: string, bytes: number[]}` | `null` | BIFF8/OLEの先頭シグネチャを検査し、最大32 MiBを保存。XLS生成はフロントエンド。完全なExcelファイル検証ではない |
| `print_preview` | `{margins: number[], landscape: boolean}` | `null` | 余白は上・右・下・左の4値、各0〜40 mm。macOSはA4のネイティブ印刷ダイアログを予約。成功は印刷完了ではない |
| `export_csv` | `{path: string, record: ScreenRecord}` | `null` | 初版互換用の両CH raw CSV。新規コードはCH選択付き `export_table` を推奨 |
| `save_png` | `{path: string, bytes: number[]}` | `null` | 初版互換用。PNG先頭シグネチャと5 MiB上限のみ検査。新規コードはデコード検証付き `export_image` を推奨 |

`export_table` の `path`／`channel`／`format`は文字列、`record`は `ScreenRecord`。`bytes` の要素は0〜255の整数。拡張子から形式を自動判定しません。

自動保存は新しい `owon-<時刻>` フォルダーに `wave-000001.json` から作成します。保存専用スレッドが最新の未保存フレームを取り、表示停止・未更新をスキップします。全収録を連続記録する機能ではなく、Macのスリープ中は動きません。`Preview.recording` で進捗と停止理由を確認します。

印刷APIには波形引数がありません。フロントエンドが固定画像のプレビューDOMを用意してから呼びます。XLSの行数上限65536はフロントエンドのBIFF8生成時の制限です。Windowsの印刷設定反映は未検証です。

## USBと応答形式

本体System 2/2のUSB設定は、この実機で接続できている `HID`。名称にかかわらず現在の通信実装はHIDレポートではなく、USB記述子から見つけたBulk IN／OUTを `rusb`／libusbで使います。USB-C端子やケーブルの公称速度はプロトコル速度を決めません。今回の列挙は `Full`（USB Full-Speed）でした。

- 選択したbus/addressだけを開き、alternate setting 0の一意なBulk IN／OUT組を使います。エンドポイント番号を固定していません。
- アプリの照会はASCII大文字＋LF。テキスト応答はLFまたはCRLFで終端したものを受理します。
- `HEAD?`／`CH1?`／`CH2?`は4バイトlittle-endian長＋ペイロード。任意のペイロード内改行は区切りにしません。既知の末尾は空／LF／CRLF。
- 応答ペイロード上限1 MiB。照会の時間予算は約2秒。OSのUSB待機により実時間が超える場合があります。API全体の固定所要時間ではありません。
- 不完全受信・不正形式後は同じアプリ通信セッションを再利用しません。再接続が必要です。USBリセット、ドライバーの強制解除、USB構成変更は行いません。

実装：[usb.rs](../crates/owon-core/src/usb.rs)、[protocol.rs](../crates/owon-core/src/protocol.rs)。この内部IPCに任意SCPI文字列を通すAPIはありません。

## GEN OUT照会の実機結果

以下はUI実装前の読取専用診断CLIで確認した当時の設定です。0.3ではこの7照会を通常アプリの許可リストとGEN OUTの操作APIに追加しました。下表は現在値ではなく、保存した診断結果です。

| 照会 | 実機応答 | 解釈と確認範囲 |
| --- | --- | --- |
| `:FUNCTION?` | `SINe` | 正弦波の設定 |
| `:FUNCTION:FREQUENCY?` | `1.000000e+03` | 設定周波数1 kHz |
| `:FUNCTION:PERIOD?` | `1.000000e-03` | 設定周期1 ms |
| `:FUNCTION:AMPLITUDE?` | `1.000000e+00` | 設定振幅1 Vpp。端子の実測ではない |
| `:FUNCTION:OFFSET?` | `0.000000e+00` | 設定オフセット0 V |
| `:CHANNEL?` | `OFF` | 出力状態の照会応答。端子の電圧や負荷は実測していない |
| `:FUNCTION:LOAD?` | `INF` | 生応答をそのまま記録。公式PDFのON/OFF表記と異なるため、booleanに変換しない |

7照会とも完全応答、各約10.9〜11.6 ms。この単発結果は連続波形表示fpsのベンチマークではありません。出力をOFF→ONにしておらず、波形種・周波数・振幅・負荷を変更できることまでは未確認です。`INF` は高インピーダンス指定を示す可能性がありますが、ここでは意味の実機検証は未完了とします。

GEN OUTの生記録はローカル保管（公開対象外）です。応答時の本体はオシロスコープモードでした。

## トリガ照会の診断結果

### 完全応答を確認した構文

| 項目 | 完全応答した照会 | 実機応答 |
| --- | --- | --- |
| 信号源 | `:TRIGGER:SOURCE?` | `CH1` |
| 信号源の別階層 | `:TRIGGER:EDGE:SOURCE?`、`:TRIGGER:SINGLE:EDGE:SOURCE?` | いずれも `CH1` |
| 結合 | `:TRIGGER:COUPLING?` | `DC` |
| モード | `:TRIGGER:SWEEP?` | `AUTo` |
| エッジ | `:TRIGGER:SINGLE:EDGE?` | `RISe` |
| レベル | `:TRIGGER:SINGLE:EDGE:LEVEL?` | `0.00pV` |

成功応答はLF終端で、約7.3〜11.3 ms。`0.00pV` はヘッダーと照会の双方で観測した文字列です。これだけでトリガレベルの分解能がpVと確認できたわけではありません。0.4の照合パーサーはpV／nVも正規化します。

### 受信0バイトでタイムアウトした構文

| 試した照会 | 変更条件と結果 |
| --- | --- |
| `:TRIGGER:SINGLE:SOURCE?` | LF、CRLFの両方で無応答 |
| `:TRIGger:SINGle:SOURce?`、`:TRIG:SING:SOUR?` | 混合大小文字・省略形でも無応答 |
| `:TRIGGER:SINGLE:COUPLING?`、`:TRIGGER:SINGLE:SWEEP?` | 無応答 |
| `:TRIGGER:SINGLE:SLOPE?` | 無応答 |
| `:TRIGGER:SINGLE::EDGE?`、`:TRIGGER:SINGLE::EDGE:LEVEL?` | PDF内の二重コロン表記も無応答 |
| `:TRIGGER:SINGLE:EDGE:COUPLING?`、`:TRIGGER:SINGLE:EDGE:SWEEP?` | EDGE階層追加でも無応答 |
| `:TRIGGER:EDGE:COUPLING?` | 無応答 |
| `:TRIGGER:SINGLE:MODE?` | 公式PCソフトの書込み文字列から作った照会候補。無応答 |

合計20試行（信号源の表記違い・改行違いを含む）中、7件完全応答、13件は受信0バイトでUSB受信タイムアウト。各失敗は約2.1〜2.3秒でした。

失敗ごとにハンドルを解放し、次の新しいハンドルで `*IDN?`、`:TRIGGER:STATUS?`、`HEAD?` が通ることを確認してから次の候補を照会しています。候補を送った後の同じ失敗セッションに別の照会を送り続けてはいません。全試行前の設定ヘッダーと最終ヘッダーは一致しました。

この結果は「HDS25Sはトリガ照会全般に非対応」という判断を否定します。同じUSB接続で別階層の照会は通り、失敗した照会は文字・改行も受信していないため、今回のタイムアウトは単なる改行パーサー不具合ではありません。**HDS25S V12.1.0と採用したコマンド階層の不一致が主要因と考えられます。** 本体内での拒否理由や、全機種／全ファームウェアの共通構文は未確定です。

公式PDFの個別構文表と、公式PCソフトから抽出したコマンド文字列には `SINGLE:SOURCE` などがありましたが、それだけではこの実機での動作を保証しません。書込み文字列の存在も、今回確認した照会パスを使う書込みが成功する根拠にはなりません。

最初の11試行、階層比較の9試行、変更前の読取記録はローカル保管（公開対象外）です。0.4で応答済み階層を採用しました。

## エラーと安全な復帰

| 状況 | 現在の主な通知 | 呼出し側の対応 |
| --- | --- | --- |
| 未接続 | `未接続です` | 列挙・選択・接続後に呼ぶ |
| 二重接続 | `すでに接続しています` | 既存接続を使う。診断前は切断する |
| 値・操作が不正 | `設定値が不正です`／`未対応の本体設定です`／許可範囲外 | 呼出しを修正。自由生成SCPIで回避しない |
| USB排他・権限・未検出 | `USBを開けません…`／`USBを使用できません…`／`USBデバイスがありません` | 他アプリ・ケーブル・本体USB設定・権限を確認して再列挙 |
| USBタイムアウト・応答不正 | USB受信エラー、`Preview.error`等 | 自動再試行しない。PC接続を切断し、必要に応じ抜き差しして再接続 |
| 書込み後の不一致 | 成功戻り値の `verified: false` | 本体表示と読戻しを確認。自動上書き／巻戻しなし |
| STOP条件不足 | 本体での停止／両CH ONを求めるエラー | 本体を手動操作してから取得。APIは本体を停止しない |
| 保存先あり／容量不足 | 上書きしない旨、OSエラー、`recording.error` | 新規ファイル名・空き容量を確認。途中ファイルがあれば内容を確認 |

文字列は正式な固定エラーコードではなく、OS由来の文言も含みます。メッセージの完全一致だけで恒久的な分岐を作らないでください。アプリの返信待ちが切れても、投入済み操作は取り消されないため、特に書込みを繰り返さないでください。

## 読取専用診断の再現方法

専用CLI：[readonly_probe.rs](../crates/owon-core/src/bin/readonly_probe.rs)。自由入力SCPIは受け付けず、以下3組の固定照会だけを実行します。接続された対象OWONがちょうど1台でなければ中止します。**アプリを切断してから実行し、終了後にアプリを再接続してください。**

```sh
# appディレクトリで実行。出力先フォルダーを作り、ファイル名は新規にする。
mkdir -p ../research/hardware
cargo run --manifest-path crates/owon-core/Cargo.toml --bin readonly_probe -- generator ../research/hardware/generator-new.json
cargo run --manifest-path crates/owon-core/Cargo.toml --bin readonly_probe -- trigger ../research/hardware/trigger-new.json
cargo run --manifest-path crates/owon-core/Cargo.toml --bin readonly_probe -- trigger-paths ../research/hardware/trigger-paths-new.json
```

各組で基準照会が失敗した場合は中止します。候補照会の無応答は結果として記録して継続するため、終了コード0だけでは全照会成功を意味しません。`trials[].observation.complete` と `aborted` を確認してください。新しいハンドルの取得自体に失敗した場合などは、報告JSONを保存する前に終了することがあります。出力先の重複検査は保存時なので、新規パスを指定してください。

報告JSONには送信hex、受信hex、改行形式、受信チャンク数／時刻、完全応答の有無、エラー、基準の本体設定、最終設定を残します。診断用のUSB生送信は固定照会のみで、通常アプリの許可リストを拡張しません。USBリセット・出力ON・設定書込み・ドライバー変更はありません。

今回のUSBなしテストは、既存core 26件＋診断CLI 3件が成功しました。診断後にアプリを再接続し、ライブ波形取得と本体12項目の測定読取りが成功しています。これは長時間安定性や全書込みAPIの実機保証ではありません。

## 根拠資料と今後の実装境界

- [OWON公式 HDS200 SCPI資料](https://files.owon.com.cn/software/Application/HDS200_Series_SCPI_Protocol.pdf)。構文・単位候補の参考にし、実機結果と区別しています。第三者配布のPDFはリポジトリに含めません。
- [OWON公式PCソフト配布](https://files.owon.com.cn/software/pc/HDS200_series_pc_software.zip)。調査対象JARは `com.owon.uppersoft.hds_1.2.11.1.v20240927.jar`。文字列の静的調査のみで、Windowsアプリによる操作検証ではありません。
- 公式機能調査・過去の検証の要約は[README](../README.md)を参照してください。調査用の生記録はローカル保管です。

GEN OUTは0.3で読取り・設定APIとUIを実装しました。波形種・振幅・オフセットの変更範囲、負荷の意味、端子の実電圧は追加検証が必要です。SINE指定の副作用を検出した後、明示的に出力OFFへ戻しました。その後ユーザーの指示で出力ONにし、全項目の保持とCH1の正弦波表示を確認しました。最終読取り時点の設定は2 kHz／1 Vpp／0 V／INF／ONです。

トリガーは0.4で成功した照会階層を採用し、専用の読取りと競合検出付き書込みAPIを追加しました。旧非応答パスは送信しません。0.4.1のAutoは現在値読取りだけを使用できます。Auto実行、RUN/STOP、任意SCPI、DMM、校正済みΔt、自動検出・統計、外部AIへの送信は使用できません。
