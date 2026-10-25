# Resoniteのローカル実行と同期

調査日: 2026-10-23。公式Wiki・開発者の説明と、このPCのResonite 2026.9.18.82の静的解析を照合した。実行中セッションの通信量や遅延は計測していない。

## 結論

Resoniteでは、ProtoFluxの処理は各クライアント内で実行する。共有するのは主にSlot・Component・フィールド・ノード接続などのデータモデル。あるユーザーが処理を実行し、通常の共有フィールドを書き換えると、その変更が他のクライアントへ伝わる。処理の呼び出しそのものをすべてホストへ送って実行させる方式ではない。

ただし「すべての処理を全員が実行する」でもない。イベントの発生元や更新ノードの設定によって、実行するユーザーが変わる。

開発者による説明は [The Resonance 2025-03-30、1:39:27–1:44:01](https://wiki.resonite.com/The_Resonance/2025-03-30/Transcript) にある。Impulseは特定ユーザー上で始まり、そのユーザー上で完了する。共有データへの変更を同期する仕組みは、Impulseの実行とは分かれている。リンク先は自動文字起こしのため、以下の実装とも照合した。

## 誰が実行するか

| 処理 | 実行する場所 | 確認した根拠 |
|---|---|---|
| 通常のImpulseの連鎖 | そのImpulseを開始したユーザーのクライアント | 開発者説明、`FrooxEngineContext.LocalUser` |
| `Local Update` | 各ユーザーが自分の更新ループで実行 | `LocalUpdate.AddedToScope/Update`、[公式Wiki](https://wiki.resonite.com/ProtoFlux:Local_Update) |
| `Update` | `UpdatingUser`で指定したユーザー。未指定かつ`SkipIfNull=false`ならホスト | `UserUpdateBase.ShouldRegister`、[公式Wiki](https://wiki.resonite.com/ProtoFlux:Update) |
| Driveの入力評価 | 各クライアント内の実行系 | `FieldDriveBase<T>.Changed`と同期要素のDrive処理 |

`UserUpdateBase.ShouldRegister`は、対象ユーザーが未指定なら条件に応じて`World.HostUser`へ置き換え、最後にそのユーザーが`IsLocalUser`かを確認する。`LocalUpdate`にはこのユーザー選別がなく、自分の実行コンテキストの更新通知へ登録する。

## 何が同期されるか

### Write: 共有する値を変更する

共有フィールドへの通常のWriteはデータモデルを変更する。`SyncField<T>.InternalSetValue`から変更検出と同期対象の登録につながる。例えばクリックした本人が共有の色フィールドを書き換え、他の人はその色の変更を受け取る。

[Writeの公式説明](https://wiki.resonite.com/ProtoFlux:Write)にも、データモデルのフィールド変更はネットワークを通じて反映される一方、ローカル化されたフィールドやStore/Localは例外とある。[Impulseの説明](https://wiki.resonite.com/Impulses)も、処理のローカル実行とデータモデルの変更同期を区別している。

### Drive: 各端末で値を導出する

Driveは、接続された計算結果から対象の値を各端末で求める。同期された入力や共通の時刻から連続動作を計算する用途に向く。

静的解析で確認した箇所:

- `FieldDriveBase<T>.Changed`: 有効なDriveリンクの対象へ、入力を評価した値を代入する。
- `SyncElement.InvalidateSyncElement`: 有効なDriveリンクがある場合、通常の同期差分登録を抑止する。
- `ConflictingSyncElement.Validate`: Drive中の要素への受信更新を`Ignore`として扱う。

したがって、Driveの最終値を毎フレーム全員へ送る、と考えるのは誤り。ノード構成・参照・元の共有値と、各クライアントで導出される結果を分ける必要がある。各端末で同じ値になるかは入力にも依存し、ローカルユーザーを入力にするとユーザーごとに異なる値を導出できる。

### 保存する状態と一時変数

[ProtoFluxの変数説明](https://wiki.resonite.com/index.php?title=ProtoFlux/en)では、Data Model Storeは同期され、Storeはネットワーク同期されず、Localは実行チェーンごとの一時変数となる。コードの定義や接続が共有されていても、実行中の変数がすべて共有されるわけではない。

## ホストの役割は残る

ローカル実行だからといって、ホストが不要になるわけではない。`ConflictingSyncElement.Validate`には、ホスト側で変更元ユーザー・版・SyncTickを確認する経路がある。指定ユーザーがない`Update`の実行先にもなる。

「誰が計算するか」と「共有変更をどう確定・整合させるか」は別の責務。静的解析だけから、競合やホスト移譲の全ケースを保証することはできない。

## 現在のtukuriumとの差

現時点のLuau UGCは、`WorldModel::advance`が`script_authority`を確認し、ホストのみが約20Hzで`script_schedule`を実行する。Engine側は`set_script_authority(self.host)`を設定する。ゲストはコードとパラメータを保持するが、UGCの計算結果はホストから受け取る。

アバター移動・ジャンプ・重力は別で、各クライアントのRust Engineがローカルプレイヤーを固定時間刻みで計算する。Godotは入力と描画を担当する。

## 今後の設計候補（未実装）

実行範囲と書き込み先を独立した設定にする。

| 実行範囲 | 用途 | 書き込み |
|---|---|---|
| 各クライアント | 個別UI、視線追従、見た目の連続動作 | ローカルの導出値。共有フィールドへ毎フレーム書かない |
| 操作したユーザー | ボタン、道具の操作 | 検証された共有データ変更を提出 |
| 指定した権限ユーザー | 共有物理、単一のタイマーや進行 | 共有データ変更。実行権限の移譲が必要 |

最初に、各クライアントのRuntimeと共有データを分離し、共有値から計算するローカルの表示用値を追加する。回転する飾りなら基準姿勢・開始時刻・速度を共有し、見た目の姿勢は各端末で計算する。ただし、足場や衝突にも使う姿勢は表示だけの値にせず、物理側との整合を設計する。

次に、操作ユーザー上のコールバックが共有フィールド変更を提出する経路を追加する。共有の変更履歴を確定する役割は、当面既存ホストに残してもよい。各ユーザーが計算することと、全員が無制限に共有値を書き換えることは別になる。

必要な仕組みは、ローカル上書きと共有の値の区別、複数Driveの競合・解除、実行者/操作対象の検証、要求の重複適用防止、途中参加時の復元、実行権限の移譲。現在のホスト判定を外すだけでは、全員のスクリプトが同じ共有値を書き換えて競合する。

この調査後、ローカルDriveと共有Writeの分離を実装した。現行のAPIと残る範囲は [ローカルDrive](ugc-driving.md) を参照。操作ユーザー上での共有Writeや実行権限の移譲は未実装。

## 調査対象と再確認

以前の抽出物と現在のDLLのSHA-256が一致することを確認したうえで、同期要素・Context・Driveの抽出コードを再読した。今回`Update`、`LocalUpdate`、`UserUpdateBase`もILSpyCmd 10.1.1.8388で抽出して照合した。抽出物はローカル調査用で、リポジトリには含めない。

| DLL | SHA-256 |
|---|---|
| FrooxEngine.dll | `CD1F57C97408AADA9158E8C9D5C9ADAB05815DBB4A28756AA42681C6B462CEA8` |
| ProtoFlux.Nodes.FrooxEngine.dll | `CA830285B0FA454BF21179BE22D7B06FFECACF30BFB188D1E1F257DE32F560AA` |

追跡した型は、前者の`FrooxEngine.SyncElement`、`FrooxEngine.ConflictingSyncElement`、`FrooxEngine.SyncField<T>`、`FrooxEngine.ProtoFlux.FrooxEngineContext`、`FrooxEngine.ProtoFlux.CoreNodes.FieldDriveBase<T>`と、後者の`ProtoFlux.Runtimes.Execution.Nodes.Actions`内の3型。
