# UGC Script Systems

## 実装タスク

- [x] 1. Resonite / Lumora のローカル実装を調べ、責務と最小APIを決める。
- [x] 2. Luauの実行器、保存可能なScript Component、ホスト側System実行と検証を実装する。
- [x] 3. Godotにコード編集・公開パラメータ・実行状態・クリック操作を追加する。
- [x] 4. 2クライアント同期、例外・実行量制限、複製、保存・再起動を検証する。

完了条件：ユーザーが物体にLuauを追加し、自分でコードと数値を変更できる。回転とクリックによる色変更が別クライアントにも反映され、保存・複製後も動く。失敗したスクリプトは停止し、ワールド全体を止めない。

## 参考調査（2026-10-23）

インストール済みアセンブリをILSpy 11.1.0.9782で静的に調査した。抽出物は `.run/ugc-research/` に置き、製品のソースやアセットを本プロジェクトへ取り込まない。ここで述べる内容は静的解析結果であり、実行時性能の測定ではない。

| 対象 | 確認した点 | 今回取り入れる点 |
|---|---|---|
| Resonite `FrooxEngine.Component` / `ComponentBase` | Slot所属、Enabled、更新順、開始・変更・破棄のライフサイクル。無効な部品の更新を止める | Scriptも物体に付く部品として扱う。設定と一時実行状態を分離する |
| Lumora `ComponentExecutionPolicy` | Userspace限定型の取り付け場所を検証する | 実行側が操作可能な範囲を決め、スクリプトへ生のWorldを渡さない |
| Lumora `ScriptHost` | グラフのアセット参照、コンパイル結果キャッシュ、共有とローカルの実行経路、診断・実行統計 | コード編集、コンパイル、実行、診断を分ける。初版の共有動作はホストのみ実行する |
| Lumora `ScriptFieldDrive<T>` | 対象Field参照と駆動状態を管理し、共有の権限状態を確認する | スクリプトの書き込みを限定APIに集め、Rust側で検証してから適用する |
| Lumora `LumoraWasm.dll` | Wasmのモジュール・検証・実行・制限の型を確認した | 言語と実行方式を編集UIから分離する。今回はLuauを使い、Wasm互換は実装しない |

調査対象のSHA-256：

- `FrooxEngine.dll`: `CD1F57C97408AADA9158E8C9D5C9ADAB05815DBB4A28756AA42681C6B462CEA8`
- `LumoraCore.dll`: `2B28B238403458EA27B4E152FEA157ED1DCBE2EBF947A0901636C023D949AC39`
- `LumoraWasm.dll`: `093B8EFD7B41EB4EA1ED324B75E069E4970367DEFF2259101A6B46380995DB7B`

## 最初の実装範囲

RustにLuauを組み込む。ソースはComponent名・公開フィールド定義・`update(ctx, dt)`・`on_interact(ctx)`を返すモジュール。公開フィールドはインスペクタから編集する。部品のソース・設定・明示的な状態をワールドへ保存し、VMや関数は保存しない。

SystemはComponentをqueryし、位置・回転・スケール・色と自身の明示的な状態を更新できる。最初は取り付け先の1オブジェクトを操作範囲とする。今のワールドに親子構造がないためで、ワールド全体の無制限queryやファイル・ネットワークアクセスは公開しない。箱・画像・動画・モデルで同じAPIを使う。

共有動作は20Hzでホストが実行し、結果をまとめて検証・確定して既存のcommitで配信する。ゲストは結果を描画する。初版は任意のSystem順序変更、親子構造、複数物体のアイテム化、クラウドインベントリを含めない。

参考API： [mlua](https://docs.rs/mlua/0.12.2/mlua/)、[Luauのサンドボックス](https://luau.org/sandbox/)。

## 作り方

1. 物体を選択し、Inspectorの **Script Systems…** を開く。
2. **Rotator** または **ClickColor** を選び、**+ Add component** で追加する。
3. コードを書き換え、**Apply / restart** で共有ワールドへ反映する。構文エラー時は前のコードを保持する。
4. コードの `fields` から作られた数値・bool・文字列・vec3・色の欄を変更する。値は約180msで自動同期する。コード自体を変更するとフィールド値は新しいdefaultへ戻る。
5. **Run on host** で停止・再開する。クリック用Systemは物体の左クリックまたは **Test click** で動かせる。

コード・パラメータの版を、実行中の姿勢や状態の更新から分ける。Systemが物体を動かしていてもパラメータ編集のrevisionは変化しない。別ユーザーのコード編集と競合した場合は拒否し、**Reload shared code** で読み直す。自分のドラフトは勝手に上書きしない。

```lua
return {
    name = "Rotator",
    fields = {
        speed = { type = "number", default = 45, min = -720, max = 720 }
    },
    update = function(ctx, dt)
        for entity in ctx:query("Rotator", "Transform") do
            ctx:rotate_y(entity, entity.Rotator.speed * dt)
        end
    end
}
```

`name` は同じ物体内で一意。`fields` のキーはフィールドIDで、各定義は `type`・`default`、任意の `label`・`min`・`max` を持つ。名前とIDは英字・数字・アンダースコアで40文字まで、先頭に数字は使えない。

| API | 動作 |
|---|---|
| `ctx.self` | 取り付け先のEntity。各ScriptのパラメータとTransform、Renderer、存在すればColliderを読める |
| `ctx:query("Component", ...)` | 条件を満たすEntityのイテレーター。現在の対象範囲は取り付け先だけ |
| `ctx:set_position(entity, {x,y,z})` | 位置を設定 |
| `ctx:translate(entity, {x,y,z})` | 位置に加算 |
| `ctx:rotate_y(entity, degrees)` | Y軸を基準に角度を加算 |
| `ctx:set_scale(entity, {x,y,z})` | スケールを設定 |
| `ctx:set_color(entity, {r,g,b,a})` | 色を設定 |
| `ctx:get_state("key", default)` | 明示的に保存された実行状態を読む |
| `ctx:set_state("key", value)` | 実行状態を変更。成功したコールバックの終了時に反映する |

Entityから読める値はその呼び出し開始時点のコピー。変更はAPIで要求する。複数の書き込みはコールバック内で積み重ね、エラーならそのコールバックの変更を破棄する。通常のLua変数やクロージャーはVM内だけで生き、保存・複製に引き継がない。必要な状態は `get_state` / `set_state` を使う。

各物体には4個、ワールド全体で32個まで。ソース8 KiB、フィールド16個、明示的状態4 KiB、全Scriptデータ128 KiBまで。VMごとに4 MiBのメモリ制限、1コールバックは最大5msと割り込み回数上限を設け、System処理をフレームごとの予算で分ける。組み込みAPIの処理時間やコンパイル時間まで含む厳密なリアルタイム保証ではない。例外・予算超過では診断を全員へ表示し、Apply / restartや設定更新までそのScriptを止める。

新しい実行器で使用できるライブラリはLuaの計算・文字列・テーブル等に限定する。ファイル・ネットワーク・モジュール読み込み、保護呼び出し、coroutineは公開しない。共有操作はホストでのみ実行し、通常の編集と同じTransform等の値域を検証する。

## 検証

`cargo test --workspace --locked`、`cargo clippy --workspace --all-targets --locked -- -D warnings` に加え、release Engineで `node tests/ugc-integration.mjs --rendered` を実行する。実際のGodot UIで追加・コード編集・パラメータ変更・停止・クリック・構文エラーの復旧を行い、ホスト・ゲスト・途中参加者、複製の状態、保存・再起動、無限ループ停止を検証する。描画テストの画像は `.run/ugc-*/script-editor.png`。

`--relay=wss://45.77.130.146/ws` では認証付きの公開中継を使い、テスト専用のランダムルームで同じ一連の検証を実行できる。この経路でも3クライアントで確認済み。認証情報は環境変数で渡す。

連続回転の検証で浮動小数点のJSON読み戻しに最下位桁の差が出たため、`serde_json` の `float_roundtrip` を有効にした。また、ゲストEngineが付け替えるネットワーク要求IDを元のUI要求IDへ戻して応答するようにし、複数編集UIの応答を識別できるようにした。
