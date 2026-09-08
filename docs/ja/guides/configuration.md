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

[package.metadata.shun.install]            # install ターゲット（既定）
local = true                               # 登録インストール（ARP、アンインストーラー、ショートカット）
portable = true                            # ポータブルモード（.shun-portable マーカー、レジストリ不使用）

[package.metadata.shun.webview2]           # Windows のみ
type = "skip"                              # skip | evergreen-installer | fixed-version
# path = "WebView2Runtime"                 # fixed-version 専用：展開済みランタイムフォルダー

[package.metadata.shun.flash]              # flash ターゲット（任意）
require-removable = true                   # リムーバブル以外のデバイスを拒否
```

## シェル UI

`[package.metadata.shun.shell]`（スタンドアロンドキュメントでは `shell` キー）
がランタイムシェルを設定します：

```toml
[shell]
timeline = "left"          # top（上部の横並び）| left（左側の縦並び）
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

[[custom-steps]]                           # markdown コンテンツステップの注入
key = "whats-new"
after = "license"
title = "What's New"
markdown = "docs/whats-new.md"
```

UI には 8 ロケール（`en`、`zh-Hans`、`zh-Hant`、`ja`、`ko`、`fr`、`ru`、`es`）
の既定テキストが同梱されています。`shell.language = "auto"` はシステムに追従し、
固定ロケールを指定すればそちらに固定されます。ロケール別ライセンスの上書きで
ローカライズされた契約書も機能します。
