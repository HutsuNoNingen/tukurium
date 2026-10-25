# Componentとインスペクタの対応を見直す

調査日：2026-10-26。状態：現状の監査と今後の設計案。実装済みの操作は[インスペクタの操作](inspector-controls.md)を参照する。

## 採用する考え方

ResoniteのScene InspectorはSlotの階層・Transformと、付いたComponentの型付きFieldを同じ画面で扱う。Componentに共通する`Enabled`や`UpdateOrder`も、すべてのComponentで機能するわけではない。Driveの制御元と共有値の編集も区別される。参照：[Scene Inspector](https://wiki.resonite.com/Scene_Inspector/en)、[Component](https://wiki.resonite.com/Type:Component)、[Drive](https://wiki.resonite.com/Drive)。

tukuriumでは、**RustのComponent登録表を、共有・保存されるFieldの正本とする**。インスペクタは、制作で意味のある設定を型付きで表示する。計算済みの値は読み取り専用、撮影や再生の操作はコマンド、GPU資源やフォーカスは端末内の状態として扱う。`Enabled`は機能ごとの意味を決めてから追加する。ノードから操作できるFieldと、インスペクタに見えるFieldの範囲も別に定義する。

## 現状と不足

| Component | 現在インスペクタにあるもの | 見直す点 |
|---|---|---|
| Slot / Transform | Name、Parent、ローカルPosition・Rotation・Scale | 親子関係を確認するためのワールド姿勢は読み取り専用で示す候補。派生値へ通常のEditを送らない |
| MeshRenderer | Color | `visual`は登録済みだがコマンド専用で非表示。まず形状・素材参照の概要を読み取り専用で示す。形状変更には構造コマンドと依存Componentの検証が必要 |
| Collider | 有無、Solid、Half extents | 現在保存される設定は概ね編集できる。Offsetや形状・衝突対象を導入するならRustの新しいデータと検証が必要 |
| WorldText | Text、Color | 空文字と80文字までをRustは許すが、従来の入力欄は空文字を拒否し32文字で切った。この監査で修正。Size・Alignmentなどは現状Godotの固定値で、共有Fieldはまだない |
| Camera | Enabled | FOVはGodotの撮影画面だけの値。共有・保存されない。ProjectionやNear/FarもまだFieldがない。撮影操作と撮影画像はFieldにしない |
| Mirror | Enabled | 反射解像度と反射テクスチャは端末内。共有設定が必要になれば鏡面の意味をRustに追加する |
| MediaPlayback | Play、Pause、Restart | Seek・LoopはRustコマンドにあるがComponent欄にはない。動画以外に空の節が出る問題はこの監査で修正 |
| ScriptComponents | Scriptの一覧、Source、公開パラメータ、実行状態 | 専用Script画面を維持。Field参照とDrive元は通常のFieldから辿れるようにする |
| WorldPanel / InspectorView / CreationView | 一部が汎用欄でJSON表示される | 共有の編集道具が持つ内部表示状態。通常設定、専用操作、内部状態をスキーマで区別する |
| ItemIdentity | 汎用編集欄なし | 作者・出典は必要なら読み取り専用。保留中の利用条件を編集欄に戻さない |

現状の根拠は[登録表](../crates/core/src/replication.rs)、[共有インスペクタと2Dインスペクタの共通表示](../client-godot/inspector_view.gd)、[CameraのローカルFOV](../client-godot/camera_tool.gd)、[WorldTextの描画設定](../client-godot/main.gd)を参照。現在、両インスペクタは同じ`inspector_view.gd`を使い、登録表のComponent schemaと`object.fields`を突き合わせて表示する。未知の`value_type`はJSON文字列の見出しになるため、登録したFieldが操作可能とは限らない。

## 実装順と完了条件

1. **既存Fieldの欠落を直す。** WorldTextの空文字・80文字を2D/3Dから編集でき、ホストとゲストに届くことを確認する。動画以外の空のMediaPlayback節を消し、Removeの事前判定をRustの依存条件へ合わせる。
2. **スキーマに表示上の役割を持たせる。** 制作者が編集する設定、読み取り専用の派生値、専用コマンド、内部状態を区別する。`FieldSchema.access`とUIの操作可否を照合し、編集可能な型にエディタがない場合はテストで検出する。3Dと2Dで別々のField定義を持たない。
3. **既存の見えない情報を示す。** MeshRendererの形状・素材概要、ワールド姿勢、動画の現在位置・Loop・Seekを適切な読み取り欄や操作欄に置く。
4. **新しい制作者向けFieldを設計する。** Camera LensのFOV、WorldText Styleのサイズ・配置などから着手する。Colliderやホッケー用のPuckMotion等も、まずComponentの意味・検証・同期・保存とインスペクタ欄を一緒に定義する。

既存ComponentのField数をそのまま増やすと、現在の復元処理が厳密なField数・スキーマ版を要求するため、保存済みワールドを読み込めなくなる。過去データの移行を決めるまでは、LensやStyleを**任意の新Component**として追加する方法を優先して検討する。既存ワールドの消去や暗黙の移行は行わない。

Resoniteで参考にした型別のField：[Slot](https://wiki.resonite.com/Slot)、[BoxCollider](https://wiki.resonite.com/Component:BoxCollider)、[MeshRenderer](https://wiki.resonite.com/Components:MeshRenderer)、[TextRenderer](https://wiki.resonite.com/Component:TextRenderer)、[Camera](https://wiki.resonite.com/Component:Camera)、[CameraPortal](https://wiki.resonite.com/Component:CameraPortal)、[Canvas](https://wiki.resonite.com/Canvas)。ResoniteのField構成をそのまま移す提案ではなく、制作時に必要な設定と実行時の内部状態を分けるための比較である。
