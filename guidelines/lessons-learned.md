# 不具合から得た知見を次の実装へ引き継ぐ

確認日：2026-10-24。以下は再現・修正を確認した記録。
現在の操作規則は [3Dパネルの実装指針](world-space-panels.md) を参照する。

## パネルを開くと歩けなくなる

最初の新規作成パネルは3Dに描画していたが、表示中はHUD全体を入力停止の対象にしていた。
見た目が3Dでも、入力処理は移動を止めるダイアログになっていた。

新規作成を共有WorldPanelへ移し、インスペクタと同じ入力経路を使うようにした。
「3Dの道具を開いていること」と「個人のメニューが移動入力を止めること」を同じフラグで扱わない。

- 確認：パネルを開いたままの移動・ジャンプ・保持・回転・拡縮、共有と途中参加。
- コード：[world_hud.gd](../client-godot/world_hud.gd)、[world_panels.gd](../client-godot/world_panels.gd)。
- テスト：[creation_smoke.gd](../client-godot/creation_smoke.gd)。修正：`ba2a9bc`。

## FPSでボタンを押すとカーソルに切り替わる

ワールドパネルの左ボタン押下で、無条件にマウスを解放していた。
さらにドラッグ中の移動イベントをUIだけで消費すると、視点も動かなくなった。

FPS中は押下・ドラッグでも捕捉を維持し、必要な移動イベントを視点操作へも渡す。
UIの入力先とOSのマウス捕捉を別々に管理する。

- 確認：視点中央でのタブ・ボタン操作、ドラッグ中の視点移動、TPSのカーソル操作。
- コード：[world_panels.gd](../client-godot/world_panels.gd)。
- テスト：[panels_smoke.gd](../client-godot/panels_smoke.gd)、[creation_smoke.gd](../client-godot/creation_smoke.gd)。修正：`cae589d`。

## 余白を押してもNameの編集が残る

入力欄以外のUIはフォーカスを受け取らないため、前の入力欄にキーボード入力が届き続けた。
カメラの移動で交点が変わった場合、UIのホバー状態も更新が必要だった。

左クリックを転送する前に、実際の交点でホバーを更新する。
以前の入力欄やその子を押していない場合は、フォーカスを外してから押下を渡す。
解除時には押下対象の記録も消す。

- 確認：Name・Color・Positionの選択、Colorの入力でNameが変わらないこと、余白で編集リースが終了すること。
- コード：[world_panel.gd](../client-godot/world_panel.gd)、[world_panels.gd](../client-godot/world_panels.gd)。
- テスト：[panels_smoke.gd](../client-godot/panels_smoke.gd)。修正：`101e31c`。

## 見出しの上では右クリックでパネルを持てない

フィールド名などの参照部品が右クリックを先に受け取り、パネル保持と競合していた。
保持中に同じ部品の上へ移ると、解除イベントまでUI側が受け取る可能性もあった。

通常の右クリックは保持に統一した。参照はCtrl＋右クリック、値のコピーはCtrl＋Shift＋右クリックに分けた。
保持中の解除を参照部品へ渡さない。修飾キー付きの操作でも、古いホバーではなく現在の交点を確認する。

- 確認：見出しからの保持、参照を誤取得しないこと、修飾キーを押した状態での解除、参照の受け渡し。
- コード：[reference_chip.gd](../client-godot/reference_chip.gd)、[world_panels.gd](../client-godot/world_panels.gd)。
- テスト：[panels_smoke.gd](../client-godot/panels_smoke.gd)、[reference_smoke.gd](../client-godot/reference_smoke.gd)。修正：`893d5b8`。

## 配布用アバターの再インポートが不安定になる

配布用コピーにはGodotの変換済みリソースを入れていたが、対応するインポートのチェックサムを入れていなかった。
再インポート時、GLBとそこから生成するテクスチャの処理でファイル読み込みエラーを観測した。

変換済みリソースと一緒に対応する`.md5`をコピーするようにした。
インポートキャッシュを配布する場合は、変換後のファイルだけで完結すると考えず、元データとメタデータも揃える。

- 確認：配布プロジェクトのGodotインポート、ZIP展開後の起動、ファイルハッシュ。
- コード：[copy-avatar.ps1](../scripts/copy-avatar.ps1)。修正：`26e8238`。
