# OWON Scope

OWON HDS25S（HDS200シリーズ）のUSB波形取得・表示・本体操作を行うデスクトップアプリ。
現行版は **Tauri 2 + React/TypeScript + Rust、バージョン0.4.1** です。
macOS / Apple Siliconで実機確認しています。Windows向けの構成も用意していますが、
Windowsビルド・USB接続は未検証です。OWON公式アプリではなく、完全互換版でもありません。

CH1／CH2の用途やセンサー種別は固定しません。

## 現在の主な機能と制限

- USBデバイス検索・接続、2CHライブ波形、表示停止、rawカーソル、XY、演算、FFT。
- JSON保存・連続記録・再生、CSV／TXT／XLS表出力、PNG／BMP／GIF画像出力、印刷プレビュー。
- 本体設定・測定値の読取り、GEN OUT操作、トリガ源・結合・エッジ・モード・レベルの操作。
- 日本語／英語UI。設定変更は明示操作と読戻し確認を経由します。
- Auto設定は読取り・確認UIのみで、実機検証待ちのため実行は無効です。
- 波形軸は受信位置・signed byte値です。校正済みの時間／電圧軸、自動Δt計測、
  イベント駆動の自動処理、自然言語AI操作は未実装です。

表示は実機で約30回/秒を確認しましたが、これはPCへの表示更新数であり、
本体の独立した新規収録数や測定精度を保証するものではありません。

## アプリの起動と操作

### Tauri版 0.4.1（現行）

このリポジトリにはソースコードを収録しています。ビルド済みアプリは含みません。
Rust 1.90以上、Node.js、macOSではXcode Command Line Toolsを準備し、次を実行します。

```sh
git clone https://github.com/afjk/owon_hds25s.git
cd owon_hds25s/tauri-app
npm ci
npm run tauri -- dev
# 配布用アプリをローカルでビルドする場合
npm run tauri -- build
```

ビルド後は、プロジェクト直下の **OWON Scope.command** から起動できます。
本体はオシロスコープモード・USB HID設定にして接続してください。
詳しい操作・安全制限は [Tauri版README](tauri-app/README.md)、
本体操作APIは [API仕様書](tauri-app/docs/device-api.md) を参照してください。

実機の取得データ・機器固有の一時診断ツール・バックアップ・第三者配布物は公開対象外です。
テストには実機の波形やシリアル番号を含まない合成データを使います。

### Python版（初期プロトタイプ）

以下は初期Python版の説明です。光／音の入力割当てはこの版の機能であり、
現行Tauri版は汎用のCH1／CH2表示です。

Finderで `OWON HDS25S.command` をダブルクリックするか、ターミナルで実行する。

```sh
.venv/bin/python owon_viewer.py
```

PySide6-Essentials + pyqtgraphによるデスクトップアプリ。ネットワーク接続やクラウド送信は不要。
USB取得は専用スレッドで直列実行し、表示操作と分離している。起動時に自動接続する。
高速プレビューでは波形を毎回取得し、本体状態・ヘッダーは約1秒ごとに更新する。
設定変更の表示にはその照会間隔分の遅れがある。表示再開時には改めて設定情報を取得する。
GUI通知は最大1件だけ待機させ、表示が詰まった場合は最新フレームへ更新する。
停止保存はこのキャッシュを使わず、取得前後の状態・設定を改めて照会・検証する。

- **接続／切断**：HDS25Sを1台だけ接続する。エラー時は本体のモード・ケーブルを確認して再接続する。
- **表示を一時停止／表示再開**：Macのプレビュー照会・更新を止める。本体のRUN/STOPや収録状態は変えない。
- **停止波形を保存**：本体でCH1/CH2をONにし、RUN/STOPで停止してから実行する。取得前後のSTOPとヘッダーの一致を確認し、両CHの元データ・ヘッダー・入力割り当てをJSON保存する。取得後はMacの表示を一時停止する。
- **保存データを開く**：USBを切断した状態で保存済みJSONを再表示する。実機は不要。
- **表示をリセット**：横軸を受信データ全体、縦軸を−128〜127に戻す。ドラッグやホイールで表示範囲を調整できる。
- **入力の割り当て**：CH1/CH2の光・音の役割を切り替える。GUIから保存したデータには割り当ても記録する。

縦軸は受信バイトをsigned int8として解釈した「画面値」。横軸はバイトの受信位置で、秒やADCサンプル番号ではない。
本体のV/div・時間軸・ADC rateは参考情報として表示するが、グラフの軸への変換には使用していない。
ライブはヘッダーと各CHの逐次照会であり、CH間の収録一致は未検証。Δt測定用データとは区別する。

他の環境でGUIを準備するには：

```sh
brew install libusb
python3 -m venv .venv
.venv/bin/python -m pip install -r requirements-gui.txt
chmod +x 'OWON HDS25S.command'
```

今回の環境ではuvを使い、プロジェクト内の `.venv` に導入済み。

```sh
uv pip install --python .venv/bin/python -r requirements-gui.txt
```

## 最初の接続確認

1. HDS25Sをオシロスコープモードで起動する。
2. 本体の **System** ボタンを押し、**F4（1/2）** で **2/2** ページへ移動する。
   左端の **USB** 項目に対応する **F1** を押し、表示を **HID** にする。
   HDS25S実機の画面例でUSBが2/2ページにあることを確認した。
   メニュー位置はファームウェアで異なる可能性がある。MSCはファイル転送用。
3. データ通信対応のUSB-CケーブルでMacに接続する。
4. 次を実行する。

```sh
.venv/bin/python owon_probe.py devices
.venv/bin/python owon_probe.py probe
```

`devices` はUSB記述子と転送方式、`probe` は `*IDN?`・トリガ状態・画面波形ヘッダーを表示する。
2026-10-03の実機確認でUSB認識・SCPI応答・CH1/CH2の画面波形取得に成功した。
libusb 1.0.30、PyUSB 1.3.1を使用。

| 実機確認項目 | 結果 |
| --- | --- |
| 機種 / ファームウェア | HDS25S / V12.1.0 |
| ヘッダーのMODEL | `HDS2-25-2S_1` |
| USB ID / インターフェース | `5345:1234` / class `0x05`、interface 0 |
| 転送 | Bulk OUT `0x01`、IN `0x81`、最大パケット64バイト |
| 画面波形の応答 | CH1・CH2とも600バイト、4バイトlittle-endian長さの解析成功 |
| 取得速度 | ヘッダー＋両CHを12回取得し全回成功。中央値45.28 ms、最小42.92 ms、最大57.68 ms |
| 測定時の本体状態 | AUTO、1 ms/div、4K、ヘッダーのADCサンプルレート250 kSa/s |

速度測定は短時間のプレビュー取得であり、長時間の安定動作・別収録の更新数・
両CHが同じ収録に属することは未検証。
初版GUIで2,600回以上の連続取得と約9.7回/秒の更新を目視確認した（STOP照会を加えた4照会/回）。
これは取得ループの回数であり、別収録の更新数やADCサンプルレートではない。

高速化前後の追加確認（同じ実機・元のケーブル）：

| 測定 | 結果 |
| --- | --- |
| 本体状態＋ヘッダー＋2CHを40回、待ち時間なし | 平均65.17 ms/回、15.34回/秒 |
| 2CHの波形のみを40回、待ち時間なし | 平均32.55 ms/回、30.73回/秒 |
| 同じ実データによるオフスクリーン表示処理120回 | 中央値6.77 ms（実画面のFPS測定ではない） |
| 高速化後の実画面 | 約29.7回/秒、USB照会32 ms、取得後の表示待ち約1 ms |

旧版の100 ms周期制限を解除し、現在は周期の下限を1/60秒としてUSB応答速度に追従する。
設定情報は約1秒ごと、波形以外のUI文字情報は最大4回/秒に更新を抑えている。
右側の「波形更新」はGUIへ適用したフレーム数であり、ディスプレイの物理リフレッシュレートや
本体の独立した新規収録数を示すものではない。補間した仮の波形は表示しない。

Codexの制限付き実行ではPyUSBの一覧が空になる一方、macOSのIORegistryには
実機が見える場合がある。今回、制限の外で実行すると認識・通信できた。
通常のターミナルで上記コマンドを実行して切り分ける。空の一覧だけでケーブル不良と判断しない。

他の環境での準備：

```sh
brew install libusb
python3 -m venv .venv
.venv/bin/python -m pip install -r requirements.txt
```

既存のPython環境を変更せず、プロジェクト内の仮想環境を使う。

## 停止した2チャンネルの受信データを保存

本体でCH1とCH2をONにし、波形を収録してからRUN/STOPで停止する。
停止を維持したまま次を実行する。

```sh
.venv/bin/python owon_probe.py capture
```

`captures/capture-日時.json` にヘッダーと両チャンネルの受信バイト列（hex）を保存する。
取得前後のSTOP状態とヘッダーが一致することを確認するが、フレームIDによる厳密な
同一収録の証明はまだできない。取得中に本体を操作しないこと。
サンプル形式未確定のため、バイト数をサンプル点数として扱わない。
保存時刻はMacで取得した時刻であり、光や音が発生した時刻ではない。

`--output captures/example.json` で保存先を指定できる。既存ファイルは上書きしない。
タイムアウト時は応答の途中から再解釈せず、コマンドを終了する。
再試行でも失敗する場合はUSBを再接続する。

## 通信の調査結果

| 項目 | 確認できたこと／残る確認 |
| --- | --- |
| USBモード | 公式HDS200マニュアルはPC通信にHIDモードを指定 |
| 実際のUSB転送 | HDS25S実機でVID `0x5345`、PID `0x1234`、Bulk OUT `0x01`、IN `0x81`を確認 |
| macOS | PyUSB + libusbで通信成功。本体の設定名はHIDだが、この実機のUSB interface classは `0x05` でBulk転送 |
| シリアルポート | LinuxのUSB serial実装をそのままmacOSに当てはめない。`/dev/cu.*` の出現を前提にしない |
| 波形コマンド | `:DATA:WAVE:SCREEN:HEAD?`、`:DATA:WAVE:SCREEN:CH1?`、`:DATA:WAVE:SCREEN:CH2?` |
| 応答 | 上記データコマンドの4バイトlittle-endian長さプレフィックスを実機で確認 |
| 波形の意味 | コマンド名・公式記述は「画面波形」。本体の8Kメモリ全体が取得できるとはまだ判断しない |
| データ形式 | 実機の600バイトをsigned int8として読んだ値は、無入力のCH1で49〜51、CH2で−51〜−49。ヘッダーのOFFSET ±50と整合する。ペアの意味と時間間隔は既知信号で検証する |
| 更新頻度 | ヘッダー＋2CHの短時間測定は中央値約45 ms/回。本体の波形更新速度とMacへの取得速度は別 |

ツールは既知の照会コマンドだけを送る。USB記述子からBulk IN/OUTの組を選び、
本体のリセット、ドライバーの強制解除、設定変更は行わない。
HDS25SのVID/PIDが違う場合は `devices` の結果に合わせて `--vid 0x.... --pid 0x....` を指定する。
複数台の場合は `--address` も指定する。Bulk以外の実機にはまだ対応していない。

## Δt計測の設計案

USBやMacの処理遅延をΔtに混ぜず、同じ収録内の共通時間軸上で開始位置を比較する。
ライブ表示時にCH1とCH2を別々の更新から読んでしまう可能性を検証し、計測用には
停止した収録を使用する。通信仕様と本体の停止・シングル収録の挙動が確認できたら自動化する。

- 光（現在CH2）：背景光の基準を取り、発光による立ち上がりを検出。しきい値、ヒステリシス、持続時間を調整可能にする。
- 音（現在CH1）：静音時のDC基準からの偏差または包絡線で音の開始を検出。MAX4466モジュールにはVCC/2付近のDCバイアスがある。
- Δt：音−光の符号付きms表示、検出点のカーソル表示、手動修正、元波形と条件の保存。
- 精度：画面データの時間間隔を実測する。ヘッダーのADCサンプルレートの逆数を画面データの点間隔として使わない。
- 補正：スピーカー〜マイクの距離、センサー・アンプの応答、検出しきい値や平滑化の遅延を区別する。

初版UIは上部に接続・保存操作、中央に横軸を連動させたCH1/CH2、右側に入力割り当て・
取得情報・Δtの未校正表示を配置した。検出条件、検出点カーソル、計測履歴は今後追加する。

必要な測定範囲と目標精度は未決定。センサーのモジュール構成・電源・配線も実物に合わせて確認する。

## 検証

```sh
.venv/bin/python -m unittest -v
```

23件の実機を使わないテストで、USB受信の分割、little-endianの長さ、バイナリ中の改行、
不完全な応答、停止状態の変更、設定コマンドの拒否、元データの保存・読み込み・上書き防止、
GUIの表示停止・再開・再接続・入力割り当て・通信エラー、設定情報キャッシュ・最新フレームの通知集約を検証する。
GUIテストはオフスクリーンで実行する。GUI依存パッケージ未導入の場合はその7件をスキップする。
停止保存のGUI経路は模擬機器で確認済み。実機の停止保存・時間軸校正は次の確認項目。
これらの合格は実機接続や測定精度を証明するものではない。

## 参照資料

- [OWON公式HDS200製品情報](https://www.owon.com.hk/products_owon_hds200_series_digital_oscilloscope)
- [OWON公式HDS200ユーザーマニュアル](https://files.owon.com.cn/probook/HDS200_series_user_manual.pdf)
- [OWON公式SCPI資料](https://files.owon.com.cn/software/Application/HDS200_Series_SCPI_Protocol.pdf)
- [OWON作成SCPI PDFの公開ミラー（今回の確認に使用）](https://github.com/qwindelzorf/owon_hds200/blob/main/docs/HDS200_Series_SCPI_Protocol.pdf)
- [HDS272Sで検証されたUSB通信実装](https://github.com/linux4life798/owon-hds200-capture/blob/main/owon_usb_scpi.py)
- [同実装の応答フレーミング](https://github.com/linux4life798/owon-hds200-capture/blob/main/owon_scpi_base.py)
- [PyUSB公式ドキュメント](https://github.com/pyusb/pyusb/blob/master/docs/tutorial.rst)
- [libusb公式macOS FAQ](https://github.com/libusb/libusb/wiki/FAQ)
- [MAX4466モジュールのDCバイアス（Adafruit）](https://learn.adafruit.com/adafruit-microphone-amplifier-breakout/assembly-and-wiring)
- [HDS25S実機のSystem 2/2・USB設定画面](https://kloeckner.com.ar/blog/owon-hds25s-bode-plot-using-python-and-scpi-commands/)
