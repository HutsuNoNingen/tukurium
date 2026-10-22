# Kokoverse prototype

Rust / bevy_ecs のEngineとGodotの表示クライアントを別プロセスで動かす、共同編集の最初のプロトタイプ。各Engine間はGoアダプターを経由してKokoSyncで接続する。Tonariumとのデータ互換性は持たせない。

現在は**箱や画像を生成・複製・削除し、名前・位置・回転・スケール・色・Colliderを共有編集できる**。ホストEngineのECS Scheduleが編集を検証・適用し、確定した差分を配信する。Tonariumで使われているお絵描きアバターも表示し、名前・位置・向きを同期する。画面はGodot、ワールドの正本はRustにある。

アバターの移動・ジャンプ・重力も各PCのRust ECSが固定60Hzで計算する。Godotは操作入力を送り、返ってきた姿勢を補間して表示する。表示クライアントを閉じても、Engineは着地まで計算を続ける。

## 起動（Windows x64）

PowerShellで、このフォルダーから実行する。

```powershell
./scripts/setup.ps1
./scripts/build.ps1
./scripts/start-demo.ps1
```

初回セットアップはRust、GCC、Go、Godotと動画変換用FFmpegを `.tools/` にダウンロードする。動画ツールだけ追加する場合は `./scripts/setup-media.ps1`。システムのPATHやレジストリは変更しない。Rust 1.99.1、bevy_ecs 0.17.3、Godot 4.7.2、Go 1.27.3を使用し、RustとGoの依存はロックファイルで固定する。

デモはlocalhostのKokoSync中継、ホストとゲストのEngine、それぞれのアダプター、Godotの2画面を起動する。

- `+ Cube`で生成、ワールド一覧または3Dの物体をクリックして選択。
- 選択するとX/Y/Zの移動ギズモが表示される。矢印をドラッグすると移動中の位置も相手に同期される。Shiftで0.25刻み、Escまたは右クリックで取り消し。離すと共有ワールドへ確定する。
- エクスプローラーから画像・GLB/FBXモデル・動画を画面へD&Dして配置。複数ファイルの同時投入と、`Import…`ボタンからの選択にも対応する。移動・回転・拡縮・複製・削除は箱と同じ操作。
- 動画を選ぶと、インスペクタにPlay/Pause・Restart・再生位置のスライダー・Loopが表示される。操作は全員に同期し、途中参加した画面も現在の位置から再生する。
- `Duplicate`で複製、`Delete…`で確認して削除。インスペクタの値は自動同期する。
- 三人称・フリーカメラは右ドラッグで視点を動かし、右ボタン＋WASDで移動。FPSはマウスで見回し、WASDで移動する。
- FPSではColliderを持つ物体に照準を合わせ、右ボタンを押し続けると保持できる（箱・画像共通）。ホイールで距離、E＋マウスで視点を固定して自由回転、Shift＋E＋マウスまたは横ホイールで垂直軸まわりに回転、Shift＋ホイールで均等拡縮。左クリックで方位を保って傾きを起こし、ダブルクリックで視点への追従を水平回転のみ／上下回転込みで切り替える。右ボタンを離すと配置、Escで取り消す。空間を左クリックするとFPS操作へ戻る。
- Inspectorから名前・色・位置・回転・各軸のスケールとColliderを編集できる。値を変えるとApplyなしで自動同期する（約160ms間隔）。`I`キーまたは上部のInspectorボタンで表示を切り替える。Colliderの有無が選択・把持、Solidがプレイヤーとの衝突を決める。画像の初期設定はColliderあり・Solidなし。保持中も位置・回転・スケールを同期し、離すとまとめて確定する。
- `V`または上部メニューで三人称→一人称（FPS）→フリーカメラを切り替え。フリーカメラではQ/Eで上下移動。
- アバター操作中はSpaceでジャンプ。箱に当たり、箱の上へ乗れる。乗っている箱を動かすと追従する。`F`で選択した物体を見に行く。アバター操作へ戻るには`V`。
- SpaceはUIの決定操作に使わない。メイン画面のボタン・チェックボックスにフォーカスが残っていてもジャンプできる。文字・数値・コードの入力中はジャンプせず、入力欄として扱う。UIのキーボード決定はEnter。
- `PEOPLE`の入力欄で表示名を変更し、Enterで確定。
- `PEOPLE`の`Avatar texture…`でお絵描きアバター用の画像を選び、`Reset`で標準へ戻す。見た目は他の参加者と共有され、次回起動時にも引き継ぐ。
- 物体を選び、Inspectorの`Script Systems…`からLuauの動作を追加する。`Rotator`は回転、`ClickColor`は左クリックで色を切り替える例。コードを書いて`Apply / restart`で反映し、公開パラメータは自動同期する。`Run on host`で停止・再開。コード・設定・明示的な状態も保存・複製できる。詳しくは[UGC Script Systems](docs/ugc-scripting.md)。

```powershell
./scripts/stop-demo.ps1
```

停止スクリプトは、記録した実行ファイル・PID・起動時刻が一致するプロセスだけを停止する。ログは `.run/` にある。Godotを閉じてもEngineは動き続ける。

## 構成

```text
Godot :47000 ↔ Host Engine :47001 ↔ Go adapter
                                      ↕
                             KokoSync :48001 (QUIC)
                                      ↕
Godot :47010 ↔ Guest Engine :47011 ↔ Go adapter
```

- `crates/core`：ECS、実行順を定義したSchedule、ObjectRoot・Slot・Transform・MeshRenderer、編集検証、参加者の状態。
- `crates/protocol`：共有型と、最大1 MiBの長さ付きJSONフレーム。
- `crates/engine`：ホストの編集承認、ゲスト複製、描画クライアント接続、再同期。
- `client-godot`：3D空間、選択、実行時インスペクタ、アバター、参加者一覧。
- `tools/kokosync-adapter`：KokoSyncの配送。ECSの意味や編集ルールは持たない。

詳細は [現在の実装と通信仕様](docs/runtime.md)、設計全体は [design.md](docs/design.md)、KokoSyncの接続方針は [kokosync.md](docs/kokosync.md) を参照。

複数の共有ワールド内インスペクタに向けた調査と実装案は [Resoniteのインスペクタ調査](docs/resonite-world-inspectors.md) を参照。ワールド内パネルはまだ未実装。

## 検証

```powershell
. ./scripts/env.ps1
cargo test --workspace --locked
cargo clippy --workspace --all-targets -- -D warnings
cargo build --release --workspace --locked
node tests/integration.mjs --godot
node tests/assets-integration.mjs
node tests/assets-integration.mjs --large
node tests/playback-integration.mjs
node tests/playback-integration.mjs --rendered
node tests/avatar-integration.mjs
node tests/persistence-integration.mjs
node tests/ugc-integration.mjs --rendered
node tests/ui-focus-integration.mjs --rendered
```

統合テストは独立したローカル中継・Engine・アダプターを起動して後片付けする。双方向の編集、同一フィールドの競合、重複要求、再接続、不正値、生成・複製・削除、アバターの名前・姿勢同期と退出を確認する。Godotのフォーム・ボタン操作とモデルの読み込みも含む。強制切断では中継側の古い接続が失効するまで再参加を待つため、約1分かかる場合がある。

再生同期テストは3つのEngineとGodotを使い、再生・一時停止・シーク、途中参加、意図的にずらした再生位置の補正を確認する。`--rendered`は描画ウィンドウを開き、シーク後の映像フレームの一致も検証する。

## 現在の範囲

デスクトップ、最大128個の物体（箱・画像・動画パネル・モデル）、画面右側のインスペクタ、アバターと参加者一覧が対象。編集競合はフィールド単位、構造変更はObjectRoot単位のrevisionで拒否する。入力中の別ユーザーの変更でフォームを上書きしない。切断後は未確定の編集を自動再送せず、現在のワールドを取得し直す。

開発用デモはlocalhostを使う。配布版は`wss://45.77.130.146/ws`の公開中継と固定ルーム`kokoverse/workshop`を使う。公開経路では通常のTLS検証とサーバーのBasic認証を行う。明示的に指定したビルドでは接続情報をZIPへ同梱できる。公開中継を経由した3クライアントの編集・動画同期・途中参加を同じPC上で検証した。異なるPC・ネットワークでの性能は未測定。参加者は全員編集可能で、利用者別の編集権限やホストの自動交代は未実装。

画像は縦横比を保ち、必要に応じて縮小・圧縮して共有する。1画像192 KiB、ワールド全体の画像データはBase64で512 KiBまで。現在は画像をsnapshotに含めるための上限で、超えた操作は理由を表示して拒否する。

モデルの入力はGLB/FBX、1個100 MiBまで。FBXはバックグラウンドでGLBへ変換して共有する。外部テクスチャはFBXと一緒に配置しておく。GLBはテクスチャ埋め込み形式。最大辺を2mに合わせ、外接直方体のColliderを付ける。モデル内部のメッシュ階層・材質は描画に使い、ワールドでは1個の物体として扱う。

動画の入力はMP4/MOV/WebM/MKV/AVI/OGV、1個100 MiBまで。FFmpegでOgg Theora＋Vorbisへ変換し、最大720p・30fpsの3Dパネルとして音声付きで再生する。動画にも把持用Colliderが付き、右クリックで移動・回転・拡縮できる。再生・一時停止・シーク・リスタート・ループ設定はRustの`MediaPlayback`として共有する。ホスト時刻に合わせて各画面が再生し、途中参加やアセット取得待ちのあとも現在の位置へ追いつく。再生中のずれが0.25秒を超えると補正する方式で、フレーム単位の厳密な同期ではない。

変換後のモデル・動画は1個128 MiBまで。SHA-256をキーに保存し、持っていないクライアントへ128 KiBずつ転送する。配布版の保存先はポートが変わっても同じ `.run/assets/shared/`。以前の配布版のポート別キャッシュは起動時にコピーして引き継ぐ。Engine単体の既定は `.run/assets/<UIポート>/` で、`--asset-dir`で指定できる。移動の確定差分には参照だけを含める。ワールドを別環境へ持ち出す場合は、snapshotとホストのアセットフォルダーを一緒に保存する。受信した動画の再生用キャッシュは `.run/media/` に置く。

箱やモデルの上への着地、動く足場、回転・拡縮したColliderとの衝突に対応する。Script Componentは各物体4個・全体32個まで追加でき、LuauのSystemをホストで実行する。現在のquery範囲は取り付け先の物体。ワールド内の3Dインスペクタ、親子構造、複数物体のアイテム化、Flux、保存UI、VR、音声チャット、ホスト移譲は今後の段階。物体の同期はオブジェクト単位の差分、アバターは20Hzの損失許容配送。大規模ワールドは未検証。

アバターの出典と変換方法は [assets/oekaki/README.md](client-godot/assets/oekaki/README.md)。モデル・サンプル画像を移植し、描画処理はGodot用に実装した。Tonariumのアカウントや通信形式は使わない。

## 配布用ZIP

`./scripts/package.ps1`でWindows x64向けZIPを`dist/`に作る。Rustのreleaseビルド、Goアダプター、Godot、お絵描きアバター、起動スクリプト、依存ライセンスをまとめる。受取側で開発ツールは不要。アバターのインポート済みリソースも同梱する。現在のワールドとユーザーが取り込んだアセットキャッシュは含めない。`-RelayCredentialsFile`にUsername/Passwordを記したファイルを指定すると、自動ログイン用の接続情報も同梱する。認証情報をリポジトリへ書き込まない。

展開後は`Start.bat`でユーザー名を入力する。最初の接続が`host`を取得し、それ以降は自動生成した参加者IDで入る。ホスト名の重複は中継サーバーが拒否するため、同時起動でもホストは1人になる。接続切れだけではホストを自動交代しない。接続先とルームは`connection.json`に固定する。同梱版は自動ログインし、それ以外では認証情報の入力画面を出す。FFmpegはGodot起動後に背後で取得する。起動ログは`.run/startup.log`、停止は`Stop.bat`。

配布版のホストは確定した変更を約2秒間隔で `.run/world.json` へ自動保存する。書き込みは別スレッドで一時ファイルを作成してから置き換え、直前の正常な保存を `.run/world.json.bak` に残す。`Stop.bat` は最新状態の保存完了を確認してから終了し、保存に失敗した場合は終了せず理由を表示する。次に**同じフォルダーからホストとして起動**すると復元する。主ファイルが壊れていたらバックアップを使い、両方が読めない場合は初期ワールドで上書きせずエラーにする。ゲストは自分の保存ファイルを読み書きしない。強制終了では最後の保存後の変更が失われる場合がある。

Engineを直接起動する場合は `--save <world.json>` で同じ自動保存・復元を有効にできる。`--load <file>` は明示した初期ワールドを読み込む。保存対象は物体・Transform・Collider・画像・動画の再生状態で、参加者の一時的な位置や未確定のドラッグは含まない。モデル・動画の実体はアセットフォルダーに残る。再生中の動画は保存された時刻を基準に進むため、再起動までの時間も経過する。保存UIとホスト交代は未実装。

同じPCで複数フォルダーから起動するときも、ローカルのIPCポートはOSの空きポートから自動選択する。ポート確保とEngine起動の間は起動用Mutexで直列化する。フォルダーのコピーを検出すると参加者IDを作り直す。起動中のポート・参加者IDは各フォルダーの`.run/connection.json`に記録する。配布版を用いた並行起動の確認は`./tests/launcher-smoke.ps1 -PackageRoot <展開フォルダー>`で実行できる。
