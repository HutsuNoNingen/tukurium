# 表示言語の切り替え

Escでドックを表示し、設定（⚙）→「言語」から `日本語` / `한국어` を選ぶ。
選択は開いている画面へ即時反映し、次回起動にも引き継ぐ。初期値は日本語。
言語を切り替えても入力中の名前、コード、インスペクタの編集ドラフトは保持する。

言語はGodotクライアントの個人設定で、`user://interface.cfg` の `[interface] language` に保存する。
共有Componentやワールドの保存データには入れない。参加者ごとに異なる言語を使える。
保存値が未対応の言語・型だった場合は日本語を使う。

## 翻訳を追加する

`client-godot/app_language.gd` が `locale/ko_shell.json`、`ko_services.json`、`ko_tools.json` を読み、
GodotのTranslationServerへ登録する。キーは元の表示文、値は韓国語。
静的なControlのラベル・ツールチップ・プレースホルダーはGodotが翻訳する。
値を埋め込む文は `AppLanguage.format_text(control, "文 %s", [value])` を使い、
書式と値を分けて言語変更時に再描画する。後続の状態表示で上書きされた古い文は再適用しない。

参加者名、セッション名、作品名、説明、チャット、コード、ワールドのテキストは翻訳対象にしない。
そのまま表示するControlは `auto_translate_mode` を無効にする。
ComponentやFieldのID、通信キー、保存形式、実行ファイル名は言語によって変更しない。

## 確認

`node tests/localization-integration.mjs --rendered` で設定の実入力、即時切替、
再起動後の復元、日本語への復帰、未対応設定のフォールバック、ユーザーの文の保持を確認する。
テストは `.run/` 内の独立した設定ファイルを使い、通常の個人設定を変更しない。
ランタイムとテストの前提は [Windowsビルド手順](windows-build.md) を参照する。
