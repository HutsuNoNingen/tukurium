# Windows配布版は作業ツリーからビルドする

対象は、KokoverseのWindows x64版をビルド・配布する開発者。
テスター向けのタグ付き差分配信は [テスター向け差分更新](tester-updates.md) を参照する。
PowerShellを使い、リポジトリのルートで実行する。通常の配布には **`scripts/package.ps1`** を使う。
このスクリプトは、現在の作業ツリーから実行ファイル・Godotクライアント・起動スクリプトをまとめる。

確認日：2026-10-24。確認済みの配布版：`Kokoverse-Windows-x64-20261024-024811`。
通常起動は、ハンドルネーム（HN）の入力からセッション作成・参加へ進む管理API版。固定ルーム版は `StartLegacy.bat` に残している。

毎回「ビルド元」を確認する。初回は続けて「環境」を準備し、セットアップ済みなら「ZIPと展開済み版を作る」へ進む。

## ビルド元は前回配布した機能を含むものにする

ビルド前に、作業中の変更と前回の `build-info.json` を確認する。
例えば、直前に配布した版が `024811` なら `dist/Kokoverse-Windows-x64-20261024-024811/build-info.json` を使う。以後は直前に配布した版へ読み替える。

```powershell
Set-Location C:\Users\neo\GitHub\kokoverse-proto
git status --short
git log -1 --oneline
```

2026-10-24時点では、管理API・ライブラリ・起動処理の一部が未コミットでも、すでに配布版に含まれている。
`git archive HEAD` だけから作ると、その機能が欠落する。実際に `023500` でログ窓・セッション管理・ライブラリが抜け、`024811` で戻した。
未コミットという理由だけで変更を除外したり、ビルド前に作業ツリーを戻したりしない。

通常の管理API版には、少なくとも次のファイルが必要になる。
新しいチェックアウトで不足している場合は、対応する変更を取り込んでからビルドする。

```powershell
$required = @(
    'crates/asset-server/Cargo.toml',
    'crates/engine/src/console.rs',
    'crates/engine/src/management.rs',
    'client-godot/service_panel.gd',
    'scripts/start-local.ps1',
    'scripts/start-online.ps1',
    'scripts/start-instance.ps1',
    'scripts/stop-local.ps1',
    'scripts/stop-online.ps1',
    'scripts/process-lifecycle.ps1',
    'scripts/connect-relay.ps1',
    'tests/managed-launcher-smoke.ps1'
)
foreach ($file in $required) {
    if (-not (Test-Path -LiteralPath $file)) { throw "Missing: $file" }
}
```

`distribution/Start.bat` が `scripts/start-instance.ps1` を経由して `scripts/start-online.ps1` を呼ぶことを確認する。`distribution/connection.json` の接続先は次のとおり。
`session: workshop` は従来版向けの設定で、通常起動を固定ルームへ戻す指定ではない。

| 用途 | 現在の接続先 |
|---|---|
| 管理API | `https://45.77.130.146/api` |
| KokoSync中継 | `wss://45.77.130.146/ws` |

別作業の変更が混在している場合は、配布に必要な変更と含めない変更を確認する。
コミットする範囲と、今回の配布に含める範囲を混同しない。

## 初回だけ開発環境を用意する

Windows x64で以下を実行する。ダウンロードにはインターネット接続が必要。

```powershell
./scripts/setup.ps1
. ./scripts/env.ps1
```

セットアップは `.tools/` にRust、GCC、Go、Godot、動画変換用FFmpegを配置する。
Rustの操作をするPowerShellでは、毎回 `env.ps1` を読み込む。

| ツール | 現在の指定 | 配置先 |
|---|---|---|
| Rust | 1.99.1、Windows GNUターゲット | `.tools/cargo/`、`.tools/rustup/` |
| GCC | w64devkit 2.10.1 | `.tools/w64devkit/` |
| Go | 1.27.3 | `.tools/go/bin/go.exe` |
| Godot | 4.7.2 stable | `.tools/godot/` |
| Node.js | 統合テストで使用。`setup.ps1`では導入しない | `node` が実行できる環境 |

RustとGoの依存は、それぞれ `Cargo.lock` とGoのモジュール定義で固定する。
ビルドが通らないことを理由に依存を更新せず、最初にツールと作業ツリーを確認する。

## ZIPと展開済み版を作る

通常はこちらを使う。Rustのreleaseビルドも行うため、古い実行ファイルを混ぜずに済む。
梱包時にもGodotのライセンス取得などでインターネット接続を使う。
PowerShellの同じセッションで実行する。

```powershell
$ErrorActionPreference = 'Stop'
Set-Location C:\Users\neo\GitHub\kokoverse-proto

$packageOutput = @(./scripts/package.ps1)
$packageOutput | ForEach-Object { Write-Output $_ }

$zipLine = $packageOutput | Where-Object { "$_" -like 'PACKAGE_ZIP=*' } | Select-Object -Last 1
$rootLine = $packageOutput | Where-Object { "$_" -like 'PACKAGE_ROOT=*' } | Select-Object -Last 1
if (-not $zipLine -or -not $rootLine) { throw 'Packaging did not return output paths' }

$zip = ([string]$zipLine).Substring('PACKAGE_ZIP='.Length)
$packageRoot = ([string]$rootLine).Substring('PACKAGE_ROOT='.Length)
$dist = Split-Path -Parent $zip
$unpacked = Join-Path $dist ([IO.Path]::GetFileNameWithoutExtension($zip))
if (Test-Path -LiteralPath $unpacked) { throw "Already exists: $unpacked" }
Expand-Archive -LiteralPath $zip -DestinationPath $dist

Write-Output "ZIP: $zip"
Write-Output "Start: $(Join-Path $unpacked 'Start.bat')"
```

`package.ps1` の既定の出力先は `dist/`。展開済み版は自動では作らないため、上の `Expand-Archive` まで実行する。
`-OutputDirectory` でZIPの出力先を変更できる。

```text
dist/
  Kokoverse-Windows-x64-YYYYMMDD-HHMMSS.zip
  Kokoverse-Windows-x64-YYYYMMDD-HHMMSS.zip.sha256
  Kokoverse-Windows-x64-YYYYMMDD-HHMMSS/
    Start.bat
    Stop.bat
    bin/
    client-godot/
    scripts/
    build-info.json
```

`PACKAGE_ROOT` は `.run/packages/<ID>/<版名>/` にある梱包用フォルダーを指す。
`PACKAGE_ZIP` が配布ファイル。利用者へ渡すのはZIPと、必要ならそのZIPから展開したフォルダー。
既存の配布フォルダーへ上書きしない。起動済みフォルダーには利用者のワールドやキャッシュがある。

### `-SkipBuild` はRustのビルド直後だけ使う

Rustをすでにreleaseビルドし、それ以降にRustのソース・依存を変えていない場合は、`package.ps1` によるRustの再ビルドを省略できる。

```powershell
. ./scripts/env.ps1
cargo build --release --workspace --locked
if ($LASTEXITCODE -ne 0) { throw 'Rust release build failed' }
./scripts/package.ps1 -SkipBuild
```

`-SkipBuild` が省くのはRustのビルドだけ。Goのテスト・ビルド、Godotのインポート、梱包は実行する。
デバッグ版や別の作業ツリーから作った実行ファイルを `target/release/` に流用しない。

## 梱包されるものと、持ち込まないもの

`package.ps1` は次を行う。処理が失敗した場合は配布を止め、エラーの段階を確認する。

| 処理 | 内容 |
|---|---|
| Rust | Engineと管理サーバーをreleaseビルド |
| Go | Adapterのテスト後、Adapterとローカル中継をビルド |
| Godot | クライアントをコピーし、元プロジェクトと梱包先の両方でインポートを検証 |
| アバター | お絵描きアバター、変換済みリソース、対応するチェックサムをコピー |
| 起動処理 | `distribution/` と管理API版の起動・停止スクリプトをコピー |
| 配布情報 | ソース、ライセンス、ファイルのSHA-256一覧、ZIPのチェックサムを作成 |

`client-godot/` 直下の `.gd` などをまとめてコピーするため、そこに追加したランタイム用スクリプトを梱包一覧へ個別に登録する必要はない。
`*_smoke.gd` は実行テスト用で、配布クライアントから除外される。
アバターのキャッシュは [copy-avatar.ps1](../scripts/copy-avatar.ps1) に任せる。変換済みファイルだけを手で選ぶと、再インポート時に壊れることがある。

利用者の `.run/`、ワールド、インポートした画像・動画・モデル、個人の設定、秘密鍵を持ち込まない。
現在の管理API版には接続パスワードを同梱しない。旧版の `-RelayCredentialsFile` 引数や `relay-access.json` を持ち込まない。
FFmpegの実行ファイルはZIPに入らず、利用者が `Install-Video-Tools.bat` で導入する。
インストーラーはGyanDのGitHubリリースからSHA-256固定のZIPを取得し、接続できない場合は元の配布元へ切り替える。取得済みZIPは `%LOCALAPPDATA%/Kokoverse/tool-cache/` に保存され、次の版での導入に再利用する。
配布版の `client-godot/version.txt` は梱包時にタグ由来の版名（例：`tester-v2026.10.24.4`）から生成する。アプリのホーム画面上部とウィンドウタイトルはこの版名を表示し、開発用のクライアントでは「開発版」と表示する。

## ZIPを展開して内容を照合する

上の手順で得た `$zip` と `$unpacked` を使う。起動前に検証すると、利用者のキャッシュと配布ファイルを混同しない。

```powershell
$expectedHash = ((Get-Content -LiteralPath ($zip + '.sha256') -Raw) -split '\s+')[0]
$actualHash = (Get-FileHash -LiteralPath $zip -Algorithm SHA256).Hash.ToLowerInvariant()
if ($actualHash -ne $expectedHash) { throw 'ZIP checksum mismatch' }

$manifest = Get-Content -LiteralPath (Join-Path $unpacked 'build-info.json') -Raw | ConvertFrom-Json
if ($manifest.sessionMode -ne 'managed') { throw 'Managed sessions are missing' }
foreach ($file in $manifest.files) {
    $path = Join-Path $unpacked $file.path
    if ((Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant() -ne $file.sha256) {
        throw "Packaged file mismatch: $($file.path)"
    }
}
if (Test-Path -LiteralPath (Join-Path $unpacked '.run')) { throw 'Runtime data was packaged' }
```

前回の `build-info.json` とも比較する。通常の管理API版では次が揃っていることを確認する。

- `bin/kokoverse-engine.exe`、`bin/kokoverse-asset-server.exe`、`bin/kokosync-adapter.exe`、`bin/kokosync-relay.exe`
- `Start.bat` → `scripts/start-instance.ps1` → `scripts/start-online.ps1` → `scripts/start-local.ps1` の起動経路
- `client-godot/service_panel.gd`、`scripts/process-lifecycle.ps1`、`client-godot/render_hierarchy.gd`
- `build-info.json` の `sessionMode: managed`、管理API・中継の接続先、`includesUserWorld: false`、`includesCredentials: false`

ファイル一覧やハッシュの一致だけでは機能確認にならない。ログ窓、セッション、UIは次の手順で実行して確認する。

## 変更した機能と、配布版の起動をテストする

Rustを変更した場合は `cargo test --workspace --locked` を実行する。
UIの統合テストにはNode.js、release版Engine、開発用Adapterと中継が必要になる。
`package.ps1` のGo出力は梱包先の `bin/` なので、統合テスト用の配置は別に用意する。

```powershell
. ./scripts/env.ps1
$repo = (Get-Location).Path
$go = Join-Path $repo '.tools/go/bin/go.exe'
New-Item -ItemType Directory -Force .run/build | Out-Null
Push-Location tools/kokosync-adapter
try {
    & $go build -mod=readonly -o (Join-Path $repo '.run/build/kokosync-adapter.exe') .
    if ($LASTEXITCODE -ne 0) { throw 'Test adapter build failed' }
    & $go build -mod=readonly -o (Join-Path $repo '.tools/kokosync-relay.exe') ./cmd/relay
    if ($LASTEXITCODE -ne 0) { throw 'Test relay build failed' }
} finally { Pop-Location }
```

今回の変更に関係するテストを選ぶ。描画ウィンドウがフォーカスを取り合わないよう、順番に実行する。

| 変更対象 | 実行するテスト |
|---|---|
| 新規作成・配置・保持 | `node tests/creation-integration.mjs --rendered` |
| 3Dインスペクタ・入力・共有ギズモ | `node tests/panels-integration.mjs --rendered` |
| Script追加・削除・実行 | `node tests/ugc-integration.mjs --rendered` |
| 共同編集・Remove Script後の操作・新規作成 | `node tests/script-documents-integration.mjs --rendered` |
| 親子の追従・ローカルDrive | `node tests/hierarchy-integration.mjs --rendered` |

配布版の起動テストは、**展開済みの配布用フォルダーではなく `$packageRoot`** に対して行う。
このテストはログ用コンソールとGodotを開き、テスト用HN・セッション・保存ワールドを作る。
実行には公開管理APIと中継へ接続できる環境が必要。

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass `
    -File ./tests/managed-launcher-smoke.ps1 `
    -PackageRoot $packageRoot `
    -Server https://45.77.130.146/api `
    -Relay wss://45.77.130.146/ws
if ($LASTEXITCODE -ne 0) { throw 'Packaged launcher test failed' }
```

Engineのログ窓が見えること、同じプロファイルを明示した再実行では既存の画面に戻ること、Godotを閉じても再接続できることを確認する。
ログ窓の×による保存・終了と、終了後の再起動も検証する。
UIの配布確認では、HN入力、セッション作成・参加、ライブラリ、FPSでの3Dパネル操作も確認する。

`Start.bat` は押すたびに別インスタンスを起動する。こちらのテストでは実際のバッチを同時に2回起動し、プロファイル・ポート・Engine・Godotが分かれることを確認する。さらに、片方だけを終了し、空いたプロファイルをデータを残して再利用できることを確認する。

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass `
    -File ./tests/multi-instance-smoke.ps1 -PackageRoot $packageRoot
if ($LASTEXITCODE -ne 0) { throw 'Multi-instance test failed' }
```

このテストは一時コピーで実行し、本人情報も専用の一時フォルダーへ分ける。

テスト後の梱包用フォルダーには `.run/` ができる。そのフォルダーを再圧縮しない。
配布にはテスト前に作ったZIPを使い、展開済み版もそのZIPから作る。
テストで修正が必要になったら、ソースを直して新しい版名でビルドし直す。

## 問題が出たら、該当する出力を確認する

| 症状 | 確認先 |
|---|---|
| Rustのビルド失敗 | `env.ps1` の読み込み、ツールチェーン、`Cargo.lock`、ビルド出力 |
| Godotのインポート失敗 | `SCRIPT ERROR:`、`ERROR:`、新しい `.gd` 依存、アバターのキャッシュ |
| ログ窓がない／セッション一覧がない | `Start.bat` の呼び先、管理API版Engineと起動スクリプトを同じ版から梱包したか |
| 起動中のエラー | `.run/online/<profile>/engine-error.log`、`godot-error.log`、`launcher-error.log` |
| UIだけ古い | 展開したフォルダーの版名と `client-godot/`、実際に起動中のプロセスのパス |

配布済みの版へ新しいEXEや `.gd` だけを上書きして直さない。
現在の起動・保存の説明は配布物の `README.txt`、実装上の注意は [不具合から得た知見](../guidelines/lessons-learned.md) を参照する。
