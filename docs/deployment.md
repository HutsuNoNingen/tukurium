# 公開サーバーでセッションとライブラリを使う

KokoSyncと同じサーバーに、ユーザー・セッション・保存アイテムを管理するAPIを配置する。ワールドの実行と編集の承認は参加者のホストEngineが担当し、リアルタイム同期は既存のKokoSyncを使う。

## 接続先

| 用途 | 外部からの接続先 | サーバー内部 |
|---|---|---|
| 管理・アセットAPI | `https://45.77.130.146/api` | `127.0.0.1:8787` |
| KokoSync | `wss://45.77.130.146/ws` | 既存の `127.0.0.1:8080` |

外向けは両方ともHTTPSの443番を使う。nginxがパスで振り分けるため、ポートは競合しない。8787番をインターネットへ開放する必要はない。TLS証明書は既存のKokoSyncと共用する。

HNを入力すると、そのHNの利用者としてログインする。前後の空白を除いた文字列が完全に同じなら、別PCでも同じライブラリ・素材・利用権が見える。大文字小文字は区別する。本人確認は行わず、同じHNを知る人は同じ利用者として扱う。内部のAPI要求には自動発行したセッションキーを使い、画面でキーやパスワードを入力する必要はない。中継の `/ws` と `/health` もパスワード不要とし、それ以外の既存管理経路は保護を維持する。登録はIPごとに毎分6件、初期バースト10件、ほかのAPIは毎秒10件、初期バースト100件。素材はHNごとに1 GiB、1ファイル128 MiBまで。

## Windowsから起動する

```powershell
./scripts/start-online.ps1 -Build
```

起動後のGodot画面でHNを入力して「Start」を押し、セッションを作成するか一覧から参加する。HNの切り替えはセッションから退出してから行う。以前の `relay-login.xml` や `relay-access.json` は読み込まない。

```powershell
./scripts/start-online.ps1 -Profile guest
./scripts/stop-online.ps1
./scripts/stop-online.ps1 -Profile guest
```

配布版では `Start.bat` / `Stop.bat` を使う。`StartLocal.bat` はローカルサーバーを起動する。以前の固定ルーム版は `StartLegacy.bat` / `StopLegacy.bat` に残す。`Start-Host.bat` も従来の固定ルーム用である。

接続先は [connection.json](../distribution/connection.json) の `service` と `relay` で指定する。公開サーバーの本人情報は `%LOCALAPPDATA%/Kokoverse/servers/<接続先のSHA-256>/profiles/<profile>/profile.dat` にDPAPIで保護して保存する。ローカル用の本人情報を公開サーバーへ自動送信しない。ワールド・キャッシュ・起動ログは `.run/online/<profile>/` に保存する。

## サーバーの保管場所と運用

| 対象 | 場所 |
|---|---|
| 実行ファイル | `/opt/kokoverse/bin/kokoverse-asset-server` |
| ビルド元 | `/opt/kokoverse/source` |
| 管理DB | `/var/lib/kokoverse/catalog.sqlite` |
| 素材 | `/var/lib/kokoverse/assets/` |
| systemd | `kokoverse-api.service` |
| nginxのAPI設定 | `/etc/nginx/snippets/kokoverse-api.conf` |
| nginxのアクセス制限 | `/etc/nginx/conf.d/kokoverse-rate-limits.conf` |
| 更新前の実行ファイル・DB・nginx設定 | `/opt/kokoverse/backups/<UTC日時>/` |

サービスは専用の `kokoverse` ユーザーで起動し、OS再起動後にも自動で立ち上がる。書き込み先は `/var/lib/kokoverse` に限定する。SIGTERMを受けて処理終了を待ち、異常終了時はsystemdが再起動する。

```sh
systemctl status kokoverse-api
journalctl -u kokoverse-api -n 100 --no-pager
curl --fail https://45.77.130.146/api/health
```

更新時は `Cargo.toml`、`Cargo.lock`、`rust-toolchain.toml`、`crates/`、`deploy/` をビルド元へ転送し、そこで実行する。鍵は手元のSSH設定から指定し、リポジトリに含めない。

```sh
cd /opt/kokoverse/source
/root/.cargo/bin/cargo build --release -p kokoverse-asset-server --locked -j 2
sh deploy/install.sh
```

導入スクリプトはnginx設定を検査し、APIの起動確認後にnginxをリロードする。KokoSyncは再起動しない。API起動に失敗した場合は、保存してあった実行ファイルとnginxのサイト設定へ戻す。

バックアップを取るときは `systemctl stop kokoverse-api` でAPIを止め、`/var/lib/kokoverse/` 全体をコピーしてから再開する。DBだけでなく素材とSQLiteの補助ファイルも保存する。ホストが持つワールドは各PCにあり、別途バックアップが必要である。

## インターネット経由の検証

資格情報の設定は不要。使い捨てのテストHNと保存アイテムを公開サーバーに作成し、テスト用セッションは終了する。

```powershell
$env:KOKOVERSE_TEST_SERVER = 'https://45.77.130.146/api'
node tests/managed-integration.mjs
Remove-Item Env:KOKOVERSE_TEST_SERVER
```

この経路では、HTTPS上の本人情報・セッション・保存・権限制御と、公開WSS上のホスト／ゲスト同期を合わせて確認する。共有サーバーを停止する再起動テストは行わない。ローカルの管理サーバー再起動テストは従来どおり環境変数なしで実行できる。

2026-10-23に本サーバーへ配置し、この公開経路のテストに合格した。同じHNのライブラリ共有、異なるHNの権限制御、2ユーザーの同期・保存・共有・再配置、2 MiBのGLBの送受信とSHA-256一致、Godotの操作を確認した。同じPC上の複数プロセスによる検証であり、別PC間の性能試験はまだ行っていない。

2026-10-24（JST）更新：ライブラリの保存・取得・共有・再公開に対する所有者・ライセンス制限を解除した。サインイン済みの利用者は既知の保存版IDと素材ハッシュから取得できる。個人の一覧の編集、公開停止、セッション管理、形式・ハッシュ・容量の検証は維持する。公開HTTPS/WSS経由の統合テストとGodot画面のテストに合格した。
更新前の実行ファイルとDBは `/opt/kokoverse/backups/20261023T183309/` に保管した。フォルダー名はUTC時刻である。ビルドに使ったソースは `/opt/kokoverse/builds/library-open-20261024/` に残し、通常の更新先 `/opt/kokoverse/source/` にも現在のソースを反映した。
