# 機能コンポーネントの有効状態をデータとして扱う

状態：MirrorとCameraで実装済み。ほかの任意の機能コンポーネントへ広げるときの方針。
確認日：2026-10-25。

`Enabled`はオブジェクトが呼び出すメソッドではなく、コンポーネントに保存された共有の値。Systemはそのコンポーネントが存在し、有効なときだけ機能を実行する。GodotはMirrorの反射テクスチャやCameraの操作画面を端末ごとに生成する。機能の有無と有効状態の確定・同期・保存はRustの登録表を通す。

構造の基礎になるIdentity・Slot・Transformは無効化しない。任意に付け外す機能、たとえばMirrorとCameraでは`Enabled`を持つ。今後別の機能へ広げるときも、無効化時に残すデータ、描画・入力・スクリプトの停止範囲をその機能ごとに定義する。「すべてのコンポーネントに同じチェックボックスを付ける」ことを自動の規則にはしない。

「Mirrorを作成」「Cameraを作成」は、見た目のMeshRendererと機能コンポーネントを一度に付けるプリセット。既存のQuadにもMirrorを、既存SlotにもCameraを追加できる。Mirrorは平面メッシュを必要とするため、Rustが追加時と保存復元時に検証する。`Enabled`は共有Editで変更し、ScriptのWriteも許す。端末ごとのDriveにはしない。

既存の版1保存ワールドにはCamera・Mirror形状だけで機能が表現された物体がある。版1を読み込むときだけ対応する機能コンポーネントを補い、版2へ保存する。版2では形状と機能を分けて扱い、コンポーネントの削除が保存後に復活しないようにする。

実装：[登録表](../crates/core/src/replication.rs)、[描画](../client-godot/mirror_renderer.gd)、[カメラ操作](../client-godot/camera_tool.gd)。確認：Rustの共有・復元テストと描画付きの [作成・インスペクタ統合テスト](../tests/creation-integration.mjs)。
