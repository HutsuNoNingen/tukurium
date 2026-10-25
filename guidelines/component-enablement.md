# 機能コンポーネントの有効状態をデータとして扱う

状態：MirrorとCameraで実装済み。ほかの任意の機能コンポーネントへ広げるときの方針。
確認日：2026-10-25。

`Enabled`はオブジェクトが呼び出すメソッドではなく、コンポーネントに保存された共有の値。Systemはそのコンポーネントが存在し、有効なときだけ機能を実行する。GodotはMirrorの反射テクスチャやCameraの操作画面を端末ごとに生成する。機能の有無と有効状態の確定・同期・保存はRustの登録表を通す。

構造の基礎になるIdentity・Slot・Transformは無効化しない。任意に付け外す機能、たとえばMirrorとCameraでは`Enabled`を持つ。今後別の機能へ広げるときも、無効化時に残すデータ、描画・入力・スクリプトの停止範囲をその機能ごとに定義する。「すべてのコンポーネントに同じチェックボックスを付ける」ことを自動の規則にはしない。

「Mirrorを作成」「Cameraを作成」は、見た目のMeshRendererと機能コンポーネントを一度に付けるプリセット。既存のQuadにもMirrorを、既存SlotにもCameraを追加できる。Mirrorは平面メッシュを必要とするため、Rustが追加時と保存復元時に検証する。`Enabled`は共有Editで変更し、ScriptのWriteも許す。端末ごとのDriveにはしない。

既存の版1保存ワールドにはCamera・Mirror形状だけで機能が表現された物体がある。版1を読み込むときだけ対応する機能コンポーネントを補い、版2へ保存する。版2では形状と機能を分けて扱い、コンポーネントの削除が保存後に復活しないようにする。

実装：[登録表](../crates/core/src/replication.rs)、[描画](../client-godot/mirror_renderer.gd)、[カメラ操作](../client-godot/camera_tool.gd)。確認：Rustの共有・復元テストと描画付きの [作成・インスペクタ統合テスト](../tests/creation-integration.mjs)。

## Cameraの撮影は一時要求として渡す

Cameraの`Enabled`と姿勢は共有データだが、シャッター操作は保存するフィールドではない。Scriptの`on_interact`で期限を状態に記録し、`update`で`ctx.time`と比較すれば、タイマーの意味をUGC側で組める。期限に達して`ctx:capture_camera(ctx.self)`を呼ぶと、ホストのScript SystemがCameraの有効状態・対象を検証し、最後に操作したクライアントへ一度だけ撮影要求を送る。Godotはその要求を受けて描画し、本人の写真フォルダーへ保存する。要求をComponentの値やDriveの毎フレーム出力にすると、途中参加で再撮影したり全員が同じ写真を保存したりするため、この経路を使い分ける。複数人が期限前に同じCameraを操作した場合は、後から操作した人へ撮影する。

カウントダウンの表示は既存の`WorldText`を使う。Scriptの状態から残り秒数を計算し、値が変わったときだけ束縛した`world_text`フィールドへ`write_field`する。公開中のワークショップ作品「カウントダウンカメラ | ノード」は`Ceil → To String → Write Field`を使う。これなら文字は通常のComponentとして同期・保存され、撮影という一時要求と混同しない。カウントダウンを自作する場合は、表示用の子SlotにWorldTextを付ける。

確認：[scripting.rs](../crates/core/src/scripting.rs) の宛先・無効化テストと、[creation-integration.mjs](../tests/creation-integration.mjs) の描画付き撮影テスト。操作方法は[インスペクタの操作](../docs/inspector-controls.md)。
