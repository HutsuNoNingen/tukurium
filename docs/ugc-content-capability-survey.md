# UGCで作れるコンテンツと必要な部品の調査

調査日：2026-10-24。対象は「Unityのエディター内でGameObject・Component・アセットを組み合わせるように、tukuriumのワールド内で作品を作る」方向性。UnityやResoniteとのファイル互換、C# API互換、全Componentの再現を意味しない。

この調査からの実装順は[UGCパーツを増やす計画](ugc-parts-plan.md)に分けて記載する。

## 比較の前提

UnityではScene中のGameObjectへComponentを付け、素材や設定をPrefabとして再利用する。C#のMonoBehaviourで振る舞いを追加でき、UIには文字・ボタン等のControlとイベント・レイアウトがある。一方、複数人への状態同期は別途Netcode等の設計を要する。画像・3Dモデル・音声は外部ファイルから取り込むことも、種類によってはエディター内で生成することもある。C#やPackageまで含めた「Unityで作れるもの」は固定のComponent一覧に収まらない。出典：[Unityの基本概念](https://docs.unity3d.com/ja/current/Manual/key-concepts.html)、[Prefab](https://docs.unity3d.com/ja/current/Manual/prefabs-introduction.html)、[UI Toolkit](https://docs.unity3d.com/2023.2/Documentation/Manual/UIElements.html)、[アセット](https://docs.unity3d.com/es/current/Manual/AssetWorkflow.html)、[Netcode for GameObjects](https://docs.unity3d.com/jp/current/Manual/com.unity.netcode.gameobjects.html)。

ResoniteではSlotの階層へComponentを付け、ワールド内で編集・保存できる。UIXにはCanvas・RectTransform・Text・Button・Checkbox・Slider・TextFieldがあり、Button EventsをProtoFluxへつなげられる。素材にはモデル、画像、音声、動画、フォント等の取り込みに加えて、多数の手続き的なメッシュ・テクスチャがある。出典：[Slot](https://wiki.resonite.com/Slot)、[UIX](https://wiki.resonite.com/UIX)、[Button Events](https://wiki.resonite.com/Button_Event)、[アセットComponent](https://wiki.resonite.com/Category%3AComponents%3AAssets)、[手続き的メッシュ](https://wiki.resonite.com/Category%3AComponents%3AAssets%3AProcedural_Meshes)、[対応する素材](https://wiki.resonite.com/Supported_Features)。

Resoniteの手元にある **2026.9.18.82** の `FrooxEngine.dll` は、既存のILSpy抽出物 `.run/resonite-inspector/` と型一覧を今回再確認した。DLLのSHA-256は `CD1F57C97408AADA9158E8C9D5C9ADAB05815DBB4A28756AA42681C6B462CEA8` で、[従来の調査](resonite-world-inspectors.md)と同一。抽出済みの `Canvas` に接触対象・押下保持の状態、`Text` に同期される文字列・書式・選択位置のフィールドがあることを確認した。これは型とコードの静的確認であり、今回の実機操作や負荷測定ではない。抽出した製品コードはリポジトリへ追加しない。

## Resoniteで組み合わせる基本部品

これはComponentの全件リストではなく、作者が一つの作品を組み立てる際に使う機能群。Resoniteでは一つのSlotに複数のComponentを付け、子Slotにも別の部品を置く。

| 機能群 | 具体例 | 作品での役割 |
|---|---|---|
| 構造・再利用 | Slot階層、ObjectRoot、Inventory | 複数部品をまとめ、配置・保存・再利用する |
| 形状の供給元 | StaticMesh、BoxMesh、SphereMesh、TubeMesh、BoxArrayMesh | 読み込みメッシュやパラメータ付きの形を用意する |
| 描画と表面 | MeshRenderer、SkinnedMeshRenderer、材質、テクスチャ、Sprite | 形を描き、色・質感・画像を与える。形状と描画は別の参照 |
| 光・画面・効果 | Light、Camera、Skybox、ReflectionProbe、粒子、Trail | 空間の照明、視点、特殊効果を作る |
| 物理と手操作 | Box/Sphere/Mesh Collider、Grabbable、PhysicalButton、TouchButton、Slider、Snapper | 当たり判定、保持、押下、部品の位置合わせを作る |
| UIX | Canvas、RectTransform、Text、Image、Button、Checkbox、Slider、TextField、ScrollRect | ワールド内の表示とフォームを組み立てる |
| 音・メディア | AudioClipPlayer、AudioOutput、ReverbZone、VideoTextureProvider | 再生する素材と、空間へ出力するComponentをつなぐ |
| 値と結線 | ValueField、ReferenceField、DynamicVariableSpace、DynamicValueVariable、各種Driver | 部品間で値と参照を保持し、表示やTransformへ反映する |
| 時間とロジック | Animator、AnimationTrack、Spinner等のDriver、ProtoFlux | アニメーション、反復動作、入力からの処理を作る |
| アバター | AvatarRoot、Rig、SkinnedMeshRenderer、VRIK、Anchor | 身体と装備、追従する部品を組み立てる |

例えば物理的に押せるランタンなら、親Slotの下に形状とMeshRenderer、材質、Collider、Grabbable、Light、ボタン、状態値、処理を組み合わせる。形状と材質のような**アセット**、物体に付く**Component**、保存する**値**、操作から生じる**イベント**が別の役割を持つ。参照とDriverがこれらを結ぶ。

根拠：[Slot](https://wiki.resonite.com/Slot)、[Componentカテゴリ](https://wiki.resonite.com/Category%3AComponents)、[手続き的メッシュ](https://wiki.resonite.com/Category%3AComponents%3AAssets%3AProcedural_Meshes)、[描画](https://wiki.resonite.com/Category%3AComponents%3ARendering)、[操作](https://wiki.resonite.com/Category%3AComponents%3ATransform%3AInteraction)、[UIX](https://wiki.resonite.com/Category%3AComponents%3AUIX%3AInteraction)、[音](https://wiki.resonite.com/Category%3AComponents%3AAudio)、[動的変数](https://wiki.resonite.com/Dynamic_variables/en)、[Avatar](https://wiki.resonite.com/Avatar)。`BoxArrayMesh`・`StaticMesh`・`AudioOutput`・`DynamicVariableSpace` 等の型名は、手元の同一DLLの型一覧にもある。具体的な動作は各Wikiページに基づき、型名だけから推定しない。

## 作れるコンテンツから見た比較

「tukurium」は2026-10-24時点の実装状態。`一部`は作品として完成できるという意味ではない。

| コンテンツ例 | 主な部品・アセット | Unity / Resoniteでの組み立て | tukurium | 主な不足 |
|---|---|---|---|---|
| 展示室・看板・動画館 | 階層、モデル、画像、動画、文字、材質、照明 | 物体を配置し、素材や表示Componentを割り当てる | 一部 | 作品内の文字、材質編集、ライト、複数部品を一作品として保存 |
| 色や動きを変えられるランタン | メッシュ、材質、ライト、スイッチ、スクリプト、効果音 | 入力イベントから状態を変え、表示へ結ぶ | 一部 | ライト、音、複数部品への共有書き込み、作品境界 |
| 操作できるドア・スイッチ盤 | ボタンまたは接触、アニメーション、状態、参照 | ボタンを別の物体の動作へ接続する | 一部 | 対象と操作種別のあるイベント、他の部品への書き込み、汎用アニメーション |
| 共同操作するテトリス | 文字、ボタン、盤面表示、ゲーム状態、タイマー、効果音 | 入力イベントをゲーム規則へ渡し、盤面を再描画する | 未実装 | 作者が作るUI、左右・回転等の入力、盤面データと描画の結合。現状の`on_interact`は単一種の操作 |
| 楽器・ジュークボックス | 音声素材、3D音源、ボタン、再生制御 | 操作で音を鳴らし、必要な再生状態を共有する | 一部 | 独立した音声アセット、音源Component、UGCからの再生イベント。動画音声は現状の機能 |
| 物理パズル・投げる玩具 | Collider、剛体、接触イベント、制約、音 | 物体同士の物理とイベントで遊ぶ | 未実装 | 剛体・物体間の衝突・接触イベント。現状は主にキャラクターと箱の衝突 |
| パラメータで変わる造形 | プリミティブ、メッシュ生成、材質、曲線、粒子 | 生成パラメータやスクリプトで見た目を組み立てる | 一部 | 基本形状はあるが、作者が値を変える生成メッシュ・材質・粒子はない |
| カメラ映像・演出のある空間 | カメラ、ライト、環境設定、アニメーション、エフェクト | 視点や照明と動きを組み合わせる | 一部 | ローカルプレイヤー視点と動画はあるが、作者が置くカメラ・ライト・環境設定はない |
| アバターの衣装・装備 | リグ、スキニング、アニメーション、アタッチ点、素材 | アバターに部品を取り付け、動作と一緒に保存する | 未実装 | 任意アバターの取り込み・装備・ボーン参照・作品単位の保存 |

Unity側の一般的なComponent / Prefabの構成は[公式マニュアル](https://docs.unity3d.com/ja/current/Manual/key-concepts.html)と[Prefabの用途](https://docs.unity3d.com/ja/current/Manual/prefabs-introduction.html)に基づく。Resonite側のUIの具体的な接続は[UIXチュートリアル](https://wiki.resonite.com/UIX_Tutorial)、物理は[Collider](https://wiki.resonite.com/Collider)と[カテゴリ](https://wiki.resonite.com/Category%3AComponents%3APhysics)を参照。tukuriumの状態は[ランタイム](runtime.md)、[操作](inspector-controls.md)、[UGCスクリプト](ugc-scripting.md)、[作品の境界](item-boundaries-and-rights.md)および `crates/protocol/src/lib.rs` の `ObjectView` / `Visual` と照合した。

## 部品とアセットの棚卸し

| 分野 | 作者が扱いたい最小単位 | 現状 | 判断 |
|---|---|---|---|
| 作品の構造 | 親子Slot、まとめて複製・保存する作品、内部参照 | 親子Slotと子孫の複製・削除はある。ライブラリ保存は単一ObjectView | 作品境界と内部参照の付け替えが先決 |
| 描画 | 基本形状、読み込みモデル、画像、材質、ライト | 8種の基本形状、画像、GLB/FBX、動画。色は編集可能 | 材質を色から独立させ、ライトを追加する余地が大きい |
| 手続き的アセット | 寸法や分割数で変わるメッシュ、曲線、反復配置、粒子 | 固定種類のプリミティブをGodot側で生成 | 全メッシュAPIより、型付きパラメータを持つ生成器から始める案 |
| UI | 文字、画像、ボタン、トグル、スライダー、文字入力、レイアウト | アプリ内の組み込みUIにはある。作者が作るUIとしては未実装 | 既存WorldPanelの表示・入力面に、作品用の内容モデルを載せる案 |
| 入力とイベント | どの部品を誰がどう操作したか、押下・解放・値変更 | 物体の`on_interact`はあるが汎用のイベント引数はない | UIと物理ボタンで共通に扱える型付きイベントが必要 |
| スクリプトと状態 | タイマー、共有状態、部品参照、生成・削除、表示値の更新 | Luauの`update` / `on_interact` / `drive`、小さな状態、selfの4フィールドのWrite/Drive | 作品内の部品参照と操作対象の拡張、盤面などの構造化状態が必要 |
| 物理 | Collider形状、剛体、Trigger、接触イベント | 箱近似Colliderとキャラクター移動・保持 | テトリスには不要。物理玩具の段階で追加 |
| 音・時間 | 音声クリップ、3D音源、再生イベント、同期する時刻 | 動画の再生同期、共有時計 | 短い効果音は共有の「鳴らすイベント」とローカル再生を分ける案 |
| アニメーション・効果 | キーフレーム、補間、粒子、スポーン | TransformのScript Driveで計算可能 | 複数部品への適用と再利用できるComponentが不足 |
| カメラ・環境 | 作品内カメラ、ライト、空・環境設定 | プレイヤー用カメラはある | 作者が配置・保存できるカメラや環境Componentは未実装 |

`WorldPanel`は共有の配置・表示面・入力転送を持つが、表示内容は現在 `InspectorView` と `CreationView` に分岐している。[UIの表示先](../guidelines/ui-surfaces.md)と[3Dパネル指針](../guidelines/world-space-panels.md)に沿い、作者用UIを増やす際は新しい描画面や通信経路を複製しない。

## ECSの分離を保つ構成案

以下は**設計案**であり、実装済みのComponent名ではない。現在の `Slot`、`LocalTransform`、`Collider`、`MeshRenderer`、`WorldPanel`、`ScriptComponents` はRustの別Componentにある（[`components.rs`](../crates/core/src/components.rs)）。`ObjectView` は同期・表示に使う集約形であり、ワールドの全機能を一つのECS Componentへ詰める指示ではない。

| データ | ECSでの置き方 | 共有・ローカルの扱い |
|---|---|---|
| 作品内の部品 | 安定アドレスを持つSlotと、役割ごとのComponent。作品境界は親子関係と別に表す | 構造・設定は共有、内部参照は複製時に付け替える |
| 形状・材質・音の素材 | ComponentはアセットIDと設定を保持し、バイト列はアセット保管層へ置く | IDと設定を共有、ファイル本体は必要時に配布 |
| 作者用UI | パネルとText/Button等の要素を型付きデータとして登録。GodotがUIノードへ投影する | 文面・配置・表示する共有値はRust、ホバー・フォーカスは端末内 |
| ボタン操作 | 対象・操作名・値・送信者を持つ一時イベントをRustの検証・確定経路へ渡す | 押した事実は入力。確定した結果だけ保存し、同じ動作を全クライアントで重複実行しない |
| 手続き的メッシュ | 生成器の種類と寸法等のパラメータをComponentに置く | パラメータを共有し、Godotがローカル生成。結果が実装依存なら完成アセットをハッシュ参照する |
| テトリスの盤面 | 盤面セル、操作中のピース、スコア、時計を用途別のComponentに分ける | Rustホストが入力順と状態を確定。200マスを200個のネットワークEntityにする必要はない |

現在の `MeshRenderer.visual` は表示種類のenum、`ScriptComponents` は複数スクリプトのVecを内包する。新機能をすべてこれらへ追加すると、関連しない変更まで同じComponentの改訂・送信単位になりやすい。部品の設定を独立Componentと登録表へ置き、表示用の集約は境界で組み立てる。登録表は検証・同期・保存を共通化する現在の仕組み（[`replication.rs`](../crates/core/src/replication.rs)、[ランタイム](runtime.md)）を使う。

## 実装順を考えるための小さな作品

以下は**調査からの提案**で、決定済みのロードマップではない。

1. **色切替パネル**：作者が `Text` と `Button` を配置し、押下で同じ作品内の物体の色を変更。入力、参照、共有Write、表示更新を通す。
2. **共同テトリス**：同じ盤面を複数人が見て操作。`Button` / キー操作にアクション名と操作した参加者を付け、Rustの共有状態とタイマーから全員の盤面を描く。盤面は最初は色付きセルでよい。
3. **動くランタン**：複数Slotの作品境界、ライト、材質、音、パラメータ付き形状を保存・複製しても内部参照が混ざらないことを確認。
4. **物理玩具**：剛体、接触、生成・削除の負荷と同期規則を別段階で検証。

テトリスの落下速度自体より、**作者が入力と表示を作れること、共有状態を小さく確定できること**が現在の差分。ResoniteのComponent数をそのまま追うより、これらの作品を完成させるための少数の部品と結線を揃える方が検証しやすい。

## 確認していないこと

- Unity製品・Packageの全機能一覧や、任意のAsset Store製品との互換性は調べていない。
- Resoniteの全Componentを動作確認していない。Wikiの列挙と手元の一部型の静的確認を使った。
- Resoniteの複数人同時操作時の遅延・競合、tukuriumでの多数UI・大量セルの性能は測定していない。
- ライセンスや所有者による操作制限は現在保留中であり、この調査の実装前提に含めない。
