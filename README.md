# OWON Scope

OWON HDS25S（HDS200シリーズ）のUSB波形取得・表示・本体操作を行うデスクトップアプリ。
**Tauri 2 + React/TypeScript + Rust、バージョン0.4.1** です。
macOS / Apple Siliconで実機確認しています。Windowsビルド・USB接続は未検証です。
OWON公式アプリではなく、完全互換版でもありません。

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
ライブの両CHは逐次照会であり、同一収録であることは未検証です。

## ビルドと起動

このリポジトリにはTauri版のソースコードを収録しています。
Rust 1.90以上、Node.js、macOSではXcode Command Line Toolsを準備し、次を実行します。

```sh
git clone https://github.com/afjk/owon_hds25s.git
cd owon_hds25s/tauri-app
npm ci
npm run tauri -- dev
# 配布用アプリをローカルでビルドする場合
npm run tauri -- build
```

Macではビルド後にプロジェクト直下の **OWON Scope.command** から起動できます。
詳しい操作・安全制限は [アプリREADME](tauri-app/README.md)、
本体操作APIは [API仕様書](tauri-app/docs/device-api.md) を参照してください。

## 本体の接続

1. HDS25Sをオシロスコープモードで起動します。
2. 本体の **System** → **F4（1/2）** で **2/2** ページへ移動し、
   **F1（USB）** で **HID** を選択します。位置はファームウェアにより異なる可能性があります。
3. データ通信対応のUSB-CケーブルでPCへ接続します。
4. 他の接続アプリを切断し、アプリで「再検索」→ 対象機器を選択 →「接続」を押します。

HDS25S / V12.1.0でVID:PID `5345:1234`、Bulk OUT `0x01`・IN `0x81`、
両CH 600バイトの画面波形応答を確認しています。
本体の設定名はHIDですが、この実機はBulk転送です。シリアルポートの出現を前提にしません。
アプリは本体のUSBリセットやドライバーの強制解除を行いません。

## 検証

```sh
cd tauri-app
npm test
npm run build
cargo test --manifest-path crates/owon-core/Cargo.toml --locked
cargo test --manifest-path src-tauri/Cargo.toml --release --locked
```

USBなしのテストはRust core 58件、診断CLI 6件、native Rust 4件、TypeScript 20件の計88件です。
実機の取得データ・機器固有の一時診断ツール・バックアップ・第三者配布物は公開対象外です。
テストには実機の波形やシリアル番号を含まない合成データを使います。
テストの合格は実機接続・測定精度・長時間安定性を保証するものではありません。

## 参照資料

- [OWON公式HDS200製品情報](https://www.owon.com.hk/products_owon_hds200_series_digital_oscilloscope)
- [OWON公式HDS200ユーザーマニュアル](https://files.owon.com.cn/probook/HDS200_series_user_manual.pdf)
- [OWON公式SCPI資料](https://files.owon.com.cn/software/Application/HDS200_Series_SCPI_Protocol.pdf)
- [Tauri前提環境](https://v2.tauri.app/start/prerequisites/)
- [libusb公式macOS FAQ](https://github.com/libusb/libusb/wiki/FAQ)
