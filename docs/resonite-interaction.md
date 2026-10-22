# Resoniteの右クリックとレーザーの調査

調査日：2026-10-22。インストール済みResonite **2026.9.18.82** の `FrooxEngine.dll` をILSpy 10.1.1で静的にデコンパイルした。実行時の操作感を測定した結果ではなく、下記のクラスと呼び出し関係を確認した記録。抽出したソースは `.run/resonite-interaction/` にある。

対象DLL：`C:\Program Files (x86)\Steam\steamapps\common\Resonite\FrooxEngine.dll`

SHA-256：`CD1F57C97408AADA9158E8C9D5C9ADAB05815DBB4A28756AA42681C6B462CEA8`

## 右クリックから保持まで

| 段階 | 確認した実装 | 挙動 |
|---|---|---|
| 機器入力を操作へ変換 | `KeyboardAndMouseBindingGenerator`、同名ソース198行 | 右マウスを主操作側の `InteractionHandlerInputs.Grab` に割り当てる。左マウスのInteract、RのSecondaryとは別操作。 |
| 押下・保持・解放 | `InteractionHandler` 1115〜1162行 | デスクトップではPressedでStartGrab、HeldでHoldGrab、ReleasedでEndGrab。VRのトグル保持には別の分岐がある。 |
| 対象を決める | `InteractionHandler.Grab` 2608行 | レーザーのCurrentHitからColliderを取り、TouchableのTryGrabやIGrabbableの判定へ進む。単に描画メッシュを移動させる処理ではない。 |
| 保持の基準点 | 同クラス2670行付近 | HolderSlotをレーザーのCurrentActualPointに置く。レーザーが当たった点をつかむため、物体の中心が照準へ瞬間移動する必要がない。 |
| 保持対象の収集 | `Grabber.Grab` 234行以降 | Colliderから親をたどってIGrabbableを探し、CanGrab、優先度、単体保持などを判定する。 |
| 距離を保持する | `InteractionHandler` 2705行付近 | スクリーン側のGetPointViewDistance、またはLaser.CurrentPointDistanceから初期距離を記録する。 |
| 解放 | `InteractionHandler.EndGrab` 2803行 | 保持状態・レーザーの固定点を解除し、Grabber.Releaseと関連イベントを呼ぶ。 |

## レーザーの役割

`InteractionLaser` は見た目の線と、操作対象の探索の両方を管理する。

- `FindHit`（544行）では複数のRaycast結果から候補を選ぶ。ツールによるIgnore/Prefer、Touchable、近接した表面などの条件があり、最も近いメッシュだけを採用する処理ではない。
- `RunRaycast`（1224行）ではPortalRaycastAllを使い、ローカルユーザー自身の近距離Collider、装備ツールの配下、InteractionModifierなどをフィルターする。
- `UpdateLaser`（699行）では入力側から渡されたorigin/direction、距離、平滑化設定を使う。保持中はmaintainDistanceを受け取る。先端にはDeltaとSmoothSpeedによる補間があり、角度による補間速度の調整もある。
- `StickCurrentPoint`（448行付近）は、当たった点を対象Slotのローカル座標へ変換して記録する。対象が動いても同じ物体上の点へ追従させるための情報になる。
- `UpdateLaserVisual`（1358行）では先端、色、透明度、表示・非表示を更新する。デスクトップではターゲットや設定次第で線を隠し、カーソルだけを出す分岐もある。常に長い線を表示する仕様ではない。
- `InteractionHandlerStreamDriver` にはLaserActive、ShowLaserToOthers、LaserTargetPoint、GrabDistanceのストリームがある。線の描画処理だけで共有しているわけではなく、操作状態と先端の情報を送って各端末で表示する。

## ホイールと回転

`KeyboardAndMouseBindingGenerator` 158〜170行で、縦ホイールをSlide、横ホイールをRotateへ割り当てている。入力にはユーザー設定の速度が掛かる。E＋マウスは自由回転、Shift＋Eは別の回転操作、Shift＋縦ホイールはスケール変更へ割り当てられる。

`InteractionHandler.SlideGrabbed`（3641行付近）は、保持距離の更新量を現在距離に応じて増やす。読み取れた計算は `max(1, distance) × 4 × slide × deltaTime`。一定メートルずつの操作ではなく、遠い物体ほど大きく動く。デスクトップ側の距離制限は0.05〜10000。Kokoverseへこの範囲をそのまま持ち込む必要はない。

## 今回のKokoverseへの反映

取り入れたのは、右ボタンの押下・保持・解放の区別、当たった点と物体中心のずれの維持、視点からの保持位置計算、距離に比例するホイール操作、E＋マウスの視点座標系での自由回転、Shift＋Eや横ホイールの垂直軸回転、Shift＋ホイールの均等拡縮。保持中の左クリックは方位を保って傾きを起こす（`AlignHeldItem` 3521・3706行）。ダブルクリックで水平回転追従と上下回転を含む追従を切り替える（`OnLaserHoldInputUpdate` 1329行）。判定とTransform計算はRust、入力と描画はGodotに置く。レーザーの状態はTransformのプレビューと同じ経路で送り、相手側にも保持中の線を表示する。

描画種別とは独立したColliderコンポーネントを使い、箱と画像を同じシステムで把持する。Colliderはローカル半径とSolidを持つ直方体で、位置・回転・スケールに追従する。画像も把持でき、初期状態ではプレイヤーの移動を妨げない。共有編集は3つのTransformフィールドのrevisionを確認し、解放時に1つのcommitで確定する。

現在の制限は、Collider形状が直方体、保持距離0.5〜6m、単体保持。Touchableやツールごとの優先順位、ポータル越しのレーザー、複数保持、投げる速度は未実装。Resoniteと同じ所有権・同期方式を再現したという意味ではない。
