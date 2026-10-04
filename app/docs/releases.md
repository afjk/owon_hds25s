# Mac／Windows向けリリース

[Release builds](../../.github/workflows/release.yml) がGitHub Releaseの公開時にビルドします。
通常のpush、タグのpush、Releaseの下書き保存だけでは起動しません。
3種類のビルドとテストがすべて成功した後、公開したReleaseへ添付します。
Releaseのタイトルや本文は変更せず、別のReleaseも作成しません。
公開後に添付する方式なので、Release immutability（リリースの不変性）を有効にすると
添付できなくなります。現在このリポジトリでは無効です。有効化する場合は、
公開前に下書きへ成果物を添付する方式へworkflowを変更してください。

## 添付されるファイル

- Apple Silicon Mac：`OWON-Scope_<version>_macos_aarch64.dmg`
- Intel Mac：`OWON-Scope_<version>_macos_x64.dmg`
- Windows x64：`OWON-Scope_<version>_windows_x64_setup.exe`（NSISインストーラー）

各ファイルに対応する`.sha256`も添付します。Windows ARM64版、MSI、Linux版は対象外です。
WebView2が未導入のWindowsでは、インストーラーが標準のWebView2導入処理を行うため
インターネット接続が必要になる場合があります。

## Releaseを公開する手順

1. リリースするコミットを`main`へpushします。最初のReleaseでは、このworkflowを含むコミットを選びます。
2. アプリのバージョンが一致していることを確認します。
   `app/package.json`、`package-lock.json`（先頭とルートpackage）、
   `src-tauri/tauri.conf.json`、`src-tauri/Cargo.toml`が対象です。
   次回バージョンを変更するときは、`src-tauri/Cargo.lock`のアプリ自身のバージョンも更新します。
3. GitHubの **Releases → Draft a new release** を開きます。
4. **`v<version>`** のタグを選ぶか、新規作成します。現在なら`v0.4.1`です。
   タグの対象コミットが、このworkflowと同じバージョンのソースを含むことを確認します。
5. タイトル・変更点・配布上の注意を書き、**Publish release** を押します。
6. **Actions → Release builds** で実行結果を確認します。初回は依存関係のビルドに時間がかかります。
   全ビルド成功後、ReleaseのAssetsにインストーラーとチェックサムが追加されます。

タグとアプリのバージョンが違う場合は、ビルド前に失敗します。
Pre-releaseの公開も対象です。下書きのままでは実行しません。
失敗時は修正したバージョンを新しいタグで公開するか、同じコミットの一時的な失敗なら
Actionsの **Re-run failed jobs** で再実行します。
同一Releaseへの再実行は、このworkflowが作った同名の添付ファイルを置き換えます。

## 公開せずにビルドを試す

**Actions → Release builds → Run workflow** でブランチを選択します。
Mac／Windowsのテストとビルドだけを実行し、Release・タグは作成しません。
成功した成果物は各実行画面の **Artifacts** からダウンロードできます。保持期間は14日です。
ArtifactsはZIPですが、中身は同じDMG／EXEとチェックサムです。

ローカルでリリース用スクリプトを検証するには、リポジトリ直下で次を実行します。

```sh
node .github/scripts/release.mjs check-version
node --test .github/scripts/release.test.mjs
```

## 署名・実機検証の範囲

Mac版はad-hoc署名（`signingIdentity: "-"`）のみです。
Developer ID署名・Apple公証は行っていないため、通常の公証済みアプリと同じ扱いにはなりません。
Windows版もコード署名証明書を使っておらず、SmartScreen等の警告が出る場合があります。
利用者にこの制限をRelease本文で案内してください。署名・公証用のSecretは現時点では不要です。

ビルド成功はUSB接続や測定精度の保証ではありません。WindowsのUSBドライバー適合、
Windows／Intel Macでの実機接続・印刷は別途検証が必要です。
インストーラーからUSBドライバーを自動置換する処理は追加していません。

workflowはNode.js 22・Rust 1.91.1とlockfileを使い、利用するActionsはコミットSHAに固定しています。
ビルドjobは読取権限だけを持ち、Releaseへの書込権限は添付jobだけに付与しています。

## 初回検証

2026-10-04、ソースコミット`e26f175`に対する[手動実行](https://github.com/afjk/owon_hds25s/actions/runs/37161019518)で
MacのApple Silicon／IntelとWindows x64の3jobがすべて成功しました。
各環境の自動テスト、Macのad-hoc署名検証、DMG／NSISインストーラーの生成を確認しています。
3種類の成果物をダウンロードし、チェックサムと添付用の6ファイル構成も検証しました。
手動検証なのでRelease添付jobは意図どおりスキップされ、Release・タグは作成していません。
公開Releaseへの実際の添付は、最初のRelease公開時に確認する項目です。

## 一次資料

- [TauriのGitHub Actionsガイド](https://v2.tauri.app/distribute/pipelines/github/)
- [Tauri GitHub Action](https://github.com/tauri-apps/tauri-action)
- [GitHub Releaseイベント](https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows#release)
- [GitHubのImmutable releases](https://docs.github.com/en/code-security/concepts/supply-chain-security/immutable-releases)
- [TauriのMac署名・公証](https://v2.tauri.app/distribute/sign/macos/)
- [TauriのWindows署名](https://v2.tauri.app/distribute/sign/windows/)
- [TauriのWindowsインストーラー・WebView2](https://v2.tauri.app/distribute/windows-installer/)
