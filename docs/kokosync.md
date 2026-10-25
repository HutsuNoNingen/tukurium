**KokoSyncはPC間のデータパイプとして使う**

更新日：2026-10-22。tukurium独自のメッセージをKokoSyncで配送する。Tonariumとのデータ互換性は設計目標に含めず、そのメッセージ型、保存形式、認証プロキシ、暗号化エンベロープは持ち込まない。ECS、ObjectRoot、フィールドの意味、編集の承認はtukurium Engineが管理する。

参照コミットは `5a171af078d3cf85cc3364c9358e1b993f4f9835`。同じコミットのGo SDKを使うアダプターを実装し、ローカル中継を経由した2つのEngineの編集・姿勢同期を検証した。本書は将来の機能も含む接続方針。現時点の配送・再接続と制限は [runtime.md](runtime.md) を参照。姿勢にはlatestを使用し、Blobはまだ使っていない。

**エンジンは通信ライブラリの外にワールドモデルを持つ**

```text
Godot ⇄ ローカルIPC ⇄ tukurium Engine
                           ⇅ tukurium独自のメッセージ
                     KokoSync Adapter
                           ⇅
                     KokoSync中継
                           ⇅
                     相手のAdapter ⇄ 相手のEngine ⇄ 相手のGodot
```

Adapterは接続、送受信、相手情報、通信能力、配送エラーを扱う。Engineから見た責務は、確実に届けたいメッセージを送ること、位置などの新しい値を送ること、受信メッセージを通知することに限定する。ObjectRootやSlotをKokoSyncの共有状態へ直接マッピングしない。

ローカルのGodotとEngineの通信は、既存案のTCP IPCを維持する。KokoSyncはPC間の経路に置く。GodotがKokoSyncへ直接参加して、Engineを通さずワールドを書き換える構成にはしない。

**アプリのメッセージ種別ごとに配送を選ぶ**

| tukuriumのメッセージ | KokoSyncの候補API | Engineの処理 |
|---|---|---|
| EditCommand | 権威側peerへの `sendTo` | 型・権限・revisionを検証して採否を決める |
| WorldCommit | 権威側から `message` | worldRevision順に確定変更を適用する |
| CommandRejected / CommandResult | 要求元への `sendTo` | 保留中のUI操作を解決する |
| PoseSample | `latest` | 接続世代・所有者・連番を確認し、新しい値を採用する |
| SnapshotRequest / WorldSnapshot | `sendTo`。大きければアプリ側で分割またはBlob転送 | 初回同期・再同期のために検証して一括反映する |

`sendTo`の成功は相手SDKによる受け付けであり、編集の承認やECSへの適用完了ではない。編集の成功はEngineが返すCommandResultまたは対応するWorldCommitで確定する。タイムアウトなどで結果が不明なら、同じ操作を別IDで自動再実行せず、同じcommandIdの結果照会か再同期で解決する。

`latest`は途中の値が失われても次の値で補える位置・姿勢に使う。生成、削除、部品追加、最後に確定した配置はWorldCommitへ載せる。PoseSampleだけでワールドの構造や永続状態を作らない。

KokoSyncのルーム連番とtukuriumのworldRevisionは別物とする。KokoSync上には配送・管理用の別メッセージも流れ得るため、ワールドの確定変更にはEngineが独立してrevisionを振る。KokoSyncの配送順序や履歴を、アプリ側の整合性規則と同一視しない。

**メッセージの中身はtukuriumだけで定義する**

共通エンベロープの候補は `protocolVersion`、`sessionId`、`authorityEpoch`、`kind`、`payload`。EditCommandには `commandId`、対象の `(instanceId, localId)`、`fieldId`、`expectedFieldRevision`、型付きの値を入れる。WorldCommitには `worldRevision`、元のcommandId、確定した変更の配列を入れる。

InstanceIdはUUID文字列、localIdとfieldIdは範囲を定めた整数にする。64bitのrevisionなどはアプリのペイロード内では10進文字列として扱い、JSON IPCやSDK間の安全な整数範囲に依存させない。KokoSync自身のプロトコルカウンターのuint64形式は変更しない。

ECSの内部Entity番号、GodotのNodePath、メモリアドレスは送らない。値の型とスキーマ版を含め、受信側が対象や型を解決できないときは適用せず、互換性エラーまたは再同期として扱う。

**KokoSyncのルームとゲームの権威側を分ける**

KokoSync中継はデータを配送するサービスであり、ゲームのホストではない。tukuriumのホストEngineが正本を持ち、ゲストからの編集要求を順序付ける。初版の共有状態を変更するFluxもホストで実行する。

ルーム名はtukurium専用の接頭辞とsessionIdから作る。ルーム参加時に権威側の接続とauthorityEpochを確定し、その接続以外からのWorldCommitやWorldSnapshotを採用しない。メッセージ本文の「自分はホスト」という宣言だけでは判定しない。KokoSyncのpeer名は表示・接続識別子であり、ユーザー認証の代用にはしない。

ホストが切断した場合、初版は編集を停止して切断を表示する。自動で別のpeerをホストに昇格させない。ホスト移譲や再開時のepoch変更は、その機能を追加するときに扱う。

**初回同期は、Engineが作ったスナップショットを受け取る**

ゲストは受信を開始してからSnapshotRequestを送る。ホストは更新の区切りでrevision Rの状態を切り出し、WorldSnapshotとして返す。ゲストはスナップショット取得中に届いたWorldCommitを上限付きで保留する。

スナップショットを検証・反映した後、R以下の変更は重複として捨て、R+1以降を連続した順番で適用する。revisionの欠落、メモリ上限超過、転送失敗では、部分的なワールドを正本として使わず、取り直す。

KokoSyncの `state` や `checkpoint/catchup` は初版では使わない。KokoSyncの共有状態とEngineのECSを、別々に書き換えられる正本にしないためである。後から履歴配送を利用する場合も、tukuriumの確定変更とスナップショットの対応を定義してから導入する。

保存ファイルはEngineが作る。中継のメモリ状態やチェックポイントを、サーバー再起動後も残る保存先とは扱わない。

**Rustへの接続にはAdapterの実装が必要**

確認したGitHubのSDKはJS / TypeScript、C#、Goであり、Rust SDKはない。最初からSDKをRustへ移植すると、分割転送、流量制御、再接続世代、配送結果の意味まで対応する必要がある。

初期候補は、Go SDKを使う小さな `kokosync-adapter` プロセス。Rust EngineとローカルIPCで接続し、Go SDKのQUIC経路から中継へ参加する。ECSはGoへ移さない。この候補を採ると各クライアントはGodot、Rust Engine、Go Adapterの3プロセスになる。起動、終了、切断通知、待機量の上限もAdapterとの契約に含める。

後からRust SDKを追加する場合は、Engine側のTransportインターフェースを維持してAdapterの実装を差し替える。Rust用WebSocketクライアントを直接実装する案もあるが、現在のSDKをそのまま使えるわけではなく、位置配送も信頼性のあるストリーム上になる点を考慮する。

Go SDK経由の経路は現状では中継になる。ブラウザ間の直結機能が存在しても、Rust / Goのネイティブ参加者にそのまま適用できるとは仮定しない。

**接続試作で確認すること**

最初は独立したtukurium用ルームへ2つのAdapterを接続し、小さなEditCommandとWorldCommit、PoseSampleを往復させる。続けて、実際の箱の色変更、同時編集の拒否、途中参加のスナップショット、切断後の取り直しまで確認する。

配送エラーを成功扱いしないこと、SDKの受信確認だけで編集UIを確定しないこと、古い接続世代の姿勢や変更を採用しないことを検証する。大きなスナップショットの転送中にもGodotの入力・描画を継続できることを確認する。

現行KokoSyncにはメッセージ・転送・履歴などの容量制限がある。接続時の能力と上限を確認し、それに合わせてスナップショットを分ける。到達遅延、同時人数、ネイティブ直結、自動再接続を未検証のまま保証しない。

参照した一次資料：

- [KokoSync README](https://github.com/katteni-kaihatu/kokosync/blob/5a171af078d3cf85cc3364c9358e1b993f4f9835/README.md)
- [実装範囲](https://github.com/katteni-kaihatu/kokosync/blob/5a171af078d3cf85cc3364c9358e1b993f4f9835/docs/implementation-status.md)
- [Go SDK](https://github.com/katteni-kaihatu/kokosync/blob/5a171af078d3cf85cc3364c9358e1b993f4f9835/sdk/go/README.md)
- [実行版の通信仕様](https://github.com/katteni-kaihatu/kokosync/blob/5a171af078d3cf85cc3364c9358e1b993f4f9835/protocol/prototype.md)
