# 現在の実装

2026-10-23。RustのECS Schedule、箱・画像・モデル・動画の共同編集、動画の再生同期、3D移動ギズモ、Tonarium由来のアバターモデル、ジャンプとFPS視点を実装した。描画はGodot、確定したワールド状態はホストEngine、PC間の配送はKokoSyncが担当する。

## Systemと実行順序

`crates/core/src/lib.rs` の `WorldModel::empty()` が、次のSystemを `.chain()` で登録する。

```text
validate_commands   編集・構造変更を検証する
apply_commands      検証済みの変更をComponentへ適用する
apply_presence      参加者の生成・更新・退出を反映する
update_transforms   LocalTransformからGlobalTransformを計算する
collect_changes     ECSのChangedで変更されたObjectRootを集める
extract_snapshot    描画・検査用の確定状態を生成する
publish_changes     変更した箱と削除したIDを確定差分へまとめる
extract_presence    参加者の表示用データを更新する
```

Engineは約16msごとに `WorldModel::advance(elapsed)` を呼ぶ。まず `tick()` で編集要求を処理する。受け付けた要求はキューに置き、Systemが動くまでComponentを変えない。1回のtickでは最大128要求を処理し、要求ごとに上の順を実行する。これにより同じtickに同じrevisionの編集が2件来ても、2件目は1件目の変更後の状態で検証される。入力キューの上限は256件。

続いて、経過時間を蓄積して別のScheduleの `update_grab → sync_colliders → simulate_character → extract_presence` を固定1/60秒で実行する。1回のadvanceで取り込む経過時間は最大0.25秒。編集要求の数、snapshotの復元、描画fpsによって移動量や落下量を増やさない。`tick()` だけではシミュレーション時刻は進まない。

生成・削除と安定IDの登録を同時に行う `apply_commands` と `apply_presence` はexclusive System。それ以外はQuery / Resourceを引数に取るSystem。通信ハンドラーやGodotがComponentを直接書き換えることはない。

Flux、階層Transformはまだない。キャラクターには移動・ジャンプ・重力・床と箱との衝突を扱うSystemがある。現在の箱は全て独立したルートで、GlobalTransformはLocalTransformと同じ位置になる。

## Entityと編集

1個の箱につき2個のEntityを持つ。

- Slot Entity：`Identity`、`ObjectRoot`、`Slot`、`LocalTransform`、`GlobalTransform`、`Revisions`。
- 部品Entity：`Identity`、`MeshRenderer`。所属Slotの安定アドレスと色を持つ。

安定アドレスは `(instance_id, local_id)`。Slotはlocal_id=1、描画部品は2。同期上のComponentアドレスはTransform=3、Collider=4、MediaPlayback=5、WorldPanel=6、InspectorView=7、ScriptComponents=8、Mirror=12、Camera=13。これらのComponentは型付きのECSデータとして保持する。ECSのEntity番号を送信しない。複製では新しいinstance_idを作り、元の箱の値をコピーする。元と複製のrevisionは共有しない。箱は最大128個で、最後の1個を削除して空のワールドにもできる。

通常の編集はフィールドrevision、複製と削除はObjectRoot全体のrevisionを確認する。名前は1〜80文字、位置は各軸±1000の有限数、色はRGBA各0〜1。同じフィールドの同時変更は先着が成功し、古い要求には現在値と理由を返す。

ホストは要求元接続とcommand_idの組を処理中・処理済みで記録し、二重実行を防ぐ。処理済みの保持数は4096件。ネットワークの要求元はKokoSyncが通知するpeerとpublisherEpochで識別する。

## ワールド同期

ローカルIPCは4バイトのbig-endian長さ＋UTF-8 JSON。1フレームの上限は1 MiB。現在のprotocol_versionは3。旧プロトコルおよび旧ObjectView形式の保存データには対応しない。revisionとsequenceは10進文字列。

## セッション参加者とチャット

参加者は従来どおり一時的なECSの `Participant` として保持する。Godotは受け取った位置から一覧の距離と表示順だけを計算する。セッションチャットは登録済みの一時Component `SessionChat` に最大200件を保持し、ワールドの保存用snapshotと `world_revision` には含めない。ホストEngineが送信元のpeerと現在の参加者を照合し、送信者名を確定する。本文は空文字、制御文字、1000文字または4096バイト超を拒否し、同じ参加者の送信には300msの間隔を設ける。

ホストは確定したメッセージをKokoSyncの確実配送で全員へ送り、途中参加者には `component_snapshot` と一緒に履歴を渡す。各端末のGodotはEngineからの表示用コピーだけを描く。セッションを切り替えるとチャットを破棄するが、同じワールドの再同期では履歴を保持する。UIのパネル開閉・入力フォーカス・スクロール位置は各クライアントのローカル状態。

確認：[chat-integration.mjs](../tests/chat-integration.mjs) は2人の送受信、送信者の確定、途中参加、無効本文の拒否とワールドrevisionの不変を調べる。[hud-integration.mjs](../tests/hud-integration.mjs) の `--rendered` はTab、マウス捕捉時の薄い表示と入力停止、チャット投稿の実入力経路を調べる。

内部の `WorldCommit` を、`crates/core/src/replication.rs` の登録表で `ReplicationCommit` へ変換する。通信は `component_commit`、初回状態は `component_snapshot`。各物体はComponentの安定アドレス・型名・スキーマ版と、フィールドID・revision・値を持つ。画像・モデル・動画・パネル・Scriptも同じ形式で運ぶ。保存も同じ登録表を使い、`format_version: 2` の `ReplicatedWorld` を書き込む。既存の版1ワールドにあるCamera・Mirror形状は読み込み時に対応する`Enabled=true`の機能コンポーネントを補う。版2では形状だけから機能を推定しないため、コンポーネントを外した状態も保存できる。

登録表には型検証、編集可否、型付きECSへの書き込み、表示データとの変換をまとめた。付け外しできる型は`ComponentSchema.attach_defaults`に初期値を載せ、Godotの追加一覧はこれを候補として表示する。依存関係と値の最終検証はRustが行う。`Edit.target` はルートではなく対象Componentのアドレスで、例えば位置編集はlocal_id=3を指定する。新しいComponentを追加する際は登録とECSの抽出処理を追加し、通信のメッセージ分岐や保存形式の処理は増やさない。再生・Scriptのような操作は引き続き専用コマンドで検証する。

変更した物体だけをupsertし、複数Componentの変更は同じworld_revisionでまとめて適用する。ゲストは変更全体を検証し、不明なComponent・スキーマ版・フィールド、不正な値・revisionを拒否する。変更のないEntityは維持する。revisionの欠落や不正な差分では編集を止め、スナップショットを取得し直す。現段階では物体内の全Componentを送る。Component・フィールド単位の差分と購読範囲の最適化は未実装。

Godotへは現在の小さなワールドの表示用snapshotを送る。これは今後、対象を絞った購読・描画差分へ変更できる。各読み出しでQueryを走らせず、Systemが作ったキャッシュを使う。

KokoSyncのGo SDKは `5a171af078d3cf85cc3364c9358e1b993f4f9835` に固定する。

- ルーム：`kokoverse/<session_id>`。
- 確実配送のトピック：`kokoverse.v2`。
- 編集要求、編集結果、初回状態：`SendTo`。
- 確定した箱の差分：`Publish`。
- 姿勢と移動プレビュー：`Latest`、キー `kokoverse.pose.v2`。

開発用アダプターはQUICを使う。配布版の公開接続先は`wss://45.77.130.146/ws`、ルームは`kokoverse/workshop`。公開サーバーはQUICを外部へ開いていないため、アダプター内の`hosted.go`がSDKのローカルQUIC通信を認証付きWebSocketへ中継する。SDKの分割転送・受領確認・順序制御をそのまま使い、最新姿勢はWebSocketのバイナリフレームで運ぶ。公開側はOSの証明書検証を使い、自己署名証明書の例外はアダプター内のループバック接続だけに限定する。

アダプターの`--address`にWebSocket URLを渡すとこの経路を選ぶ。認証情報は`KOKOSYNC_USERNAME`と`KOKOSYNC_PASSWORD`から読み、コマンドラインやログへ出さない。接続先とルームはZIPの`connection.json`に置く。明示的に接続情報を同梱したビルドは`relay-access.json`から自動ログインし、未同梱なら前面の入力画面を使ってWindowsの暗号化付きXMLで保存する。動画ツールの初回取得はGodotを起動してから別プロセスで行う。

SDKの配送受理だけで編集成功とはしない。ゲストはホストのresultと対応するworld_revisionの反映を待つ。アダプターは確実配送の待機中も姿勢を処理できるよう、確実配送と最新姿勢の送出を分ける。姿勢の送信待ちは最後の1件だけを保持する。

## 要求と編集セッション

Godotの `request_broker.gd` がcommand_id／request_idを発行し、応答先、タイムアウト、切断時の失敗処理を共通管理する。最大128件を保持し、応答種別とIDが一致した要求だけを完了する。要求は完了時に取り除くため、重複応答でコールバックを繰り返さない。再接続で変更要求を自動再送しない。時計同期も同じ仕組みを使う。

`crates/core/src/edit_sessions.rs` は、UIの種類に依存しない期限付きのフィールド占有を管理する。キーはComponentのアドレスとフィールドID。`edit_session` で取得・更新・解放し、`edit_sessions` で所有者とドラフト、キャレット、選択範囲を共有する。保存には含めない。Engine側も通常の編集と同じ応答・保留要求の処理を使う。

任意の編集UIは `context: null` で利用できる。共有インスペクタは、InspectorViewの `inspector_target` を参照元contextに指定する。その参照が変わるか参照元が消えれば占有を解放する。フィールド削除、所有者の切断、6秒間の未更新でも失効する。キーボード入力先、フォーカス、IMEは各Godot内の状態で、他人のキー入力として再実行しない。

## アバターと参加者

アバターも同じECSに `Participant` Entityとして置く。名前・位置・向きは一時的なセッション状態であり、箱のworld_revisionや保存用snapshotに入れない。姿勢入力はtickまでまとめ、同じ参加者の古い入力を置き換える。

Engine間の姿勢は20Hzで配送する。受信側はKokoSyncの参加者一覧・publisherEpochを照合し、本文の自己申告IDを使わない。名前は40文字まで、位置・角度は範囲と有限数を検証する。退出通知または5秒間の姿勢未受信でアバターを除去する。再接続でpeer世代が変わった場合は古いアバターを引き継がない。

1つのEngineを1人の参加者として扱う。同じEngineに複数Godotをつなぐと同じアバターを操作する。現在は全員編集可能で、peer名はアカウント認証ではない。

従来の固定ルーム版の`StartLegacy.bat`はユーザー名を入力し、`--role auto` / `--peer auto`で起動する。アダプターはまず`host`としての参加を試し、サーバーが名前の重複を返した場合はランダムなゲストIDで再参加する。決まった役割は再接続時も維持し、ホストの一時切断でワールドを別の正本へ切り替えない。

お絵描きアバターの変更は`ParticipantView.avatar_texture`にSHA-256参照を持たせる。画像は最大2048四方・PNG 1 MiBに収め、通常のアセット転送でホストへ届いてから参照を公開する。姿勢には参照だけを含め、画像本体を20Hzで送らない。Godotは必要なPNGを取得して各アバターの材質へ適用する。参加者の見た目なのでworld_revisionは変えない。自分の画像は各アプリフォルダーの`.run/avatars/custom.png`に保存し、接続先の変更・再起動時には再アップロードする。

各PCのRustが自分のアバターをシミュレーションする。ローカルの `Participant` Entityに `CharacterMotion` Componentを付け、速度と接地状態を持たせる。`LocalControl` Resourceには操作入力と、その更新からの経過時間を保持する。他の参加者の姿勢は受信値として表示し、こちらで重力を再計算しない。移動入力はホストへの往復を待たず、自分のEngineへ送る。アバターの移動は現在も各参加者を信頼する方式で、ホストの移動検証や補正は未実装。

Godotからの `player_input` は左右・前後の軸、視点のyaw/pitch、ジャンプボタン、視点へ向くか、操作中かを含む。座標は送らない。旧UIの `pose` 入力は受け付けない。RustのSystemが視点基準の水平移動、斜め入力の速度制限、押した瞬間のジャンプ、重力、床と箱への着地を計算する。現在の速度は3.8、重力は18、ジャンプ初速は8.0。高さ1.5の箱に乗れるよう、ジャンプの最高到達点を約1.78にした。入力が0.25秒途切れるか、操作を終了すると横移動が止まり、空中では着地まで計算が続く。UI切断でも同様。同じEngineへの複数UI接続では、操作中の接続が入力を占有し、他の接続の中立入力で上書きしない。

Godotへは確定した姿勢とローカルの接地状態・速度を返す。Godotは位置を補間し、カメラとモデルを表示する。ワールドsnapshotの再同期でもローカルの姿勢・速度・入力を保つ。三人称・一人称（FPS）・フリーカメラを切り替えられ、FPSでは自分のモデルを非表示にする。フリーカメラはGodot側の表示操作で、キャラクターの位置を変更しない。音声、任意アバターの配布は未実装。

## 箱との衝突とFPSの保持操作

箱のSlot Entityに `BoxCollider` を付ける。現在は半径0.75の軸に平行な箱と、幅0.56・高さ1.7のキャラクター用直方体を使う。`sync_colliders` は毎シミュレーションステップで確定Transformと有効な移動プレビューから衝突形状を更新する。画像パネルにはキャラクター用の衝突形状を付けない。

キャラクターは移動区間と拡張した箱の交差を調べ、面に到達したら残りの移動を面に沿わせる。側面、上面、下面を扱い、動かした箱がキャラクターと重なった場合は押し出す。上に乗っている箱のIDと前回位置も保持し、箱が動けば追従し、削除されれば落下する。これはキャラクターのための運動制御であり、箱同士の剛体物理や、投げた箱の慣性を実装したものではない。

FPSではマウスで見回し、WASDで歩く。右ボタンを押すとRustが視点から6m以内の箱を探索し、当たった点を基準に保持する。保持距離と当たり点からのずれを記録し、視点を動かしても箱の中心が照準へ飛ばないようにする。ホイールの距離変化は現在距離に比例する。自分が保持している箱は自分の衝突判定から外す。

保持中の位置は既存のLatestプレビューで共有する。右ボタンを離すと通常のposition編集として確定し、Esc・フォーカス喪失・切断では取り消す。他の編集でposition revisionが変わった場合も保持を終了する。複数人の同時保持をロックで予約する機能はなく、解放時の編集競合は従来のrevisionで検出する。

Rustはレーザーの始点・先端・対象・保持状態も生成する。Godotは照準と線を描画し、保持中のレーザーは相手にも配信する。Resoniteのデコンパイルで確認した入力経路、保持点、距離操作、レーザー表示との違いは [調査メモ](resonite-interaction.md) に記録した。回転・拡縮・複数保持・ポータル越しの操作は未実装。

参考として、2026-10-22にインストール済みLumoraVRの `Lumora.dll` / `LumoraCore.dll` を静的に確認した。`GodotKeyboardDriver` が機器入力を受け取り、Coreの `PhysicalLocomotion` が入力を移動方向とジャンプ要求に変換し、`CharacterController` が `ICharacterControllerHook` へ処理を委譲していた。tukuriumでは入力と制御を分ける考え方を参考にし、今回の床・重力計算はRustのSystemに実装した。

## 移動ギズモと画像

`translation_gizmo.gd` はワールド軸X/Y/Zの矢印を表示し、投影した軸との距離からクリック対象を決める。画面上の大きさを距離に応じて保ち、ドラッグでは選択軸を含む平面とマウスのrayの交点から移動量を求める。軸と視線が重なって操作しづらい場合は、視点を回す。

ドラッグ中は表示ノードだけを動かし、ECSの確定値は変えない。対象アドレス・開始時のposition revision・位置を20Hzの姿勢パケットに同梱して相手へ送り、受信側Godotは補間して表示する。画像本体はプレビューに含めない。Engineはpeer世代・送信順・座標範囲・対象のrevisionを検証する。キャンセルはnullを繰り返し配信し、更新が1秒止まったプレビューも除去するため、古い位置が残り続けない。プレビューはEngineの一時状態で、保存用snapshotには入らない。同じEngine内では同時に1接続がドラッグを配信できる。

マウスを離すと、開始時のposition revisionを付けて通常の編集要求を送る。結果を受け取るまでプレビューを保ち、確定したrevisionで古いプレビューを無効にする。Shiftで0.25刻み、Esc・右クリック・ウィンドウのフォーカス喪失で取り消す。他の参加者が同じ位置を変更した場合や対象が消えた場合もプレビューを取り消す。拒否・切断では現在の確定値へ戻す。

`Visual::Image` はPNG/JPEGのdata URLと検証済みの寸法を持つ。ホストはBase64、画像形式、画素数、サイズを検証し、画像パネルを生成する。UIは最大1024pxへ縮小し、1枚192 KiB以下に圧縮する。ホストの寸法上限は2048px、全画像のBase64文字列の合計は512 KiB。初回snapshotを1 MiB以内に収めるための初期上限で、大規模な画像共有には別アセット配送が必要。

画像パネルは縦横比を保つQuadMeshで、箱と同じ選択・移動・複製・削除に対応する。画像はworld snapshot・commitに含めて届けるため、他のPCからローカルのファイルパスへアクセスする必要はない。移動中のプレビューは座標だけだが、移動の最終確定では従来どおり画像を含むObjectViewを配信する。

アバターのモデルと画像はTonariumの `client/public/oekaki/` 由来。Godot用にDraco圧縮を解除し、元の14メッシュと7つの待機アニメーションを保持した。サンプルは透明背景の線画。出典・変換方法は [アセットの説明](../client-godot/assets/oekaki/README.md) にある。

## ファイルのD&DとGLBアセット

GodotのWindow.files_droppedから `asset_importer.gd` が画像・GLB/FBX・動画を受け取る。複数ファイルは順番に処理し、マウスのrayが指す床・Collider付近へ配置する。画像は既存の圧縮・共有形式へ変換する。モデルは最大辺2mへ正規化し、メッシュ全体の外接直方体をColliderとして付ける。RustのImportAssetコマンドでObjectRootを生成する。

GLBと動画のバイト列はワールド状態に埋め込まない。SHA-256のasset_idを `Visual::Model` / `Visual::Video` に持たせ、Engineの `assets.rs` がファイル本体を管理する。既定の保存先は `.run/assets/<UIポート>/`、`--asset-dir`で変更できる。入力は100 MiB、変換後は128 MiBまで。128 KiBの要求・応答で順番に転送し、受信完了時にハッシュと形式を検証する。モデル・動画は変換後すぐにSlotと参照を共有し、ホストへのファイル転送を並行して進める。ホストに実体がない参照も受け付け、受信側の取得要求は転送完了まで保留する。後から参加したクライアントは表示するときに必要なファイルだけ取得する。移動・複製は参照だけを共有する。

`asset_status.gd` は各端末の転送一覧と、読み込み待ちの物体の上に状態を表示する。変換、ローカルEngineへの送信、ホストへのアップロード、アップロード待ち、ホストからのダウンロード、キャッシュ読み込み、検証、モデル生成を区別する。進捗の%とサイズは各区間で処理した実バイト数で、全工程を仮の%に換算しない。変換・モデル生成は工程名を表示する。

未到着アセットの取得要求は最大64種類・各32接続までまとめ、到着時にローカル／リモートの待機者へ配信する。約30秒の無通信、転送エラー、ハッシュ不一致は失敗表示にし、再取得はRetryから行う。遅れて届いた進捗で失敗表示を消さない。進捗・成功・失敗は一時状態であり、保存やworld_revisionに含めない。ファイルが未到着でもSlotとColliderは存在し、到着後は同じ物体に描画を付ける。アセットの恒久的な保存先は引き続きホストのキャッシュで、配布方式はホスト経由のまま。

描画はGodotの[GLTFDocument](https://docs.godotengine.org/en/stable/classes/class_gltfdocument.html)でバッファから生成する。GLB内の階層・材質は描画ノードに残すが、現状のECSでは1ファイルを1オブジェクトとして扱う。ファイルへのリンクとファイル本体は分かれているため、ワールドを別環境へ移すときにはsnapshotとアセットフォルダーを一緒に持ち出す。

FBXは[FBXDocument](https://docs.godotengine.org/en/stable/classes/class_fbxdocument.html)とFBXStateでバックグラウンド読み込みし、生成したシーンをGLBへ再梱包する。動画はFFmpeg 9.0.2で最大720p・30fpsのTheora/Vorbisへ変換する。各GodotがVideoStreamPlayerで再生し、その動画TextureをQuadMeshへ貼る。動画はColliderあり・Solidなしで生成する。

## 動画の再生同期

動画のSlotに`MediaPlayback` Componentを付け、`playing`、`looping`、操作時点の`position_seconds`、その時点のホスト時刻`anchor_seconds`を保存する。`Visual::Video`にはアセットID・幅・高さ・再生時間を持たせる。`playback`フィールドのrevisionを使い、`WorldCommand::Playback`のPlay/Pause/Restart/Seek/Loopを既存の検証・確定・差分配信に通す。通常のEditでホスト時刻を直接書き換えることはできない。同じrevisionへの同時操作は先着が成功する。

ホストの時計は起動時のUnix時刻に単調増加時計の経過秒数を加えたもの。Engineが`MediaClock` Resourceを更新し、Systemが操作時の値を確定する。時計の更新だけではworld_revisionやcommitは発生しない。Godotは2秒ごとに`clock_ping`を送り、ホストの`clock_pong`と往復時間の中点から時計の差を推定する。ゲストEngineはこの往復をホストへ中継する。要求IDと接続世代を照合し、10秒で未応答の要求を破棄する。接続変更時には補正値を取り直す。

再生中の期待位置は`position_seconds + (現在のホスト時刻 - anchor_seconds)`。ループでは再生時間で折り返し、ループなしでは末尾で止める。Godotは新しい操作を受け取ったときと、再生中のずれが0.25秒を超えたときにシークする。一時停止中は0.025秒以上のずれを補正する。時計が未取得の間は止めておき、取得後に現在位置へ合わせる。途中参加やアセットのダウンロード完了も同じ処理なので、動画の先頭から再生し直さない。映像と音声のデコード・表示はGodotに残す。これは時間を合わせる方式で、画面間のフレームロックではない。

インスペクタのPlay/Pause・Restart・シークスライダー・Loopは全員に反映される。スライダーはドラッグを離した時点で送信する。再生状態を含むsnapshotでは同じ時系列を引き継ぎ、旧6フィールドの保存ファイルでは動画に初期再生状態を追加して復元する。再生中のsnapshotを時間を置いて読み込むと、その経過時間も進んだ位置になる。

## 切断と保存状態

切断時はゲストの編集を止め、アダプターは2秒間隔で再接続する。強制終了直後は中継に古い接続が残ることがあり、その失効を待って再参加する。未確定の要求は自動再実行しない。10秒以上結果が来ない編集は結果不明として再同期し、現在値を確認してもらう。

起動時の `--load <snapshot.json>` で保存済みsnapshotを検証して読み込める。`scripts/start-demo.ps1 -WorldFile <path>` はこれをホストへ渡す。`--save <world.json>` を指定すると、ホストだけが約2秒間隔で変更されたrevisionのsnapshotを保存する。ファイルの書き込みと同期は別スレッドで行い、一時ファイルのrenameで置き換える。直前の正常なファイルは `.bak` に残し、主ファイルが読めない場合はバックアップから復元する。壊れた主ファイルをバックアップへ上書きしない。両方とも無効なら起動を中止する。

従来の固定ルーム版は `.run/world.json` と `.run/assets/shared/` を使う。自動参加では中継のhost割当を確認してから復元するため、ゲストは自分の古い保存状態を読み込まず、保存もしない。ローカルUIの `save_world` に対して `save_result` を返し、StopLegacy.batは成功を確認してから記録したプロセスを止める。失敗・タイムアウト時はアプリを残して再試行できる。Ctrl+Cでも最終snapshotを保存する。強制終了時は最後の保存後の編集が失われる場合がある。セッション・ライブラリ版は管理APIからルームとホストIDを取得し、本人確認後に編集を開始する。保存・再開のUIと配布版の保存先は [公開サーバーの構成](deployment.md) を参照。ホストの自動交代は未実装。

## 検証

単体テストではSystem実行前後、同一tickの競合、複製の独立性、最後の箱の削除、不正snapshot・差分の原子性、変更していないEntityの維持、姿勢の統合と永続状態との分離を確認する。キャラクターでは斜め移動の速度、視点基準の方向、ジャンプの押下判定、重力・着地、入力失効、編集の処理回数からの独立性、空中でのsnapshot復元、遠隔アバターを再シミュレーションしないことを確認する。

統合テストではローカル中継と独立した2つのEngineを使い、双方向編集、生成・複製・削除、参加者の出現・移動・名前・退出、アダプター再接続を確認する。Godotのモデル読み込み・編集フォーム・生成ボタン・削除確認・表示更新も含める。移動中の双方向プレビュー、箱と画像の描画・補間、キャンセル・失効・古いrevisionの拒否、確定値が途中で変わらないことも検証する。LANやインターネット越し、大人数での性能は未検証。

`tests/persistence-integration.mjs` はホストの自動保存、ゲストの保存ファイル保持、強制終了後の自動ホストによる復元、ポート変更後のモデル取得、Windows PowerShellの終了時保存、バックアップ復元、保存失敗時に終了せず再試行できることを確認する。
