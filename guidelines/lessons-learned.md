# 不具合から得た知見を次の実装へ引き継ぐ

確認日：2026-10-24。以下は再現・修正を確認した記録。
現在の操作規則は [3Dパネルの実装指針](world-space-panels.md) を参照する。

## レーザーだけ延ばしても遠くの物体は保持できない

レーザーの見た目、Rustの当たり判定、保持開始時の距離、保持中のホイール距離、3Dパネルのポインターは別の上限を持っていた。Rust側の6mだけを延ばさないと、遠い物体を指しても保持できない。ワールドの有効座標±1000mを基準に、判定と保持距離を4000mへ揃える。無限大をraycastへ渡さず、有限の全域到達値を使う。[grabbing.rs](../crates/core/src/grabbing.rs) の遠距離テストと描画付きの [interaction-reach-integration.mjs](../tests/interaction-reach-integration.mjs) で、30m以上の右クリック保持と配置を確認する。確認日：2026-10-25。

## 反射面を同じワールドのカメラで描くと再帰する

鏡は共有の`MeshRenderer`の形状として保存し、反射画像はGodotのローカルなSubViewportで生成する。反射カメラが鏡面を再び描くと、自己参照や複数鏡の再帰で負荷と表示が不安定になる。鏡面を専用の描画レイヤーへ置き、反射カメラから除外する。鏡の枠は通常レイヤーに残す。見えている鏡は枚数制限を設けず、Godotの描画フレームごとに更新する。描画付きの [creation-integration.mjs](../tests/creation-integration.mjs) で、鏡の前の物体が反射に映ることを確認する。関連コード：[mirror_renderer.gd](../client-godot/mirror_renderer.gd)。確認日：2026-10-25。

## 鏡のTransformだけ更新しても反射が遅れる

保持中の鏡面は [render_hierarchy.gd](../client-godot/render_hierarchy.gd) が受け取ったプレビュー姿勢で滑らかに移動するが、反射用SubViewportを一律66ms間隔で更新すると映り込みだけが段階的に変わる。鏡が動いた場合だけ更新間隔を外しても、静止した鏡の前で視点やほかの物体が動くと反射が遅れる。反射カメラはRustの確定済みTransformではなく、実際に表示したGodotノードの`global_transform`を参照し、見えている各鏡を描画フレームごとに更新する。描画付きの [creation-integration.mjs](../tests/creation-integration.mjs) で、鏡と視点を同じフレーム内で別々に動かしても反射カメラが追従することを確認する。関連コード：[mirror_renderer.gd](../client-godot/mirror_renderer.gd)。確認日：2026-10-25。

## 鏡の裏側で反射が更新されない

反射更新は鏡のローカルZ方向を表として距離を判定していた。裏側の視点では距離が負になって更新されず、面も裏側を描画しなかった。反射カメラの法線を視点側へ向け、距離の絶対値で更新を判定し、反射材質を両面描画にする。前後それぞれに色付きの物体を置く描画付き [creation-integration.mjs](../tests/creation-integration.mjs) で、両方向の反射テクスチャを確認する。以前の配布版には同時更新を4枚に制限する処理もあり、上限外の鏡は未描画になり得た。この制限は既に外している。関連コード：[mirror_renderer.gd](../client-godot/mirror_renderer.gd)。確認日：2026-10-26。

## FPSの自分のアバターを隠すと撮影と鏡からも消える

自分のアバターのNodeを`visible=false`にすると、通常のFPS画面だけでなく、同じ3Dワールドを描くMirrorとCameraのSubViewportからも消える。自分のアバターのVisualInstanceを専用レイヤーへ置いてNodeは表示したままにし、FPSの主カメラの`cull_mask`だけからそのレイヤーを外す。反射カメラと撮影カメラはそのレイヤーを描く。名前ラベルは自分の視点では表示しない。描画付きの [creation-integration.mjs](../tests/creation-integration.mjs) で、撮影画像と鏡の反射画像をアバターレイヤー有無で比較し、FPS主カメラでは非表示になることを確認する。関連コード：[avatar.gd](../client-godot/avatar.gd)、[main.gd](../client-godot/main.gd)。確認日：2026-10-26。

## Debug.batを押し直しても古いEngineが残ると鏡が白い

`Debug.bat`は`-Build`を渡すが、既存のEngineが動いていると`start-local.ps1`がビルド前に画面を再表示して終了していた。Godotの新しいソースだけを別途起動すると、Mirror形状の古いSnapshotには`Mirror`コンポーネントがなく、新しいGodotは反射を有効にできず、平面の素の色だけを表示する。実際のデバッグセッションでは3枚のMirrorがすべてコンポーネントなしで、保存ワールドは形式1だった。`-Build`を受けたら既存セッションを保存してEngineを停止し、ビルドしてから両方を起動する。保存ワールドのコピーを新しいEngineで開くと、3枚とも`Mirror.Enabled=true`に移行したことを確認した。関連コード：[start-local.ps1](../scripts/start-local.ps1)、[復元処理](../crates/core/src/replication.rs)。確認日：2026-10-26。

## 3D作成パネルの道具を形状カードに増やすと操作が隠れる

4列の形状カードを8個から9個へ増やすと、固定寸法の共有3Dパネルで3行目が生じ、下部の作成ボタンが操作できなくなった。カメラのように専用の操作画面を持つ道具は形状カードではなく、フッターのプリセット入口に分ける。カード数やボタン配置を変えたら、単体のレイアウト表示だけでなく、[creation-integration.mjs](../tests/creation-integration.mjs) の描画付き入力テストで、パレットの選択から作成まで確認する。関連コード：[creation_panel.gd](../client-godot/creation_panel.gd)。確認日：2026-10-25。

## お絵描きアバターの表裏がちらつく

アバターGLBはBody・Head・手足・尻尾の各部位について、同じ位置に逆向きの`Front`と`Back`の面を持つ。Godot側で共通材質を`CULL_DISABLED`にすると、両方の面をそれぞれ表裏から描くため、同じ深さの描画が重なってちらつく。

GLBの表裏メッシュは削らず、共通材質を`CULL_BACK`にしてカメラを向く側の面だけ描く。テクスチャ差し替えやアニメーションはそのまま使う。新たに平面アバターを取り込む場合も、モデルがすでに表裏の面を持つか確認してから両面材質を選ぶ。

- 再現・確認：GLBの14メッシュが7組の`Front`/`Back`であることと、材質の`doubleSided`が指定されていないことを確認する。描画付きの [avatar-integration.mjs](../tests/avatar-integration.mjs) でアバター画像の変更・復元・後からの参加を確認する。
- コード：[avatar.gd](../client-godot/avatar.gd)。確認日：2026-10-25。

## 共同編集のApplyはゲストの通常コマンド結果にも登録する

ノードグラフの下書き操作は`script_graph_result`で返すが、`Apply graph`はRustの`ScriptAction::Update`として実行され、結果は通常の`result`になる。ゲストがグラフ要求をホストへ転送するだけでは、戻ってきた`result`を受け取る保留コマンド表にIDがなく、UIへの応答が落ちる。ゲストのApply時に通常コマンドの保留表へ登録し、ホストで下書き世代・版が古いときも`result`で拒否を返す。

- 再現・確認：ゲストから古い下書き版のApplyを拒否し、続いて最新版をApplyする。ホスト・ゲストのSnapshotと保存・復元後のScript sourceが一致し、未Applyの下書きが保存されないことを [script-documents-integration.mjs](../tests/script-documents-integration.mjs) `--rendered` で確認する。
- コード：[graph_documents.rs](../crates/engine/src/graph_documents.rs)、[main.rs](../crates/engine/src/main.rs)。確認日：2026-10-25。

## ノードグラフのField参照は送信前に整数のAddressへ戻す

Godotが共有SnapshotのJSONを読むと、Rustでは整数の`Address.local_id`が`2.0`のような浮動小数点になる。ノードグラフで数字だけを変えてもApplyは既存の`ref_paint`を再送するため、ホスト直結ではRustの`u32`への復元が失敗し、`Invalid script parameter: ref_paint`になる。ゲスト経由ではGoアダプターがJSONを再構築して`2.0`を`2`へ直していたため、同じUIテストが通り、ホスト側だけの不具合を隠していた。

クライアントは`field_ref`の形を検査し、送信時に`local_id`を整数へ正規化する。受け取り時には対象Slot・Component・Fieldが表示中のワールドにあることも確認する。不正な値は送らず、`Clear`または再ドロップを案内する。狭い共有3Dパネルでは参照欄をラベルの下に置き、操作と現在の結び先を見えるようにする。最終的な値の検証はRust側に残す。

- 再現・確認：`local_id`が文字列のField参照を拒否し、有効なColor参照を3Dパネルへ渡してから、ホスト直結で`ref_paint`付きグラフの数字を`2`から`3`へ変更する。Rustで`2.0`の参照を拒否することも確認する。[script-documents-integration.mjs](../tests/script-documents-integration.mjs) と [ugc-integration.mjs](../tests/ugc-integration.mjs) の `--rendered` を使う。
- コード：[script_editor.gd](../client-godot/script_editor.gd)、[shared_script_smoke.gd](../client-godot/shared_script_smoke.gd)、[lib.rs](../crates/scripting/src/lib.rs)。確認日：2026-10-25。

## クライアントのウィンドウを広げると2D UIまで拡大する

Godotの`window/stretch/mode="canvas_items"`では、基準の1280×800からウィンドウを広げると2Dのキャンバス全体が拡大される。チャットやトップバーをアンカーで端に配置していても、表示文字と部品の寸法が大きくなり、使える領域が増えない。

デスクトップクライアントは`disabled`にし、1 UI単位を画面の1ピクセルとして扱う。ウィンドウの大きさに応じた位置と領域はControlのアンカーで決める。UI倍率を将来設定として追加する場合は、ウィンドウサイズに自動連動させず、明示的な個人設定にする。

- 再現・確認：ウィンドウを256×128拡大し、HUDの領域が同じだけ増えること、トップバーの高さとチャット幅が一定で、チャットが右端に追従することを描画付きテストで確認する。
- コード：[project.godot](../client-godot/project.godot)。テスト：[hud_smoke.gd](../client-godot/hud_smoke.gd)、[hud-integration.mjs](../tests/hud-integration.mjs)。確認日：2026-10-25。

## ワークショップ作品のフィールド参照が配置後に古い個体を指す

保存版には元のObjectRootのinstance IDを持つ`field_ref`が残る。ライブラリから配置するとObjectRootとRendererは新しいIDへ変わるが、Scriptの参照パラメータをそのままコピーしていたため、配置した色・回転などへのDrive/Writeが元の個体を探して失敗した。Scriptの経過時間も保存時の時計を引き継いでいた。

単一オブジェクトの作品を配置するとき、参照先instance IDが保存版自身と一致するパラメータだけを新しいIDへ置き換える。外部への参照は勝手に付け替えない。Scriptの時計と一時Stateを配置時に初期化する。将来、子スロットを含む作品を保存できるようにする際は、作品内の全IDに対して対応表を使う。

- 再現・確認：自身のColorへDriveするScript付き作品を配置し、参照先ID、Drive結果、保存復元を確認する。
- コード：[lib.rs](../crates/core/src/lib.rs)。テスト：[tests.rs](../crates/core/src/tests.rs) の `placing_library_item_rebinds_its_own_field_references`。確認日：2026-10-25。

## グラフにノードを追加すると、移動済みノードが元の位置へ戻る

GraphEdit上でノードをドラッグしてから別のノードを追加すると、表示を作り直す際に古いJSON上の座標を使い、移動済みノードが元の配置へ戻っていた。GraphNodeの`position_offset`はドラッグ中のローカル編集状態で、共有ソースへ保存する前にも追加・削除が起こる。

ノード移動の終了時と再描画の前に、表示中の各`position_offset`をグラフドラフトへ取り込む。再描画時はスクロール位置と倍率を保ち、右クリック追加ではクリック位置をグラフ座標へ変換して新しいノードだけの位置を決める。共有のグラフソースを更新するのは引き続きApply時だけ。

- 再現・確認：ノードを移動し、空白の右クリックからDisplayを追加しても既存位置が変わらないこと。2Dウィンドウと、カーソルを解放した3Dパネルで右クリックが届くこと。
- コード：[script_graph.gd](../client-godot/script_graph.gd)、[world_panels.gd](../client-godot/world_panels.gd)。テスト：[script_smoke.gd](../client-godot/script_smoke.gd)、[shared_script_smoke.gd](../client-godot/shared_script_smoke.gd)を描画付き統合テストで実行。確認日：2026-10-25。

## パネルを開くと歩けなくなる

最初の新規作成パネルは3Dに描画していたが、表示中はHUD全体を入力停止の対象にしていた。
見た目が3Dでも、入力処理は移動を止めるダイアログになっていた。

新規作成を共有WorldPanelへ移し、インスペクタと同じ入力経路を使うようにした。
「3Dの道具を開いていること」と「個人のメニューが移動入力を止めること」を同じフラグで扱わない。

- 確認：パネルを開いたままの移動・ジャンプ・保持・回転・拡縮、共有と途中参加。
- コード：[world_hud.gd](../client-godot/world_hud.gd)、[world_panels.gd](../client-godot/world_panels.gd)。
- テスト：[creation_smoke.gd](../client-godot/creation_smoke.gd)。修正：`ba2a9bc`。

## チャットから3D操作へ戻っても歩けない

2Dのチャット入力欄を選んだ後にワールドをクリックすると、マウスは捕捉されてパネルも薄くなるが、`LineEdit`のキーボードフォーカスが残っていた。移動入力は`gameplay_keyboard_blocked()`で止まり、文字は入力欄へ届き続ける。

マウス捕捉に伴ってチャットパネルを入力停止にするとき、チャット欄のフォーカスも解除する。パネルを収納するときも同様に解除する。見た目やマウス入力の切り替えだけでキーボードフォーカスが消えるとは仮定しない。

- 再現・確認：チャット欄をクリックし、ワールドをクリックしてFPSへ戻り、移動キーで実際に位置が変わること。
- コード：[social_panels.gd](../client-godot/social_panels.gd)。テスト：[hud_smoke.gd](../client-godot/hud_smoke.gd)を[描画付き統合テスト](../tests/hud-integration.mjs)で実行。修正：2026-10-24。

## FPSでボタンを押すとカーソルに切り替わる

ワールドパネルの左ボタン押下で、無条件にマウスを解放していた。
さらにドラッグ中の移動イベントをUIだけで消費すると、視点も動かなくなった。

FPS中は押下・ドラッグでも捕捉を維持し、必要な移動イベントを視点操作へも渡す。
UIの入力先とOSのマウス捕捉を別々に管理する。

- 確認：視点中央でのタブ・ボタン操作、ドラッグ中の視点移動、TPSのカーソル操作。
- コード：[world_panels.gd](../client-godot/world_panels.gd)。
- テスト：[panels_smoke.gd](../client-godot/panels_smoke.gd)、[creation_smoke.gd](../client-godot/creation_smoke.gd)。修正：`cae589d`。

## 余白を押してもNameの編集が残る

入力欄以外のUIはフォーカスを受け取らないため、前の入力欄にキーボード入力が届き続けた。
カメラの移動で交点が変わった場合、UIのホバー状態も更新が必要だった。

左クリックを転送する前に、実際の交点でホバーを更新する。
以前の入力欄やその子を押していない場合は、フォーカスを外してから押下を渡す。
解除時には押下対象の記録も消す。

- 確認：Name・Color・Positionの選択、Colorの入力でNameが変わらないこと、余白で編集リースが終了すること。
- コード：[world_panel.gd](../client-godot/world_panel.gd)、[world_panels.gd](../client-godot/world_panels.gd)。
- テスト：[panels_smoke.gd](../client-godot/panels_smoke.gd)。修正：`101e31c`。

## 見出しの上では右クリックでパネルを持てない

フィールド名などの参照部品が右クリックを先に受け取り、パネル保持と競合していた。
保持中に同じ部品の上へ移ると、解除イベントまでUI側が受け取る可能性もあった。

通常の右クリックは保持に統一した。参照はCtrl＋右クリック、値のコピーはCtrl＋Shift＋右クリックに分けた。
保持中の解除を参照部品へ渡さない。修飾キー付きの操作でも、古いホバーではなく現在の交点を確認する。

- 確認：見出しからの保持、参照を誤取得しないこと、修飾キーを押した状態での解除、参照の受け渡し。
- コード：[reference_chip.gd](../client-godot/reference_chip.gd)、[world_panels.gd](../client-godot/world_panels.gd)。
- テスト：[panels_smoke.gd](../client-godot/panels_smoke.gd)、[reference_smoke.gd](../client-godot/reference_smoke.gd)。修正：`893d5b8`。

## 配布用アバターの再インポートが不安定になる

配布用コピーにはGodotの変換済みリソースを入れていたが、対応するインポートのチェックサムを入れていなかった。
再インポート時、GLBとそこから生成するテクスチャの処理でファイル読み込みエラーを観測した。

変換済みリソースと一緒に対応する`.md5`をコピーするようにした。
インポートキャッシュを配布する場合は、変換後のファイルだけで完結すると考えず、元データとメタデータも揃える。

- 確認：配布プロジェクトのGodotインポート、ZIP展開後の起動、ファイルハッシュ。
- コード：[copy-avatar.ps1](../scripts/copy-avatar.ps1)。修正：`26e8238`。

## 親を持って動かすと子がカクカク追従する

親だけを描画フレームごとに補間し、子にはEngineから届いたワールド座標を直接設定していた。
Rustの階層計算が正しくても、親と子で表示する時点が違うと相対位置がずれる。

Rustは描画用フレームに、親のアドレス、Drive適用後のローカルTransform、元のObject revisionを含める。
Godotはその表示用コピーを親から順に評価し、補間後の親の位置・回転・スケールから子を配置する。
子を直接保持している場合は、その子の表示位置を基準に孫を配置する。
共有状態・衝突判定・保存は引き続きRustが担当し、補間した表示値を共有状態へ書き戻さない。

描画用フレームには検査用の評価済みワールド座標も残すが、子の描画位置には直接使わない。
親の付け替えや編集後に古いフレームが残っていても、アドレス・親・revisionが一致しない値は適用しない。
GodotのMeshはピック判定やパネル入力との互換性のため同じ階層へ置き、専用の表示階層で求めた姿勢を反映する。
非等方スケールと回転の合成もRustのTRS規則に合わせ、Godotのノード階層によるシアーを持ち込まない。

- 確認：30／60／144 FPSでの親・子・孫の追従、回転・拡縮、子の直接保持、ギズモ、Drive、古いフレームの拒否、親の付け替え・削除。
- 通信・保存の確認：ゲストの親移動プレビューをホストで受信、途中参加、保存復元。共有パネルの入力・保持も回帰テストで確認。
- コード：[render_hierarchy.gd](../client-godot/render_hierarchy.gd)、[hierarchy.rs](../crates/core/src/hierarchy.rs)。
- テスト：[render_hierarchy_smoke.gd](../client-godot/render_hierarchy_smoke.gd)、[hierarchy-integration.mjs](../tests/hierarchy-integration.mjs)。確認日：2026-10-24。

## 親子付けした子をギズモで動かすと飛ぶ

ギズモはワールド座標でドラッグするが、子Slotの`position`フィールドは親を基準にしたローカル座標。ドラッグのワールド座標をそのまま共有フィールドへ送ると、Rustがさらに親の位置・回転・スケールを合成し、子が意図しない場所へ移動する。ドラッグ中の旧インスペクタ表示にもワールド座標が入っていた。

プレビューの配送と描画はワールド座標のままにし、フィールド表示と確定Editだけを、表示中の親の姿勢の逆変換でローカル座標へ戻す。親にローカルDriveがある場合も、そのクライアントで見ている姿勢を使う。親のスケールがゼロに近く逆変換できない場合は確定しない。

- 再現・確認：親を移動・拡縮し、Driveで回転させた状態で子の実ギズモをドラッグする。保存した子のローカル`position`が逆変換結果と一致することを確認する。
- コード：[main.gd](../client-godot/main.gd)の`gizmo_local_position`。テスト：[hierarchy_smoke.gd](../client-godot/hierarchy_smoke.gd)を[描画付き統合テスト](../tests/hierarchy-integration.mjs)で実行。修正：2026-10-24。

## Rotator付きの親から子を離すと飛ぶ

親がRotatorで回る間、つかみ操作はDrive適用後の子のワールド姿勢をプレビューする。従来の解除コマンドはそのワールド姿勢を送り、ホストがDrive適用前の親姿勢で子のローカル値へ変換していた。確定後に親のDriveが再び合成されるため、子が飛んだように見える。

解除した端末で、表示中の親姿勢を使ってワールド姿勢をローカル値へ変換し、`TransformCommand`の`space: parent`で確定する。ホストはこの値をそのまま共有の基準値へ保存する。従来の`space: world`は、Driveを伴わないワールド座標指定として維持する。つかみ中のプレビューとDrive結果は共有値に書き戻さない。

- 再現・確認：親にRotatorを付け、子をつかんで移動し、回転中に離す。解除前後の表示位置、ホストとゲストの共有値、次のDriveフレームでの表示を確認する。
- コード：[grabbing.rs](../crates/core/src/grabbing.rs)、[hierarchy.rs](../crates/core/src/hierarchy.rs)、[systems.rs](../crates/core/src/systems.rs)、[main.rs](../crates/engine/src/main.rs)。テスト：`releasing_child_of_rotator_keeps_its_visible_position`、`released_child_uses_driven_parent_pose_for_local_transform`。修正：2026-10-24。

## Remove Scriptを押すと操作できなくなる

3Dインスペクタ内のScriptエディタが、削除確認にConfirmationDialogを使っていた。
SubViewport内の排他的なWindowがメイン画面の入力を止め、レーザーで確認ボタンを押せなくなった。
削除確認は同じパネル内のControlで表示する。キャンセル・確定も通常のパネル入力で操作し、確認中の移動を止めない。
対象のScript、オブジェクト、revisionが変わった場合は確認を取り消す。

- 確認：FPSの実入力で削除確認を開く、確認中に歩く、キャンセル、再度開いて削除、その後に新規作成する。
- コード：[script_editor.gd](../client-godot/script_editor.gd)。テスト：[shared_script_smoke.gd](../client-godot/shared_script_smoke.gd)、[script-documents-integration.mjs](../tests/script-documents-integration.mjs)。

## 新規作成パネルがインスペクタの奥に隠れる

新規作成はカメラから2.6m、インスペクタは2.1mの位置に出していた。
インスペクタを見ながら新規作成すると、手前のインスペクタが新しいパネルを隠し、クリックも遮っていた。
パネルを開くときは視線上の既存パネルを調べ、見つかった場合はその手前へ出す。
これは表示用の出現位置で、生成したパネルの共有・保持・寿命は既存のWorldPanelに従う。

- 確認：インスペクタを見たままリングメニューから新規作成し、FPSで作成ボタンを押して物体を置けること。
- コード：[world_panels.gd](../client-godot/world_panels.gd)。テスト：[shared_script_smoke.gd](../client-godot/shared_script_smoke.gd)。

## 配布の基準をGitのHEADだけで決めない

前回の配布に含まれていたセッション管理・ライブラリ・ログ窓が未コミットだったため、HEADだけから作った次の配布で欠落した。
ビルド前に前回のbuild-info.jsonと起動スクリプトを確認し、維持する機能を決める。
作業ツリーの変更を除外する場合も、その機能を前回すでに配布していないか確認する。未コミットであることだけを除外理由にしない。
梱包後は前回の機能、今回の修正、起動方法を配布物で検証する。ZIPと実行用フォルダーを別バージョンで作り、利用中のフォルダーへ上書きしない。

- 確認：セッション管理、ライブラリ、表示されるEngineログ、再起動、3Dパネルの入力。
- 比較元：`Kokoverse-Windows-x64-20261024-020925`。欠落した配布：`Kokoverse-Windows-x64-20261024-023500`。確認日：2026-10-24。

## Start.batの多重起動ではプロファイルの割り当てを排他制御する

Start.batは呼び出すたびに別のインスタンスを起動し、空いているdefault、instance-2、instance-3…を使う。
起動処理全体を排他制御し、同時に押しても同じ枠を選ばないようにする。明示的なプロファイル起動とも競合しないよう、選んだ枠のロックを取得してから起動する。
稼働判定には記録された実行ファイル・PID・起動時刻を使う。記録が読めない枠は避け、保存データを上書きしない。
終了した枠は既存データを残して再利用する。`start-online.ps1 -Profile ...` の明示的な再接続とは入口を分ける。

- コード：[start-instance.ps1](../scripts/start-instance.ps1)。
- 確認：[multi-instance-smoke.ps1](../tests/multi-instance-smoke.ps1)で実際のStart.batを同時に実行し、個別停止と保存データを残した再利用を検証する。

## ライブラリの所有者をワールド内の共同操作の許可に使わない

管理API版では、別のユーザーが配置したコライダーを右クリックしても保持できなかった。
ライブラリ用の所有者チェックが、保持開始・ローカルと遠隔のプレビュー・編集確定にも使われていたためである。匿名の従来セッションでは再現しなかった。

ワールド内では作者・所有者・ライセンスによる操作制限を外し、参加者の確認、値の検証、revisionと編集リースによる競合チェックを維持する。
保持や移動で作者・所有者を変更しない。作品の来歴を示す情報と操作の許可は別に扱う。

- 確認日：2026-10-24。
- 回帰テスト：[world-collaboration-integration.mjs](../tests/world-collaboration-integration.mjs)。異なるHNのホストとゲストで相互の保持・回転・拡縮・編集・構造変更を確認し、古いrevisionの書き込みは拒否する。
- 今後の方針：[作品の境界と権利の計画](../docs/item-boundaries-and-rights.md)。ライブラリ側の制限解除は次の記録を参照。

## ライセンスの保留はライブラリの画面とAPIの両方に反映する

ワールド内の制限だけを外しても、ライブラリの保存・取得・共有・再公開には所有者や利用条件のチェックが残っていた。
Engineの保存前チェック、管理APIの各操作、公開・共有ボタンの無効化を合わせて解除する。古い保存版や公開情報の条件も、現在の操作を拒否する根拠には使わない。

他人のモデルを保存するとき、素材がサーバーに存在していてもローカル未取得を理由に失敗する経路があった。ローカルにある素材はアップロードし、すでにサーバーにある素材は参照で保存する。存在しない素材の参照はサーバーで拒否する。
作者・派生元・保存版は維持し、個人の一覧や公開停止の管理、認証、形式・ハッシュ・容量の検証は残す。

- 確認日：2026-10-24。
- 確認：管理APIの単体テストで古い利用条件を持つデータを検証。`tests/managed-integration.mjs --rendered`で別HNによる保存・共有・再公開、素材取得、Godotの公開画面を確認する。

## 差分配布のパス検証は実際の梱包内容で試す

少数の仮ファイルを使った更新テストは通ったが、実際の配布物からマニフェストを作ると、依存ライセンスの版名に含まれる `+` を危険なパスとして拒否した。パスの許可文字を推測で狭めると、配布物にある正当なファイルも更新できない。

マニフェストは梱包済みファイルの実パスとハッシュから作り、ルート外への移動や `..` を拒否する。許可文字は実際の梱包内容で確認し、ランチャー側も同じ規則で検証する。利用者の `.run/` は版の外に置き、更新対象の一覧へ含めない。

- 確認日：2026-10-24。実配布物の全ファイルでマニフェスト生成とローカルHTTPからの取得を確認した。
- 回帰テスト：[tester-updater-smoke.ps1](../tests/tester-updater-smoke.ps1)で初回取得、未変更ファイルの再利用、起動中の延期、失敗時の旧版保持を確認する。
- コード：[prepare-tester-feed.ps1](../scripts/prepare-tester-feed.ps1)、[updater.ps1](../tester-launcher/updater.ps1)。

## 差分ランチャーの一時展開パスを短くする

実配信の初回取得で、版名とランダム値を連結した一時フォルダーにさらに配布ファイル名と `.download.gz` を加えると、Windowsのパス長制限を超えた。検証用の深い場所では `client-godot/.godot/editor/` のファイルで失敗し、通常のDownloads配下を想定した長さでも一部のインポート済み素材とライセンスが制限を超える。

一時フォルダーは版名を含めず、短い名前とランダム値だけにする。最終版のフォルダー名とマニフェストの版名は維持する。実際の配布ファイル名を含めた最長パスで確認し、ランチャーのテストにも長い相対パスを含める。

- 確認日：2026-10-24。`tester-v2026.10.24.1` のR2初回取得で再現。修正版は [tester-updater-smoke.ps1](../tests/tester-updater-smoke.ps1) で初回取得と更新を確認する。
- コード：[updater.ps1](../tester-launcher/updater.ps1)。

## 動画ツール導入時にZIP全体を展開しない

`Install-Video-Tools.bat` の導入画面が `Web 要求を書き込んでいます` で長く止まる。約115MBのFFmpeg ZIPを `Invoke-WebRequest` で単一配布元から取り、49項目を全部展開していた。実際に必要なのは `ffmpeg.exe`、`ffprobe.exe`、`LICENSE` の3項目だけだった。

同じSHA-256のGyanD公式GitHubリリースを第一候補とし、元の配布元を予備にする。`curl.exe` で取得し、ZIPと実行ファイルのハッシュを照合する。ZIPはユーザーのローカルキャッシュに置いて版をまたいで再利用し、必要な3項目だけを展開する。導入済みでハッシュが合う場合は再取得しない。配布ZIPにはFFmpeg本体もキャッシュも入れない。

- 再現・確認：旧スクリプトの取得処理とZIP内49項目を確認。GitHubミラーのZIPのSHA-256が元の固定値と一致することを確認。隔離フォルダーへの初回導入、再実行、別フォルダーへのキャッシュ再利用でファイルのハッシュを確認する。
- コード：[setup-media.ps1](../scripts/setup-media.ps1)、[package.ps1](../scripts/package.ps1)。確認日：2026-10-24。

## 右クリック保持中のミドルクリックで保持を解除しない

FPSで3Dパネルを右クリック保持したままミドルクリックすると、保持が消えてリングメニューが開いた。ミドルクリックの共通処理が保持状態を確認せずにメニューを開き、`release_cursor()` が `cancel_hand_grab()` を呼んでいた。

右ボタン押下中、保持開始待ち、保持中、保持終了待ち、および右ボタンでの視点操作中は、ミドルボタンの押下・解放をリングメニューへ渡さない。右ボタンを離すまで保持を続け、通常のミドルクリックでは従来どおりメニューを開く。

- 再現・確認：3Dパネルを右クリックで保持し、右ボタンを押したままミドルボタンを押して離す。保持・マウス捕捉が続き、リングが閉じたままであることを確認してから右ボタンで確定する。修正前はこの順序で [panels-integration.mjs](../tests/panels-integration.mjs) が失敗した。
- コード：[world_hud.gd](../client-godot/world_hud.gd)、[panels_smoke.gd](../client-godot/panels_smoke.gd)。確認日：2026-10-24。

## 描画部品を持たないSlotでは表示用フィールドを参照しない

空のSlotを作成して選択すると、個人インスペクタの `refresh_fields()` が存在しない `color` フィールドを読んでGodotのスクリプトエラーになった。`MeshRenderer` の有無で `color` の存在を決め、色入力もその場で無効化する。描画ノードはメッシュを持たない状態を許し、選択枠を表示しない。

- 再現・確認：共有作成パネルから空のSlotを作り、選択、2クライアント同期、途中参加を [creation-integration.mjs](../tests/creation-integration.mjs) の `--rendered` で確認する。
- コード：[main.gd](../client-godot/main.gd)、[systems.rs](../crates/core/src/systems.rs)。確認日：2026-10-24。

## Godotから構造コマンドへ渡すAddressは整数化する

インスペクタから `AddComponent` を送ったとき、スナップショット内の `address.local_id` をそのまま再送するとGodotが `1.0` としてJSON化し、Rustの整数フィールドへ復元できず `Invalid edit payload` になった。既存の `app.address_of()` で `local_id` を整数化してから要求Brokerへ渡す。

- 再現・確認：空のSlotを選択し、インスペクタから表示部品を追加・削除する。[creation-integration.mjs](../tests/creation-integration.mjs) の `--rendered` で実入力、ホストの変更、途中参加を確認する。
- コード：[inspector_view.gd](../client-godot/inspector_view.gd)。確認日：2026-10-24。

## 共有Componentの数値は型付きで比較する

Godotのインスペクタから `WorldText` の白色 `[1.0,1.0,1.0,1.0]` を送ると、JSONでは `[1,1,1,1]` になった。Rustでこの生のJSONとComponentの浮動小数点配列を再シリアライズしたJSONを比較すると、意味が同じでも不一致になり追加を拒否した。値域検証の後は `[f64; 4]` に復元して比較する。

- 再現・確認：空Slotに文字を追加し、色を同期、付け外し、途中参加を [creation-integration.mjs](../tests/creation-integration.mjs) の `--rendered` で確認する。整数の色値はRustの [tests.rs](../crates/core/src/tests.rs) でも確認する。
- コード：[lib.rs](../crates/core/src/lib.rs)、[replication.rs](../crates/core/src/replication.rs)。確認日：2026-10-24。

## 描画部品のないSlotも作成位置を明示する

空Slotを原点へ固定して作ると、後から付けた `WorldText` が床に隠れ、部品の追加は成功していても利用者には見えなかった。作成パネルは視点前方の水平位置と目線の高さを渡し、Rustはそのワールド位置を検証してから親Slotの相対座標に変換する。メッシュの有無ではなく、配置と画面での見え方も描画付きテストで確認する。

- 再現・確認：[creation-integration.mjs](../tests/creation-integration.mjs) の `--rendered` で作成・付け外し後に視点をSlotへ向けたスクリーンショットを確認する。親付きSlotのワールド位置は [tests.rs](../crates/core/src/tests.rs) で確認する。
- コード：[creation_panel.gd](../client-godot/creation_panel.gd)、[systems.rs](../crates/core/src/systems.rs)。確認日：2026-10-24。

## 新規Slotと初期Componentは一つの構造操作で作る

新規テキストをSlot作成と部品追加の二要求に分けると、他の参加者が途中の空Slotを見る可能性がある。`CreateSlot` に初期Componentを渡し、Rustが登録表の検証を通して一度に確定する。子テキストは親相対位置を指定する。単独テキストは視点の水平前方へ向ける回転を渡さないと、作成直後に文字の側面だけが見える。

- 再現・確認：単独テキストと子テキストを3Dパネルから作り、[creation-integration.mjs](../tests/creation-integration.mjs) の `--rendered` で表示、同期、途中参加を確認する。不正な初期ComponentでSlotを残さないことと親相対位置、保存復元は [tests.rs](../crates/core/src/tests.rs) で確認する。
- コード：[creation_panel.gd](../client-godot/creation_panel.gd)、[inspector_tree.gd](../client-godot/inspector_tree.gd)、[systems.rs](../crates/core/src/systems.rs)。確認日：2026-10-24。

## 別SlotへのDriveは表示対象とスクリプトの持ち主を分けて追う

従来のDrive表示は、表示対象Slotにスクリプトも付いている前提で制御元を探していた。別Slotの `WorldText` をDriveすると値は届いても、インスペクタから制御元を見つけられない。Drive結果には対象フィールドとスクリプトIDを載せ、UIはそのIDを全Slotのスクリプトから解決する。共有の基準値は変えず、Driveが外れたら表示を基準値に戻す。

コード欄へのフィールドD&Dは、生のSlot IDをLuaへ挿入せず、型付き `field_ref` パラメータを作ってコードから参照する。Rustは登録表のWrite・Drive可否、値の型と範囲、参照が当該スクリプトに登録済みかを確認する。

- 再現・確認：[reference-integration.mjs](../tests/reference-integration.mjs) の `--rendered` で別Slotの位置とWorldTextへのDrive、WorldTextへの共有Write、表示と同期を確認する。[references.rs](../crates/core/src/references.rs) でDriveの非永続性とWriteの確定を確認する。
- コード：[drives.rs](../crates/core/src/drives.rs)、[world_panels.gd](../client-godot/world_panels.gd)、[script_editor.gd](../client-godot/script_editor.gd)。確認日：2026-10-24。

## タグ付き配布ではGodotのUIDを先にコミットする

新しい `.gd` を追加し、対応する `.gd.uid` をコミットしないまま配布を始めると、GodotのインポートがUIDを生成する。梱包開始時の作業ツリーはクリーンでも、ZIPにはタグにないUIDが入る。配布前にUIDをソースへ含め、公開スクリプトは梱包後にも作業ツリーを確認して、新たな生成ファイルがあればpushとR2公開を止める。

- 再現・確認：`script_code_edit.gd.uid` がタグ `tester-v2026.10.24.6` に含まれず、同版のZIPに含まれることを確認した。UIDをコミットした後、[publish-tester.ps1](../scripts/publish-tester.ps1) で再梱包し、作業ツリーがクリーンな状態を確認する。
- コード：[package.ps1](../scripts/package.ps1)、[publish-tester.ps1](../scripts/publish-tester.ps1)。確認日：2026-10-24。

## Engine終了時にGodotが初回接続中でも閉じる

配布版の起動直後にEngineのコンソールを閉じると、Godotがまだ初回TCP接続を完了していない場合がある。接続済みフラグを条件とする終了処理ではこの状態を拾えず、Godotだけが接続を再試行して残った。前回版でも同じ症状を再現した。

`--close-with-engine` の起動では、最初の接続を5秒以内に確立できなければGodotを終了する。接続後に切れた場合は従来の切断処理で終了する。Engineを待って起動するランチャーの用途に限定し、開発用の再接続動作は維持する。

- 再現・確認：[managed-launcher-smoke.ps1](../tests/managed-launcher-smoke.ps1) で配布版の起動、画面再表示、Engineコンソールの×、Godot終了とワールド保存を確認する。
- コード：[main.gd](../client-godot/main.gd) の `connect_engine()` と `_process()`。確認日：2026-10-24。

## 2Dウィンドウのドラッグは入力イベントの画面座標を使う

`root.push_input()` で押下と移動を送るテストでは、`Viewport.get_mouse_position()` が注入した入力位置と一致せず、ウィンドウが画面端まで飛んだ。押下と移動の `InputEventMouse.global_position` を同じ座標系で比較して移動量を出す。複数ウィンドウを重ねた場合は、実際にクリックされた子Controlを含む枠だけを前面へ移す。

- 再現・確認：[hud-integration.mjs](../tests/hud-integration.mjs) の `--rendered` でドラッグ量、サイズ変更、背面ウィンドウのフォーカス、各ウィンドウの独立性を確認する。
- コード：[desktop_window.gd](../client-godot/desktop_window.gd)、[service_panel.gd](../client-godot/service_panel.gd)。確認日：2026-10-24。

## FPSへ戻る入力とドックの退場を同時にする

ワールドのクリックでFPSのマウス捕捉が始まっても、HUDの次フレームの監視だけでドックを閉じると、そのフレーム中はドックが開いた状態のまま残る。カーソル捕捉を確定する入力経路で、同時にドックの退場を開始する。Escでカーソルを解放したときは逆にドックを表示し、個人ウィンドウは勝手に閉じない。

- 再現・確認：[hud-integration.mjs](../tests/hud-integration.mjs) の `--rendered` でEscによるスライド表示、再度Escによる収納、ワールドクリック直後のFPS捕捉と収納を確認する。
- コード：[main.gd](../client-godot/main.gd)、[world_hud.gd](../client-godot/world_hud.gd)。確認日：2026-10-24。

## ワールド復元時にセッションの一時Componentを失わない

`WorldModel::restore()` は保存用snapshotから新しいECSを組み立てるため、新しく追加した一時チャットComponentを引き継がなければ、チャット中のワールド再同期で履歴が消える。参加者と同様に一時履歴を復元先へコピーし、ワールドのrevisionや保存データには入れない。別セッションへ移るときは新しいWorldModelを作り、履歴を持ち越さない。

- 再現・確認：メッセージ投稿後に同じワールドのsnapshotを復元し、履歴が残ること、保存用snapshotが変わらないこと、200件上限を [tests.rs](../crates/core/src/tests.rs) で確認する。2人の送受信と途中参加は [chat-integration.mjs](../tests/chat-integration.mjs) で確認する。
- コード：[session_chat.rs](../crates/core/src/session_chat.rs)、[lib.rs](../crates/core/src/lib.rs)。確認日：2026-10-24。

## インスペクタの入力制限は共有Fieldの検証と揃える

WorldTextの共有Fieldは空文字と80文字までを受け付けるが、共通インスペクタの入力欄は32文字で切り、空文字の変更を送らなかった。Rustの検証だけを見ても制作画面から実際に入力できるとは限らない。Fieldを追加・変更するときは、登録表の型・範囲、Godotの入力Control、2D/3Dからの送信、ホストの確定値、途中参加の復元を一続きで確認する。

Component欄もスキーマの存在だけで表示すると、未装着のMediaPlaybackに空の節が出る。Removeの可否は、Rustが拒否する依存Componentと合わせる。新しいFieldは「編集設定」「読み取り専用の派生値」「専用コマンド」「内部状態」の役割を決めてから表示する。

- 再現・確認：[creation-integration.mjs](../tests/creation-integration.mjs) の `--rendered` で3Dインスペクタから長文入力と空文字への変更、ホストへの同期、途中参加を確認する。[inspector-integration.mjs](../tests/inspector-integration.mjs) の `--rendered` で共通インスペクタの操作を確認する。
- コード：[inspector_view.gd](../client-godot/inspector_view.gd)、[replication.rs](../crates/core/src/replication.rs)。設計上の不足は[Componentとインスペクタの対応](../docs/component-inspector-review.md)に整理する。確認日：2026-10-26。
