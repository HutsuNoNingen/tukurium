# テスター向け差分更新

通常の配布ZIPとは別に、テスター用ランチャーを一度渡し、その後は起動時に変更されたファイルだけ取得する。ローカルの `scripts/publish-tester.ps1` が配信の正本で、将来Actionsから実行する場合も同じスクリプトを呼ぶ。

このリポジトリで「リリースして」とだけ依頼されたときは、テスター向けWindows版の変更をコミットし、新しいGitタグを作ってpushし、R2へ差分をアップロードして `latest.json` を切り替えるところまでを一回の作業として扱う。本番公開や別の配布先が指定された場合は、その指定に従う。エージェント向けの作業規則は [AGENTS.md](../AGENTS.md) に置く。

## 配信元を用意する

テスター配信先として設定したのはR2バケット `kokoverse-proto` の `tester/`。公開URLは `https://pub-0f9c05b145564ec593d063ff8bb35919.r2.dev/tester`。`r2.dev` は開発用のレート制限があるため、配布規模が増えたら独自ドメインへ切り替える。独自ドメインでキャッシュ規則を設定する場合、`latest.json` はキャッシュせず、版付きマニフェストとハッシュ付きオブジェクトだけを長くキャッシュする。

ローカルの書き込み認証情報は `%APPDATA%\rclone\rclone.conf` に置く。`r2` リモートは `kokoverse-proto` バケットだけに Object Read & Write 権限を持つ。権限の期限は2027年10月24日。認証情報をリポジトリ、配布物、Actionsログに入れない。rcloneの設定方法は [Cloudflareの手順](https://developers.cloudflare.com/r2/get-started/cli/) を参照する。

このPCではGitに無視される `.run/tester-publish.local.psd1` に `Remote`、`PublicBaseUrl`、`RcloneConfig` を置く。別のPCやActionsでは同じ項目を引数で渡すか、`KOKOVERSE_TESTER_REMOTE`、`KOKOVERSE_TESTER_PUBLIC_BASE_URL`、`RCLONE_CONFIG` の環境変数を使う。明示した引数、ローカル設定、環境変数の順に採用する。書き込み鍵そのものはこの設定ファイルへ入れない。

## ローカルから公開する

配布する内容を先にコミットする。Gitタグはコミットだけを指すため、公開スクリプトは未コミット・未追跡の変更があると止まる。

```powershell
./scripts/publish-tester.ps1 -Tag tester-v2026.10.24.1 -CreateTag -PushTag
```

スクリプトはGitの作業ツリーが空で、タグがHEADを指すことを確認する。R2への接続確認後、現在の作業ツリーからWindows配布版をビルド・検証し、版名付きZIPを `dist/` に残す。各ファイルをSHA-256名のgzipオブジェクトへ変換し、R2へまだないものだけをアップロードする。版のマニフェストを置いて公開URLから照合した後、最後に `latest.json` を更新する。既存の版のマニフェストは上書きしない。同じコマンドを再実行する場合は `-PackageRoot` に前回表示された `PACKAGE_ROOT` を渡すと、同じ梱包結果を再利用できる。

公開の順序は次のとおり。

1. 変更をコミットし、[Windows配布版のビルド手順](windows-build.md)に沿って配布する機能を確認する。既存のワールドは梱包しない。
2. 上のコマンドでタグを作成・pushし、配布版と小さなランチャーZIPを作る。
3. 変更されたハッシュのオブジェクト、版名付きマニフェストの順にR2へ上げる。公開URLからマニフェストを照合する。
4. 最後に `latest.json` を更新する。この時点からランチャーが新版を取得する。ランチャーZIPは初回だけテスターへ渡す。
5. 公開URLの `latest.json` が新しい版とマニフェストのハッシュを指すことを確認し、コミットID、タグ、公開URL、ランチャーZIPを記録する。

公開の途中で失敗した場合は `latest.json` が指す版を変えない。原因を直し、同じビルド成果物を `-PackageRoot` で指定して再実行する。タグとビルド内容を変える場合は新しいタグを使う。

同じタグを別のPCやActionsで再ビルドして、別内容の配布物として公開しない。ビルドし直す場合は新しいタグを使う。Actionsから配るときも、このスクリプトを一つの公開経路として実行する。

`dist/Kokoverse-Tester-Launcher-<tag>.zip` が初回にテスターへ渡す小さなZIP。`config.json` には公開URLだけが入り、R2の書き込み鍵は入らない。R2へのアップロードをせずに配信データだけ確認する場合は `-PrepareOnly` を付ける。この場合 `-Remote` と `-PublicBaseUrl` は不要。

## ランチャーの動作

テスターはランチャーZIPを展開して `Start.bat` を使う。ランチャーは `latest.json` と版のマニフェストを取得し、SHA-256が一致するファイルを現在の版からコピーする。不足するファイルだけをダウンロードして展開し、全ファイルを照合してから `current.json` を切り替える。起動中のEngineやGodotがあれば更新を次回に延ばす。取得に失敗した場合は既存の版を起動する。

```text
Kokoverse-Tester-Launcher-<tag>/
  Start.bat
  Stop.bat
  updater.ps1
  config.json
  current.json                 # 初回起動後
  versions/<版名>/             # アプリ本体。旧版を残す
  data/.run/                   # ワールド、素材、ログ。版の外に置く
```

公開サーバー側の構成は次のとおり。`blobs/` は内容のハッシュで命名するため、同じファイルを複数の版から参照できる。

```text
latest.json
manifests/Kokoverse-Windows-x64-<tag>.json
blobs/<SHA-256>.gz
```

従来の配布フォルダーの `.run/` は自動で消去・移動しない。既存ワールドを引き継ぐ場合は、旧版を終了してから、その `.run/` をランチャーの `data/.run/` へコピーする。既存の `data/.run/` がある場合は上書きせず、ワールドと素材を確認して移す。本人情報は従来どおり `%LOCALAPPDATA%/Kokoverse/` に置く。

ランチャー自身はアプリのマニフェストに含めない。ランチャーの更新が必要になった場合は、新しいランチャーZIPを渡す。

## ローカル検証

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File ./tests/tester-updater-smoke.ps1
```

このテストはローカルHTTPを使い、初回取得、変更がないファイルの再利用、ワールドの保持、失敗した更新からの復帰を確認する。実際にR2へ公開する前には、`dist/` の通常ZIPも [Windows配布版のビルド手順](windows-build.md)に従って確認する。
