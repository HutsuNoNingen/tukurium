# 共有WriteとローカルDrive

`update(ctx, dt)` / `on_interact(ctx)`はホストで共有データを変更する。`drive(ctx)`は各クライアントのRust Engineで計算し、共有データを変更せずに、その端末の描画と衝突判定で使う値を生成する。1つのスクリプトに両方のコールバックを定義できる。

## 回転する物体を作る

InspectorからScript Systemsを開き、Rotatorテンプレートを追加する。現在のRotatorは次の方式を使う。

```lua
return {
    name = "Rotator",
    fields = {
        speed = { type = "number", default = 45, min = -720, max = 720 }
    },
    drive = function(ctx)
        local entity = ctx.self
        local angle = entity.Rotator.speed * ctx.time
        ctx:drive(entity, "rotation", ctx:rotation_y(angle, entity.Transform.rotation))
    end
}
```

各フレームの角度を足し合わせず、経過時間から直接角度を求める。同じ設定・基準姿勢・時刻なら、更新回数が違っても同じQuaternionになる。コード、公開パラメータ、再生の基準時刻を共有する。毎フレームの回転結果は共有しない。

既存の`update`で回転するコードは引き続き共有Writeとして動く。自動変換はしないので、ローカル化する場合は上のように`drive`へ変更する。

## WriteとDriveのAPI

| API | 使用する場所 | 動作 |
|---|---|---|
| `ctx:write(entity, field, value)` | update / on_interact | 共有フィールド変更を要求し、通常の検証・commit経路で同期する |
| `ctx:drive(entity, field, value)` | drive | その端末の導出値を設定する。共有値とrevisionを変更しない |
| `ctx:write_field(field_ref, value)` | update / on_interact | スクリプトに登録した別Slotを含むフィールド参照へ共有値を書き込む |
| `ctx:drive_field(field_ref, value)` | drive | 参照先のフィールドへ端末内の導出値を供給する |
| `ctx:rotation_y(degrees, base?)` | どちらでも | 基準QuaternionへワールドY軸回転を加えたQuaternionを返す。基準省略時は単位回転 |
| `ctx.world_time` | どちらでも | ホストに合わせた共有タイムライン上の秒数 |
| `ctx.time` | どちらでも | スクリプトを追加してからの、停止・再開可能な経過秒数 |

従来の `ctx:write` / `ctx:drive` は取り付け先の物体の `position`、`rotation`、`scale`、`color` を対象にする。参照版の `write_field` / `drive_field` は、スクリプトの `field_ref` パラメータに登録された参照を使う。現在のWrite対象は `name`、Transform、`color`、`world_text`、`world_text_color`。Drive対象は `name` を除く同じフィールド。どちらもRustのComponent登録表で対象、型、値域を検証し、パネル操作などのコマンド専用フィールドには適用しない。従来の`set_position`、`translate`、`set_scale`、`set_color`、`rotate_y`は共有WriteのAPIとして残る。

インスペクタのフィールドをスクリプトのコード欄にD&Dするか、Ctrl＋右クリックで参照を持ってコード欄をクリックする。`Read`、`Write`、`Drive` の選択後に参照パラメータの宣言とコードの呼び出しを挿入する。`Apply code` で両方を保存する。SlotのIDをLuaの文字列として埋め込まず、複製時に参照を付け替えられる `field_ref` を使う。Writeは `update` / `on_interact`、Driveは `drive` の中に置く。

たとえばWorldTextをDriveで選ぶと `ctx:drive_field(ctx.self.TextClock.ref_world_text, string.format("%.1f", ctx.time))` のひな形を挿入する。文字列は端末内で更新され、共有の文字列はそのまま残る。`on_interact` から同じ参照へ `ctx:write_field` すれば共有の文字列が変わる。

Driveからの共有Write・`set_state`、共有コールバックからのDriveは拒否する。スクリプトが通信処理を書く必要はない。例えばクリックによる色変更は以下となる。

```lua
on_interact = function(ctx)
    ctx:write(ctx.self, "color", {1, 0.3, 0.1, 1})
end
```

Driveが読むTransformは共有された基準値であり、前フレームのDrive結果ではない。これにより、回転を二重に積み上げることを防ぐ。同じフィールドを複数のスクリプトがDriveした場合は、Component順で先のものを使用し、後のものに競合を表示する。先のDriveを外すと次の評価で競合を解消する。

## 時計・停止・途中参加

Engine自身が約2秒ごとにホストと時刻を照合する。往復時間の中点を使って補正し、遅延が大きいサンプルを避ける。ゲストは最初の時計合わせが完了してからDriveを実行する。Godotが時計やスクリプトの計算を担当することはない。

`ctx.world_time`はホストの単調増加時計に基づく絶対秒数で、0から始まるカウンターではない。必要なら共有パラメータの開始時刻との差を使う。`ctx.time`は共有された`ScriptClock { anchor, elapsed }`から導出し、Run / Pauseと連動する。Pause時は角度を保持し、Runで続きから動かす。停止中のDrive評価には停止時刻を渡すため、WorldTimeを参照するコードも停止位置を保持する。ただし再開時のWorldTimeは現在時刻へ戻る。停止時間を飛ばさずに再開したい動作には`ctx.time`を使う。

途中参加者は共有されたコード・基準値・時計のanchorから、その時点の姿勢を求める。保存には基準値とScriptClockを含め、Drive結果は含めない。動作中に保存したものは、再読込時に時計上で経過した時間を反映する。

同じ時刻を与えた計算結果は更新頻度に依存しないが、実際の端末間には時計合わせと通信遅延による推定誤差がある。現在のEngineの更新間隔は約16ms。GodotのFPSごとにLuauを実行する構成ではない。

## ECS内での流れ

1. `ScriptComponents`に保存されたコード・設定と、共有された基準値を読む。
2. 共通Systemの`drives::evaluate`が各端末でDriveを評価する。VMと直近の結果はローカルの`DriveRuntime` Resourceで管理する。
3. 結果を物体の`DrivenFields` Componentに格納する。共有Componentは変更しない。
4. `collision::sync_colliders`がDrive結果を反映した姿勢を使い、キャラクターの固定刻みシミュレーションへ渡す。
5. 同じ結果の`DriveFrame`を、そのEngineにつながったGodotへ渡す。KokoSyncには送らない。

Godotは値を描画へ反映する。SlotごとのGodotスクリプトを生成したり、UGCからGodotのノードへ直接アクセスしたりしない。つかみ操作やギズモの一時プレビューは、その操作対象フィールドについてDriveより優先する。

共有物理の担当ユーザー移譲、操作ユーザーでの共有Write実行、登録表で許可していないフィールドへのDrive、ローカルで導出した値同士の依存グラフはまだ実装していない。

## 実行予算と検証

VMごとのメモリ・コールバックの時間上限は共有スクリプトと同じ。Drive全体にも1評価回あたり約10msの処理予算を設け、評価順を巡回する。予算に入らなかったものは直近の有効な結果を保持するため、高負荷時の全スクリプト同時更新は保証しない。例外になったコールバックの出力はまとめて破棄する。

`cargo test --workspace --locked`では、30Hz / 144Hzでの姿勢一致、途中参加、停止・再開、Drive削除、共有Writeの拒否、競合、描画と同じQuaternionを用いた衝突を検証する。`node tests/ugc-integration.mjs --rendered`では、2つの実Engineが時計を合わせてローカル回転を生成し、その間共有revisionが増えないことと、Godot UI・Write・保存復元を検証する。
