# 配布マニフェストリファレンス

配布フローはアプリケーション自身の `Cargo.toml` 内の
`[package.metadata.shun]` テーブルで宣言します — cargo-deb / cargo-wix の
方式です。製品の識別情報は `[package]`（`name`、`version`）が既定値で、
テーブル内の残りの項目がフローをカスタマイズします。

```toml
[package.metadata.shun]
product = "ShunDemo"                       # 既定：パッケージ名
publisher = "celestia-island"              # ARP の Publisher 項目
logo = "docs/logo.webp"                    # シェルのロゴアセット
payload = "examples/demo_payload"          # アーティファクトに梱包するディレクトリ
main-exe = "bin/shun-demo.exe"             # payload 内のエントリーポイント

[[package.metadata.shun.attachments]]
key = "models"
title = "2D/3D model pack"
dest = "models"
[package.metadata.shun.attachments.online]
url = "https://example.test/models.shun"   # 任意の添付リソース（ライト版はインストール時にダウンロード）

[package.metadata.shun.install]            # install ターゲット（既定）
local = true                               # 登録インストール（ARP、アンインストーラー、ショートカット）
portable = true                            # ポータブルモード（.shun-portable マーカー、レジストリ不使用）
portable-marker = ".shun-portable"          # ポータブルコピーに書かれるマーカーファイル名（アプリが独自のマーカーを検出するなら上書き）
desktop-shortcut = "ask"                   # always | never | ask（ウィザードのチェックボックス、既定でオン）
start-menu-shortcut = "always"             # always | never | ask（既定は always、尋ねられるのはデスクトップの方）
scope = "ask"                              # user（既定）| machine | ask
deep-links = ["shundemo"]                  # アプリが保有する URL スキーム（myapp://…）
root-dir-folder = "ShunDemo"              # ドライブルート直下に自動で挟むフォルダー（D:\ → D:\ShunDemo、既定は製品名）

[[package.metadata.shun.install.verbs]]    # 右クリック動詞（Explorer 動詞 / デスクトップアクション）
key = "open-data"                          # 安定した動詞 id
display = "Open data folder"               # メニュー表示
target = "data-folder"                     # data-folder | uninstall | app
# arguments = "--safe"                     # app ターゲットのみ：追加 CLI 引数

[package.metadata.shun.webview2]           # Windows のみ
type = "skip"                              # skip | evergreen-installer | fixed-version
# path = "WebView2Runtime"                 # fixed-version 専用：展開済みランタイムフォルダー

[[package.metadata.shun.steps]]            # 順序付きウィザードパイプライン（任意）
columns = 2              # 任意：モードグリッドの列数（既定はモードごとに 1 列）
kind = "mode"                              # mode | scope | license | content | install
align = "center"                           # ステップ単位の上書き：center | start（既定は kind 次第）

[[package.metadata.shun.steps]]
kind = "content"
title = "Release notes"                    # content ステップはタイトルを持つ…
markdown = "notes.md"                      # …とドキュメント、ビルド時にインライン化

[[package.metadata.shun.steps]]
kind = "install"                           # install ステップはちょうど 1 つ

[package.metadata.shun.flash]              # flash ターゲット（任意）
require-removable = true                   # リムーバブル以外のデバイスを拒否
```

## ドライブルートの保護

ドライブなど、裸のファイルシステムルート（`D:\` のほか `D:`、
UNC 共有ルート `\\server\share`、POSIX の `/` も同様）が対象の
場合、ペイロードが直接そこへ置かれることはありません：
`InstallContext::apply_config` がその下に一段フォルダーを挟みます。
既定は製品名で、`install.root-dir-folder` でカスタマイズできます。
ウィザードはルートが選択・入力された時点でパス欄を書き換えるため、
表示される宛先は常に実際のものです。ヘッドレスの `--dir=D:\` 実行
もフロー内で同じ保護を受けます。

## シェル UI

`[package.metadata.shun.shell]`（スタンドアロンドキュメントでは `shell` キー）
がランタイムシェルを設定します：

```toml
[shell]
timeline = "left"          # top（上部の横並び）| left（左側の縦並び）
log-level = "all"          # all（既定）| files | scripts | off
log-order = "newest"      # newest（既定、最新が上）| oldest（末尾に追加）
language = "auto"          # auto | en | zh-Hans | zh-Hant | ja | ko | fr | ru | es

[shell.theme]
mode = "system"            # system | light | dark
accent = [34, 211, 238]    # RGB チャンネル — --color-primary を上書き
```

## payload の供給元

`[package.metadata.shun.source]` でインストール時の payload の供給元を選択：

```toml
[source]
type = "embedded"          # payload アーカイブはインストーラー本体に埋め込み
```

```toml
[source]
type = "online"            # インストーラーが payload をダウンロード
url = "https://github.com/<org>/<repo>/releases/latest/download/ShunDemo.shun"
```

オンラインインストーラーは **ダウンロード → 展開 → 検証** を 1 回のパスで
実行します：到着したバイトはマニフェストに照らして検証され、進捗イベントは
ダウンロードと展開の両フェーズを同時に報告します（多層プログレス）。`url`
をリリースフィード（GitHub Releases や任意の HTTP ホスト）に向ければ、新しい
パッケージの公開だけでインストーラーが更新されます。

## ライセンスとカスタムステップ

```toml
license = "docs/LICENSE.md"                # markdown、ライセンスステップで描画

[license-locales]                          # ロケール別ライセンスの上書き
ja = "docs/LICENSE.ja.md"
en = "docs/LICENSE.en.md"

[[licenses]]                               # 追加のライセンス文書
title = "Copyright notice"                 # 任意：本文上の見出し
path = "NOTICE.md"                         # markdown、マニフェスト相対
[licenses.locale-paths]                    # この文書のロケール別上書き
ja = "NOTICE.ja.md"

[[custom-steps]]                           # markdown コンテンツステップの注入
key = "whats-new"
after = "license"
title = "What's New"
markdown = "docs/whats-new.md"
```

`license` + `license-locales` は単一文書の省略記法（シュガー）です。
テーブル配列 `licenses` は追加の文書を宣言し、それぞれ任意の `title` と
専用の `locale-paths` を持てます。両者は組み合わせ可能で、省略記法の
文書が先、その後に宣言順で配列が続きます。一致するロケールのパス
（`license-locales` または `locale-paths`）は基本の文書より優先され
ます。ライセンスステップは文書を一度に 1 つ表示し、複数が解決された
場合は前へ/次へのページャーで切り替えられます。同意チェックボックスは
1 つで全文書を対象とします。解決済み JSON では各文書が `licenses`
（title + body）として運ばれ、従来の `body` 文字列は全本文を区切り行で
連結したものになるため、`body` しか読まない描画側でも契約全体を表示
し続けられます。

UI には 8 ロケール（`en`、`zh-Hans`、`zh-Hant`、`ja`、`ko`、`fr`、`ru`、`es`）
の既定テキストが同梱されています。`shell.language = "auto"` はシステムに追従し、
固定ロケールを指定すればそちらに固定されます。ロケール別ライセンスの上書きで
ローカライズされた契約書も機能します。
