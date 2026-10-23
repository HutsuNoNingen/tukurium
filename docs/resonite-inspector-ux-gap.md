# ResoniteのインスペクタとKokoverseのUX差分

調査日：2026-10-23。Kokoverseは `bd2c7fd` と現在の作業ツリーを確認した。

いちばん大きな差は、**階層・Component・フィールドの参照を画面上で扱い、物体の構成を組み替える操作**にある。共有パネルの生成、把持、ピン留め、即時反映はすでにある。次はComponent単位の表示とDriveの可視化から着手し、親子構造と型付き参照へ進むのがよい。

この文書は、[共有ワールドパネルの調査](resonite-world-inspectors.md)に対する追加調査。前回以降、共有インスペクタ内のLua共同編集とローカルDriveを実装している。前回文書の「Scriptドラフトはローカル」という記述は当時の状態で、現在の共有パネルには当てはまらない。独立した2Dスクリプト画面は、引き続き共同編集とは別の経路を使う。

## 現在との差

| 操作・表示 | Resoniteで確認した内容 | 現在のKokoverse | 必要な変更 |
|---|---|---|---|
| 対象を探す | 左にSlot階層、右に選択対象のプロパティとComponent。表示階層のRootと選択対象を別々に保持 | 上部のフラットなオブジェクト選択欄。`InspectorView`に対象参照が一つある | 左右ペイン、Rootと選択の分離。実際の親子構造はRust側にも必要 |
| 選択して編集する | 階層の選択がワールド上の選択に結び付く。参照を持った状態から別のインスペクタも開ける | 対象の表示用ギズモは出るが、操作対象にするには`Select / Gizmo`を押す。`Ctrl+I`は現在の選択、`I`は2D表示切替 | 注視・クリック・階層選択から編集までの導線を統一 |
| Componentを調べる | 型名を見出しに、各Componentのメンバーを表示。複製・削除・ヘルプを持つ | RustにComponentスキーマはあるが、共有画面は`object.fields`を一列に並べる | スキーマからComponentごとのセクションを生成 |
| 構成を変える | Componentの追加、ヘッダーを掴んで別Slotへ移動・複製 | Collider等は個別機能、Luaは専用の追加・削除UI。任意Componentの追加・移動はない | Componentのライフサイクル操作と検証、インスタンスの識別方法 |
| 参照・値を渡す | Slot、Component、フィールド参照や値を掴み、型が合う欄に渡す | パネル対象は参照だが、ユーザーが掴んで渡せる汎用参照UIはない | 参照と値を区別したペイロード、型検査、ドロップ先の表示 |
| 値を編集する | 型に応じた数値、色、列挙、参照、リスト等の編集欄。フィールド操作メニューもある | 文字列、ベクトル、回転、RGBA、Collider、再生操作を個別実装。他はJSON表示になる | 型別エディターの拡充。既定値に戻す操作、色のプレビュー等 |
| Driveを理解する | Drivenは紫、Linkedはシアン。メニューから制御元を開き、解除できる | 物体はローカルDriveで動くが、フィールド欄には共有値が出る。制御元への導線はない | 保存値とこのクライアントの評価値、制御元、編集可否を表示 |
| 元に戻す | Slot・Componentの生成や複製、削除、フィールドのリセット等にUndoを使用 | Lua本文の編集機能とは別に、ワールド操作を戻す汎用Undoはない | ホストが検証する操作履歴。共同編集で他人の後続変更を壊さない規則 |
| パネルを一緒に使う | ワールド内パネル、ピン、複数画面、共有スクロールと入力状態 | これらは実装済み。Lua本文には複数人のカーソルもある | 既存機能を維持して新しい表示構造へ接続 |

Resoniteの階層、Componentの移動・複製、参照から別の画面を開く操作は[Scene Inspectorの公式説明](https://wiki.resonite.com/Scene_Inspector)にも記載されている。キーの割り当ては[Controls](https://wiki.resonite.com/Controls)と照合した。上表の細かな生成・編集処理は、後述するローカルの静的解析でも確認した。

## 「箱を回す」操作で違いを見る

現在は箱を選び、共有インスペクタのScriptsへ切り替え、Rotatorを追加して速度を編集する。ローカルDriveの実行とLua共同編集は使える。ただし、そのスクリプトがどのフィールドを制御しているかを通常のフィールド欄からたどれない。回転の欄を直接変えても、実際の表示はDriveの評価値で決まるため、意図した変更が見えない場合がある。

Resoniteでは回転フィールドの状態から制御元を開ける。フィールド参照を持ち出して別の機能へ渡す入口もある。今回の静的解析では、`MemberEditor.GetFieldColor`がDrivenとLinkedを色分けし、`InspectorMemberActions`が制御元を開く操作と解除操作を作ることを確認した。[Linkageの公式説明](https://wiki.resonite.com/Drive)とも一致する。

Kokoverseでまず用意したい表示は、たとえば「Rotation · Local Drive」「制御元：Rotator」「共有の保存値」「現在の評価値」。制御元を押すと、そのスクリプトの編集画面を開く。解除については現在の値を保存するのか、元の共有値に戻すのかを決めてから実装する。見た目だけで操作を追加すると、値が跳ぶ理由が分からなくなる。

## 左右ペインだけでは親子編集は成立しない

現在の`Slot`が持つのは名前だけで、親への参照がない。`LocalTransform`と`GlobalTransform`はあるが、ユーザーが編集できるSlot階層にはなっていない。Godotが読み込んだモデル内部のNode階層も、そのまま共有編集できるSlot階層を意味しない。

「空の子を作る」「箱の下に画像を入れる」「親ごと動かす」を成立させるには、Rustに親子関係とTransformの伝播を追加する必要がある。親変更時にワールド座標を保つか、親削除時に子も消すか、循環をどう拒否するかを決め、保存・途中参加・衝突判定・Driveと同じ構造を使う。

一方、Componentごとの表示は既存の`ComponentSchema`と`ComponentState`を利用できる。現在の登録方式は型ごとに固定の`local_id`を割り当てるため、同じ型のComponentを任意個追加・複製するには拡張が必要。Luaが最大4本付く機能は`ScriptComponents`内の配列であり、汎用Componentの複数インスタンス対応とは異なる。

## 参照を渡す操作はLuaにも使える

型付き参照を追加するなら、Slot参照、Component参照、Field参照を区別する。さらに「現在値のコピー」と「フィールドへの参照」は別の操作として扱う。参照先の型が合わない欄には入れず、削除された参照先も表示できるようにする。

この仕組みをLuaの公開パラメータにも接続すれば、ユーザーは対象をコード内の文字列で指定せず、箱のPositionをパラメータ欄へ渡せる。これは今後の実装案で、現在の`ctx:drive`は自分に付いた物体の限られたフィールドを扱う。

Resoniteの操作から取り入れたいのは、この参照の受け渡し。Luaの共同編集は今の方向を維持できる。今回確認したResoniteの通常のTextFieldは他ユーザーのフォーカスを確認する方式であり、Google Docs型の本文共同編集と同じ実装だとは判断していない。

## 実装する順番と完了条件

以下はKokoverseへの提案。工数の大小は既存コードとの依存関係による相対評価で、日数の見積もりではない。

| 順番 | 範囲 | 大きさ | 完了を判断する操作 |
|---|---|---|---|
| 1 | Component単位の表示、Drive状態と制御元、値の編集欄を整理 | 小〜中 | 回る箱を開き、どのスクリプトがRotationを制御するか分かり、その編集へ移動できる |
| 2 | Rustの親子構造、左右ペイン、Rootと選択、親変更 | 大 | 子として置いた画像が親の箱に追従し、別の参加者と途中参加者にも同じ階層・配置が見える |
| 3 | 型付き参照、掴む・渡す、参照先を開く、Luaパラメータへの接続 | 中〜大 | パネルAのField参照をパネルBへ渡せる。型違いを拒否し、参照先の削除も扱える |
| 4 | 任意Componentの追加・複製・削除・移動、ワールド操作のUndo | 大 | 同じ型を2個付けて区別でき、2人で編集してもUndoが他人の変更を黙って上書きしない |

Undoの設計は手順2の構造操作を作る時点から始める。最後に一括で追加しようとすると、操作の記録方法を作り直すことになる。

画面は、左を対象一覧・将来の階層、右を選択対象のComponent群にする。Luaの長い本文は、Componentから開く専用の共同編集画面に置く。現在の720×960のパネルに階層とコードを常時詰め込むと編集領域が狭いため、物理サイズと解像度、別パネルで開く導線を合わせて調整する。

## 根拠と未確認の範囲

Resoniteはインストール済みの**2026.9.18.82**を対象にした。`FrooxEngine.dll`のSHA-256を今回も照合し、`CD1F57C97408AADA9158E8C9D5C9ADAB05815DBB4A28756AA42681C6B462CEA8`であることを確認。ILSpy **11.1.0.9782**で抽出したコードを調べた。抽出物は`.run/resonite-inspector/`に置き、製品コードはコミットしない。

| 確認対象 | 主な根拠 |
|---|---|
| Rootと選択、追加・削除・複製、ギズモ | Resonite `SceneInspector`、`SlotInspector` |
| 左右の幅と独立したスクロール | Resonite `InspectorPanel.Setup` |
| Componentヘッダー、型別フィールド、参照と値の受け渡し | Resonite `WorkerInspector`、`SyncMemberEditorBuilder` |
| Driveの色、制御元を開く、解除、リセットとUndo | Resonite `MemberEditor`、`InspectorMemberActions` |
| 現在の共有画面と参照先 | Kokoverse [`inspector_view.gd`](../client-godot/inspector_view.gd)、[`world_panels.gd`](../client-godot/world_panels.gd)、[`panels.rs`](../crates/protocol/src/panels.rs) |
| Component登録と親子構造の現状 | Kokoverse [`replication.rs`](../crates/core/src/replication.rs)、[`components.rs`](../crates/core/src/components.rs) |
| Driveの表示、共同編集 | Kokoverse [`main.gd`](../client-godot/main.gd)の`apply_drive_frame`、[`shared_script_view.gd`](../client-godot/shared_script_view.gd)、[`script_editor.gd`](../client-godot/script_editor.gd) |

Kokoverseの既存レンダリング結果も確認した。共有フィールド画面は`.run/panels-90216e21-fbeb-44c9-ba80-84745b9cca03/world-panels.png`、共同編集画面は`.run/script-docs-7952457e-0268-446f-b558-15377d8a54bf/shared-script.png`。前者はLua共同編集の追加前の画像なので、現在の状態はソースと合わせて判断した。

今回は実装と既存画像の比較であり、Resoniteを起動して操作時間やクリック数を測った調査ではない。VRでの操作感、遅延時の入力競合、日本語IME、パネルを多数開いた性能は未測定。汎用フィールド検索や数値のドラッグ操作など、今回根拠を確認していない機能は差分として断定していない。
