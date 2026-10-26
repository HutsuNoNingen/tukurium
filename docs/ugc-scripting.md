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

Systemは取り付け先のComponentを読み書きし、位置・回転・スケール・色と自身の明示的な状態を更新できる。別のSlotは、登録したField参照を通して操作する。ワールド全体の無制限queryやファイル・ネットワークアクセスは公開しない。箱・画像・動画・モデルで同じAPIを使う。

Luauの共有Writeは20Hzのupdateまたは操作時のon_interactをホストが実行し、結果を検証・確定して既存のcommitで配信する。drive(ctx)は各クライアントのRust ECSで実行し、描画と衝突に使うローカル値を求める。Rotatorの作例はこのDrive方式を使う。

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
| イベント | On Update（ホストの定期更新）、On Interact（クリック）、On Button Press / Release、On Contact Start / End、On Drive（各クライアントの表示更新） |
| 実行の流れ | Branch（If / Else）、Sequence、Set State、Write Field、Drive Field、Capture Camera、For Range、While、Break、Continue |
| 値・入力 | Number、Bool、String、Vec3、Color、Public Number、Delta、Time、World Time、Contact Other ID、Read Field、Get State |
| 計算 | Add、Subtract、Multiply、Divide、Modulo、Min、Max、Ceil、Abs、Floor、Sin、Cos、Clamp、Lerp、Negate、Sqrt、Power、Round、Sign、Tan、Asin、Acos、Atan2、Log、Exp、Remap、Smoothstep、Ping Pong |
| 条件・論理 | Less、Less Equal、Greater、Greater Equal、Equal、Not Equal、And、Or、Not、Xor、Select |
| 文字列・変換 | To String、Join、Parse Number、Number To Bool、Bool To Number、String Length / Contains / Starts With / Ends With / Slice |
| ベクトル | Vec3 Make / Split、Vec3 Add / Subtract / Scale、Vec3 Dot / Cross、Vec3 Length / Normalize / Distance / Lerp / Angle / Reflect / Move Towards |
| 色 | Color Make / Split / Lerp |
| 回転・確認 | Rotation Y、Display |

Displayは接続した値をノード上に表示する確認用ノードで、値の出力もそのまま次へ渡せる。共有状態や保存済みStateには表示結果を書き込まない。数値・演算・公開パラメータ・共有フィールド・保存済みStateなど、編集画面で読める値からプレビューを計算する。実行時にしか決まらないDeltaや未接続の入力は「—」と表示する。表示値は編集用プレビューなので、共有WriteやローカルDriveの確定値ではない。[ResoniteのValue Display](https://wiki.resonite.com/ProtoFlux:ValueDisplay) が入力値をノード上に示す用途を参考にした（確認日：2026-10-25）。

Read/Write/Drive Fieldを追加して適用すると、そのノード用のField reference欄が公開パラメータに現れる。インスペクタのフィールド見出しから参照をドラッグして設定する。Write FieldはOn UpdateまたはOn Interactから、Drive FieldはOn Driveからつなぐ。Set Stateは共有イベント側でのみ使える。グラフはScript Componentのソースとして保存され、Rustが接続・型・循環を検証し、既存のスクリプト実行器とComponent登録表を通して実行する。

たとえばローカル表示のストップウォッチは、`On Drive → Drive Field` のflowをつなぎ、`Time → To String → Drive Field.value` をつなぐ。Apply後、Drive Fieldの参照欄へText子スロットの`world_text`を入れる。表示は各端末で更新され、共有ワールドの文字列は変更しない。共有のカウントなら `On Update → Set State` と `Get State + Delta → Set State.value` を組む。

1グラフは最大64ノード・128接続、公開フィールドは16個まで。ソース全体の8 KiB制限はLuauと共通。入力が未接続、型が違う、循環がある、イベントに対して不正なWrite/DriveがあるグラフはApply時に拒否され、直前の実行可能な版を維持する。グラフを変更しても同じノードIDの公開パラメータ値は保持する。

共有のLuauコードとノードグラフは複数人で同時編集できる。グラフはノード・値・位置・接続ごとの小さな操作をホストが検証して配り、別のノードへの同時編集を残す。同じ値や入力ポートはホストが受信した順で決まる。実行・保存する版へ反映するにはApplyが必要で、古い下書き版からのApplyは拒否する。下書き自体はセッション内だけに保持する。

### 数学と3Dベクトルを組み合わせる

ノードは入力・出力を結ぶ作成方法、Luauはコードを書く作成方法である。どちらもScript Componentとして物体に付き、ノードのJSONはRustで検証してLuauへ変換し、同じ実行器を使う。ワールドへの書き込みとローカルDriveの区別、Field参照の検証、実行予算、保存の規則も共通である。Luauには条件分岐・関数・反復を書けるが、ノードは登録された種類の範囲で組む。任意のLuauをノードへ自動変換する機能はない。

数学・ベクトル・条件・変換・文字列・色・構造化ループを合わせて53種類を追加し、ノードは全97種類になった。既存のVec3は定数で、`vec3_make`は計算したX/Y/Zを受け取る。`vec3_split`はX/Y/Zをそれぞれ別のNUMBER出力へ取り出す。

| ノードのkind / Luau | 入力と意味 |
|---|---|
| `abs` / `ctx:abs(value)`、`floor` / `ctx:floor(value)` | 絶対値、負の無限大方向への切り捨て |
| `sin` / `ctx:sin(value)`、`cos` / `ctx:cos(value)` | ラジアンで指定する三角関数。Rotation Yの角度（度）と区別する |
| `clamp` / `ctx:clamp(value, min, max)` | 境界を含む範囲へ制限。min=maxならその値、min>maxはエラー |
| `lerp` / `ctx:lerp(a, b, t)` | tを0〜1へ制限した線形補間。端点は正確にaまたはbを返す |
| `vec3_make` / `ctx:vec3_make(x, y, z)` | 3つのNUMBERから3要素のベクトルを作る |
| `vec3_split` / `ctx:vec3_x(v)`、`ctx:vec3_y(v)`、`ctx:vec3_z(v)` | 各成分を取り出す |
| `vec3_add`、`vec3_subtract` / 同名の`ctx:`関数 | a+b、a−b |
| `vec3_scale` / `ctx:vec3_scale(value, factor)` | 全成分に同じ係数を掛ける |
| `vec3_dot`、`vec3_cross` / 同名の`ctx:`関数 | 内積はNUMBER、外積a×bはVECTOR 3を返す |
| `vec3_length`、`vec3_normalize` / 同名の`ctx:`関数 | 長さ、長さ1の方向。ゼロの正規化はゼロベクトルを返す |
| `vec3_distance` / `ctx:vec3_distance(a, b)` | 2点間の距離 |
| `vec3_lerp` / `ctx:vec3_lerp(a, b, t)` | tを0〜1へ制限したベクトルの補間 |

追加した`ctx:`関数は数値の文字列変換を行わず、有限な数と正確に3要素のベクトルを受け付ける。不正な形、NaN、Infinity、計算結果の非有限値はエラーにする。共有FieldとStateへ反映する際は、既存の型・値域検証も通る。これらの関数自体は共有データを変更しない。Displayも不正な入力や非有限な結果を表示値として扱わない。

たとえば`On Drive → Drive Field`へ処理をつなぎ、`Time → Sin → Vec3 Make.y`、定数0をX/Zへ入れる。`Read Field(position) → Vec3 Add.a`と`Vec3 Make → Vec3 Add.b → Drive Field.value`で、基準位置から上下する表示を作れる。ReadとDriveの参照には同じPositionを指定する。毎回共有位置を読むので、前フレームの変位を積み重ねない。Luauなら次のように書ける。

```lua
return {
    name = "Bob",
    fields = {},
    drive = function(ctx)
        local offset = ctx:vec3_make(0, ctx:sin(ctx.time), 0)
        ctx:drive(ctx.self, "position", ctx:vec3_add(ctx.self.Transform.position, offset))
    end
}
```

参考資料の確認日：2026-10-26。[ProtoFluxの分類](https://wiki.resonite.com/Category:ProtoFlux)、[ValueClamp](https://wiki.resonite.com/ProtoFlux:ValueClamp)、[ValueLerp](https://wiki.resonite.com/ProtoFlux:ValueLerp)、[Sin](https://wiki.resonite.com/ProtoFlux:Sin)、[Unpack3](https://wiki.resonite.com/ProtoFlux:Unpack3)、[ベクトルの解説](https://wiki.resonite.com/Tutorial:Vectors)で、範囲制限・補間・ラジアン・成分分解・ベクトル演算の用途を確認した。tukuriumでは明示的なNUMBER/VECTOR 3ポートと既存Luau実行器を使い、ゼロ正規化や逆転した範囲の扱いは上記の独自仕様とした。原実装・画面・アセットは取り込んでいない。

関連実装は[数学ヘルパー](../crates/scripting/src/math.rs)、[グラフのコンパイラ](../crates/scripting/src/graph.rs)、[ノードの表示とプレビュー](../client-godot/script_graph.gd)。`cargo test -p kokoverse-scripting --locked`で計算・多出力・不正値を、`node tests/ugc-integration.mjs --rendered`でGodotからの作成・適用、ホスト/ゲスト、途中参加、複製、保存復元を検証する。

### 計算・条件・文字列・色を扱う

以下もノードのkindと同名の`ctx:`関数で使える。入出力はNUMBER / BOOL / TEXT / VECTOR 3 / COLORを明示する。型を暗黙に変えず、変換には専用ノードを使う。

| kind | 意味・境界 |
|---|---|
| `negate`、`sqrt`、`power` | 符号反転、平方根、累乗。負数の平方根など実数として定義できない結果はエラー |
| `round`、`sign` | 四捨五入は中間値をゼロから遠ざける。Signは負数−1、ゼロ0、正数1 |
| `tan`、`asin`、`acos`、`atan2` | 角度はラジアン。Asin / Acosの入力は−1〜1。Atan2の入力順はY、X |
| `log`、`exp` | 自然対数、指数関数。Logは正数のみ。非有限の結果は拒否 |
| `remap` | 入力範囲を出力範囲へ線形変換。入力端点は異なる値が必要で、逆順も扱う。範囲外を外挿する |
| `smoothstep` | Edge0 < Edge1で、範囲内を0〜1の滑らかな曲線へ変換。範囲外は0または1 |
| `ping_pong` | 正のLengthに対して0〜Lengthを往復する三角波。負のValueにも対応 |
| `less_equal`、`greater_equal` | 数値の≦、≧ |
| `not_equal`、既存の`equal` | nil、数値、BOOL、TEXT、2〜4要素の数値配列を値として比較。配列は成分ごとに比較し、異なる型は等しくない。数値は有限値が必要 |
| `xor` | BOOL二つのうち片方だけがtrueならtrue |
| `parse_number` | TEXTを有限の数値へ変換。前後の空白を除去し、不正な文字列やNaN / Infinityはエラー |
| `number_to_bool`、`bool_to_number` | 数値0はfalse、非ゼロはtrue。falseは0、trueは1 |
| `string_length`、`string_contains`、`string_starts_with`、`string_ends_with` | Unicode文字数、部分文字列、先頭・末尾の一致 |
| `string_slice` | TEXTから0始まりのStartとCountでUnicode文字単位に取り出す。非負整数のみ。範囲の末尾は文字列長に制限 |
| `vec3_angle` | 二つの方向の角度をラジアンで返す。ゼロベクトルはエラー |
| `vec3_reflect` | ValueをNormalに対して反射。Normalは正規化して使い、ゼロは拒否 |
| `vec3_move_towards` | AからBへ最大Max Deltaだけ進む。負の距離は拒否し、到達できるなら正確にBを返す |
| `color_make` | 0〜1のR/G/B/AからCOLORを作る。範囲外は拒否 |
| `color_split` | COLORからR/G/B/Aの四つのNUMBER出力へ分解。Luauでは`ctx:color_r/g/b/a(value)`を使う |
| `color_lerp` | 二つのCOLORを補間。Tは0〜1に制限し、端点を正確に返す |

文字列ヘルパーの入出力は512 UTF-8バイトまで。文字数とSliceの位置はバイト数ではなくUnicodeの文字単位で、結合文字をまとめた見た目の文字数とは異なる。Equal / Not Equalの数値入力は共有Stateの保存上限を適用せず有限値として比較し、書き込み時には従来の保存・フィールド検証を通す。条件ノードはBOOLだけを受け付けるため、数値を条件にしたいときはNumber To Boolを挟む。Luauの通常の`if`自体の真偽規則は変えない。

### If / For / Whileを組む

`Branch`の表示名は`If / Else`で、条件に応じてYesまたはNoへ進む。値の選択は既存の`Select`を使う。どちらも条件はBOOLである。

| kind | 入力 | 出力・動作 |
|---|---|---|
| `for_range` | Flow、Start、End、Step | Body、Done、Index。Endを含む整数範囲を巡回し、各回のIndexをBody内で使う |
| `while_loop` | Flow、Test（BOOL） | Body、Done。各回にTestを読み直し、falseなら終了 |
| `break_loop` | Flow | 最も内側のループを終了し、そのループのDoneへ進む |
| `continue_loop` | Flow | 最も内側のループの残りを飛ばし、次の反復へ進む |

For RangeのStart / End / Stepは正確に表現できる整数、絶対値は2^53−1以下。Stepは0にできない。StartからEndへ進まない符号なら0回、Start=Endなら1回。IndexはそのForのBody内と、その中のループだけで参照でき、Done側や別イベントからは参照できない。Break / ContinueもループBodyの外ではApplyを拒否する。

Bodyの終端をループの入力へ結び直す必要はない。グラフの循環接続は禁止したまま、Rustが構造化されたLuauのFor / Whileへ変換する。ネストを含め**1コールバック全体で最大64反復**で、上限超過・型エラー・実行予算超過はそのコールバックの変更をまとめて破棄する。従来の実行時間、割り込み回数、書き込み回数の制限も適用する。

WhileのTestは`Get State`の値を`Set State`で変えると同じ実行中に変化する。`Read Field`が読むのは呼び出し開始時点の共有値なので、同じコールバック中のWriteを読み直す仕組みではない。Driveから共有Stateの変更はできない。ローカルDriveでも有限のForやBreakは使えるが、共有値を変えてWhileの終了条件にすることはできない。

Luauでは元から通常の`if` / `for` / `while`が使える。ノードの64回上限は生成した構造化ループの規則で、手書きLuauには既存の実行予算と書き込み制限が適用される。`ctx:for_count(start, end, step)`、`ctx:condition(value)`と`ctx:loop_step()`も提供する。Loop Stepのカウンターは呼び出しごとに独立し、保存・同期の対象にはしない。

参照した概念：[ProtoFlux If](https://wiki.resonite.com/ProtoFlux:If)、[For](https://wiki.resonite.com/ProtoFlux:For)、[While](https://wiki.resonite.com/ProtoFlux:While)、[RangeLoopInt](https://wiki.resonite.com/ProtoFlux:RangeLoopInt)（確認日2026-10-26）。条件分岐と範囲反復の用途だけを参考にし、符号付きStep、共有状態の確定、スコープ検査、64回上限はtukurium自身の規則として実装した。

実装：[純粋な追加演算](../crates/scripting/src/operations.rs)、[構造化ループの検証](../crates/scripting/src/control.rs)。

### Luauコード

1. 物体を選択し、Inspectorの **Script Systems…** を開く。
2. **Add a script** の **+ Blank script** でLuaを作るか、**+ Node graph** でノードを作る。完成済みの作品はワークショップからライブラリに保存し、ワールドへ配置する。
3. コードを書き換え、**Apply code** で共有ワールドへ反映する。構文エラー時は前のコードを保持する。
4. コードの `fields` から作られた数値・bool・文字列・vec3・色の欄を変更する。値は約180msで自動同期する。コード自体を変更するとフィールド値は新しいdefaultへ戻る。
5. 右上の **Pause** で一時停止し、**Run** で再開する。停止・再開しても `ctx:set_state` の保存済み状態は残る。**Apply code** は実行／停止の状態を変えずにコードを保存する。クリック用Systemは物体の左クリックまたは **Test click** で動かせる。

**Attached scripts** はその物体に付いているスクリプトの一覧で、左側に常時表示し、選択した1つのコードとパラメータを右側で編集する。一覧を切り替えても、他のenabledなスクリプトは動き続ける。各項目にはRunning / Paused / Errorを表示する。右側にも選択したスクリプトの実行状態を表示する。未適用のコードがある間は、別スクリプトへの切り替えと実行／停止を抑止してドラフトを守る。**Add a script** は新しく追加するための欄で、作成済みスクリプトのライブラリではない。

1つの物体に最大4つ付けられる。同じ物体内ではコードの `name` を別々にする。Blank scriptはScript1、Script2…と未使用の名前で作成する。4つに達した場合は追加ボタンを無効にし、理由を表示する。削除は **Remove selected** で、確認後に選択したスクリプトを取り外す。

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
| `ctx:write(entity, field, value)` | 共有Write。共有コールバック用。対象はposition / rotation / scale / color |
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

`on_button_press(ctx)`と`on_button_release(ctx)`はButton付きのSlotに届く。押下時は既存の`on_interact(ctx)`も呼ばれる。`on_contact_start(ctx)`と`on_contact_end(ctx)`はContactSensor付きのSlotに届き、`ctx.other`から相手のAddressを読める。接触イベントはホストが共有Colliderの重なりから作る。スクリプトの公開`field_ref`へValueNumber・ValueBool・ValueTextやPlanarMotionのVelocityを渡し、`ctx:read` / `ctx:write_field`で扱える。PlanarMotionのVelocityは共有Write用で、毎フレームのローカルDrive先にはできない。

ノードグラフには`On Button Press`、`On Button Release`、`On Contact Start`、`On Contact End`、`Contact Other ID`を追加した。押下でパックを打ち出す場合は、`On Button Press`から`Write Field`へつなぎ、参照先にパックの`planar_velocity`を渡す。得点表示は`On Contact Start`からValueNumberのWriteとWorldTextのWriteへつなげられる。薄いセンサーを高速で通過すると、離散的な重なり判定で接触を見落とす場合があるため、センサーに厚みを持たせる。

Entityから読める値はその呼び出し開始時点のコピー。変更はAPIで要求する。複数の書き込みはコールバック内で積み重ね、エラーならそのコールバックの変更を破棄する。通常のLua変数やクロージャーはVM内だけで生き、保存・複製に引き継がない。必要な状態は `get_state` / `set_state` を使う。

各物体には4個、ワールド全体で32個まで。ソース8 KiB、フィールド16個、明示的状態4 KiB、全Scriptデータ128 KiBまで。VMごとに4 MiBのメモリ制限と、1回の実行に2000回の割り込み上限を設け、System処理をフレームごとの予算で分ける。

Windowsの実行時間は現在のスレッドのCPU時間を基準に、5msを目安として割り込み時に検査する。OSの待機時間だけで正常な計算を失敗させないための処理で、別途100msの経過時間watchdogも適用する。`GetThreadTimes`の100nsは値の単位であり、計測分解能の保証ではない。CPU時計が取得不能・逆行・別スレッドになった場合は、その実行開始時からの経過時間5msへ戻す。Windows以外も経過時間5msを使う。

モジュールのコンパイル準備を終えてから初期化コードの実行予算を開始する。コールバックの関数取得・ctx準備にも新しい予算を設定し、コールバック直前に再設定する。同じProgramの同時呼び出しは直列化し、別の呼び出しが実行中の予算をリセットしない。割り込みによる協調的な制限なので、組み込みAPIの処理やコンパイルまで含む厳密なリアルタイム保証ではない。共有Writeの例外・予算超過はホストの診断を全員へ表示する。ローカルDriveの例外はその端末へ表示し、失敗した評価の結果を破棄する。Apply codeや設定更新で再評価する。

実装：[execution_clock.rs](../crates/scripting/src/execution_clock.rs)。Windowsの時間単位と現在スレッドの扱いは[GetThreadTimes](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-getthreadtimes)・[GetCurrentThread](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-getcurrentthread)を参照（確認日2026-10-27）。5msのCPU目安と100msのwatchdogはtukuriumの判断で、API自体が保証する制限ではない。

既存のset_positionなどは共有Writeの省略形として残る。drive(ctx)からの共有Writeとset_state、および共有コールバックからのDriveはエラーになる。既存のupdateコードを自動的にDriveへ変換はしない。

新しい実行器で使用できるライブラリはLuaの計算・文字列・テーブル等に限定する。ファイル・ネットワーク・モジュール読み込み、保護呼び出し、coroutineは公開しない。共有操作はホストでのみ実行し、通常の編集と同じTransform等の値域を検証する。

## 検証

`cargo test --workspace --locked`、`cargo clippy --workspace --all-targets --locked -- -D warnings` に加え、release Engineで `node tests/ugc-integration.mjs --rendered` を実行する。実際のGodot UIで追加・コード編集・パラメータ変更・停止・クリック・構文エラーの復旧を行い、ホスト・ゲスト・途中参加者、複製の状態、保存・再起動、無限ループ停止を検証する。描画テストの画像は `.run/ugc-*/script-editor.png`。

`--relay=wss://45.77.130.146/ws` では認証付きの公開中継を使い、テスト専用のランダムルームで同じ一連の検証を実行できる。この経路でも3クライアントで確認済み。認証情報は環境変数で渡す。

連続回転の検証で浮動小数点のJSON読み戻しに最下位桁の差が出たため、`serde_json` の `float_roundtrip` を有効にした。また、ゲストEngineが付け替えるネットワーク要求IDを元のUI要求IDへ戻して応答するようにし、複数編集UIの応答を識別できるようにした。
