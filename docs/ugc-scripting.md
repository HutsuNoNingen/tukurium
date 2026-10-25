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

Luauの共有Writeは20Hzのupdateまたは操作時のon_interactをホストが実行し、結果を検証・確定して既存のcommitで配信する。drive(ctx)は各クライアントのRust ECSで実行し、描画と衝突に使うローカル値を求める。RotatorテンプレートはこのDrive方式を使う。初版は任意のSystem順序変更、親子構造、複数物体のアイテム化、クラウドインベントリを含めない。

WriteとDriveのAPI・WorldTime・ECSへの接続は [ローカルDrive](ugc-driving.md) を参照。操作のWriteをホストで実行する点など、Resoniteと同一ではない。Resoniteのローカル実行・Write・Driveの違いと今後の設計候補は [実行と同期の調査](resonite-execution-sync.md) を参照。

参考API： [mlua](https://docs.rs/mlua/0.12.2/mlua/)、[Luauのサンドボックス](https://luau.org/sandbox/)。

## 共有インスペクタから共同編集

ワールド内の共有インスペクタで **Scripts** を押すと、同じ面の中でLuaを編集できる。表示中のタブとスクリプト選択は共有される。同じスクリプトを別のインスペクタから開いても、同じ共有ドラフトを読む。

Luaは複数人が同時に入力でき、他の人のカーソルが名前と色で表示される。文字には安定したIDを付けてマージするため、同じ位置への入力や、通信の返答待ち中の追加入力も保持する。通常の位置・色などのフィールド編集は従来の編集リースを使う。

入力中のドラフトと、実行するコードは別。ドラフトは約80msで送信し、**Apply code** が成功した時だけ共有スクリプトを更新する。構文エラーでもドラフトは全員に見え、動作中のコードは保持する。未送信の入力がある状態でApplyを押すと、送信完了を待ってから適用する。

ドラフトはホストのメモリに保持する。ワールドに保存されるのはApply済みのコードであり、ホスト再起動をまたいで残したい内容はApplyする。接続が切れた時に未送信の文字があれば、ローカルに残して編集を止める。必要な部分をコピーしてから **Reload draft** で共有文書へ戻る。

現在はオンライン共同編集。共有Undo/Redo、変更履歴UI、オフライン編集は未実装。文書は8 KiB、同時参加は文書ごとに64編集画面まで。履歴が上限に達したらApplyし、全員が閉じてから開き直す。通常の2D Script Systemsウィンドウは従来どおり適用済みコードを直接編集する。

検証: `node tests/script-documents-integration.mjs --rendered` はホスト／ゲストの同時挿入と、実際のGodot共有パネル2枚からの同時編集、追加入力、複数カーソル、Apply、無効コードの隔離を確認する。

## 作り方

### ノードグラフ

**+ Node graph** でビジュアルスクリプトを物体へ追加する。キャンバスの空白を右クリックし、検索してノードをその位置に追加する。上部の一覧と **+ Node** からも追加できる。ポートをドラッグして接続し、**Apply graph** で保存する。オレンジ色はイベントから処理へ進む線で、ほかの色は値を処理の入力へ渡す線。ノードはドラッグで移動でき、選択してDeleteで削除する。追加しても既存ノードの位置と表示倍率は維持する。同じ編集画面が個人の2Dウィンドウと共有インスペクタの3Dパネルに出る。3DパネルのFPS操作中は右クリックが物体の保持に使われるため、Escでカーソルを出してからグラフを右クリックする。

| 種類 | ノード |
|---|---|
| イベント | On Update（ホストの定期更新）、On Interact（クリック）、On Drive（各クライアントの表示更新） |
| 実行の流れ | Branch、Sequence、Set State、Write Field、Drive Field |
| 値・入力 | Number、Bool、String、Vec3、Color、Public Number、Delta、Time、World Time、Read Field、Get State |
| 計算・比較 | Add、Subtract、Multiply、Divide、Modulo、Min、Max、Less、Greater、Equal、And、Or、Not、Select |
| 変換・確認 | To String、Join、Rotation Y、Display |

Displayは接続した値をノード上に表示する確認用ノードで、値の出力もそのまま次へ渡せる。共有状態や保存済みStateには表示結果を書き込まない。数値・演算・公開パラメータ・共有フィールド・保存済みStateなど、編集画面で読める値からプレビューを計算する。実行時にしか決まらないDeltaや未接続の入力は「—」と表示する。表示値は編集用プレビューなので、共有WriteやローカルDriveの確定値ではない。[ResoniteのValue Display](https://wiki.resonite.com/ProtoFlux:ValueDisplay) が入力値をノード上に示す用途を参考にした（確認日：2026-10-25）。

Read/Write/Drive Fieldを追加して適用すると、そのノード用のField reference欄が公開パラメータに現れる。インスペクタのフィールド見出しから参照をドラッグして設定する。Write FieldはOn UpdateまたはOn Interactから、Drive FieldはOn Driveからつなぐ。Set Stateは共有イベント側でのみ使える。グラフはScript Componentのソースとして保存され、Rustが接続・型・循環を検証し、既存のスクリプト実行器とComponent登録表を通して実行する。

たとえばローカル表示のストップウォッチは、`On Drive → Drive Field` のflowをつなぎ、`Time → To String → Drive Field.value` をつなぐ。Apply後、Drive Fieldの参照欄へText子スロットの`world_text`を入れる。表示は各端末で更新され、共有ワールドの文字列は変更しない。共有のカウントなら `On Update → Set State` と `Get State + Delta → Set State.value` を組む。

1グラフは最大64ノード・128接続、公開フィールドは16個まで。ソース全体の8 KiB制限はLuauと共通。入力が未接続、型が違う、循環がある、イベントに対して不正なWrite/DriveがあるグラフはApply時に拒否され、直前の実行可能な版を維持する。グラフを変更しても同じノードIDの公開パラメータ値は保持する。

共有のLuauコードは複数人で同時編集できる。グラフは共有テキストドラフトを使わず、Script Componentのrevisionを検査して一括更新する。同時にグラフを編集した場合、後からのApplyは競合として拒否される。

### Luauコード

1. 物体を選択し、Inspectorの **Script Systems…** を開く。
2. **Add a script** の **Rotator** / **ClickColor** を選び、**+ Add template** で追加する。自作する場合は **+ Blank script** を押して、生成されたコードを編集・貼り替える。
3. コードを書き換え、**Apply code** で共有ワールドへ反映する。構文エラー時は前のコードを保持する。
4. コードの `fields` から作られた数値・bool・文字列・vec3・色の欄を変更する。値は約180msで自動同期する。コード自体を変更するとフィールド値は新しいdefaultへ戻る。
5. 右上の **Pause** で一時停止し、**Run** で再開する。停止・再開しても `ctx:set_state` の保存済み状態は残る。**Apply code** は実行／停止の状態を変えずにコードを保存する。クリック用Systemは物体の左クリックまたは **Test click** で動かせる。

**Attached scripts** はその物体に付いているスクリプトの一覧で、左側に常時表示し、選択した1つのコードとパラメータを右側で編集する。一覧を切り替えても、他のenabledなスクリプトは動き続ける。各項目にはRunning / Paused / Errorを表示する。右側にも選択したスクリプトの実行状態を表示する。未適用のコードがある間は、別スクリプトへの切り替えと実行／停止を抑止してドラフトを守る。**Add a script** は新しく追加するための欄で、作成済みスクリプトのライブラリではない。

1つの物体に最大4つ付けられる。同じ物体内ではコードの `name` を別々にする。Blank scriptはScript1、Script2…と未使用の名前で作成する。4つに達した場合と、同名テンプレートがすでに付いている場合は追加ボタンを無効にし、理由を表示する。削除は **Remove selected** で、確認後に選択したスクリプトを取り外す。

コード・パラメータの版を、実行中の姿勢や状態の更新から分ける。Systemが物体を動かしていてもパラメータ編集のrevisionは変化しない。別ユーザーのコード編集と競合した場合は拒否し、**Reload shared code** で読み直す。自分のドラフトは勝手に上書きしない。

```lua
return {
    name = "Rotator",
    fields = {
        speed = { type = "number", default = 45, min = -720, max = 720 }
    },
    drive = function(ctx)
        for entity in ctx:query("Rotator", "Transform") do
            ctx:drive(entity, "rotation", ctx:rotation_y(entity.Rotator.speed * ctx.time, entity.Transform.rotation))
        end
    end
}
```

`name` は同じ物体内で一意。`fields` のキーはフィールドIDで、各定義は `type`・`default`、任意の `label`・`min`・`max` を持つ。名前とIDは英字・数字・アンダースコアで40文字まで、先頭に数字は使えない。

| API | 動作 |
|---|---|
| `ctx.world_time` / `ctx.time` | ホストに合わせた時刻 / 停止・再開可能なスクリプトの経過秒数 |
| `ctx:write(entity, field, value)` | 共有Write。update / on_interact専用。対象はposition / rotation / scale / color |
| `ctx:drive(entity, field, value)` | 同じ4フィールドのローカル値を導出。drive専用 |
| `ctx:rotation_y(degrees, base?)` | Y軸回転を基準Quaternionへ加算して返す。基準省略時は単位回転 |
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

各物体には4個、ワールド全体で32個まで。ソース8 KiB、フィールド16個、明示的状態4 KiB、全Scriptデータ128 KiBまで。VMごとに4 MiBのメモリ制限、1コールバックは最大5msと割り込み回数上限を設け、System処理をフレームごとの予算で分ける。組み込みAPIの処理時間やコンパイル時間まで含む厳密なリアルタイム保証ではない。共有Writeの例外・予算超過はホストの診断を全員へ表示する。ローカルDriveの例外はその端末へ表示し、失敗した評価の結果を破棄する。Apply codeや設定更新で再評価する。

既存のset_positionなどは共有Writeの省略形として残る。drive(ctx)からの共有Writeとset_state、および共有コールバックからのDriveはエラーになる。既存のupdateコードを自動的にDriveへ変換はしない。

新しい実行器で使用できるライブラリはLuaの計算・文字列・テーブル等に限定する。ファイル・ネットワーク・モジュール読み込み、保護呼び出し、coroutineは公開しない。共有操作はホストでのみ実行し、通常の編集と同じTransform等の値域を検証する。

## 検証

`cargo test --workspace --locked`、`cargo clippy --workspace --all-targets --locked -- -D warnings` に加え、release Engineで `node tests/ugc-integration.mjs --rendered` を実行する。実際のGodot UIで追加・コード編集・パラメータ変更・停止・クリック・構文エラーの復旧を行い、ホスト・ゲスト・途中参加者、複製の状態、保存・再起動、無限ループ停止を検証する。描画テストの画像は `.run/ugc-*/script-editor.png`。

`--relay=wss://45.77.130.146/ws` では認証付きの公開中継を使い、テスト専用のランダムルームで同じ一連の検証を実行できる。この経路でも3クライアントで確認済み。認証情報は環境変数で渡す。

連続回転の検証で浮動小数点のJSON読み戻しに最下位桁の差が出たため、`serde_json` の `float_roundtrip` を有効にした。また、ゲストEngineが付け替えるネットワーク要求IDを元のUI要求IDへ戻して応答するようにし、複数編集UIの応答を識別できるようにした。
