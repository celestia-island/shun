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
desktop-shortcut = "ask"                   # always | never | ask（ウィザードのチェックボックス、既定でオン；ヘッドレス実行は ask をショートカットの存在で解決）
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
# silent-install = true                    # evergreen-installer：ランタイム未導入の環境で同梱
                                           #   インストーラーを実行（既定はオン——ウィザードは
                                           #   先に同意を尋ね、ヘッドレスはサイレント実行）
# download-url = "https://…"               # evergreen-installer：同梱しない場合、インストール時に
                                           #   Evergreen インストーラーを取得（同梱分と同じ同意フロー）
# warn-missing = true                      # skip / evergreen-installer：egui 代替画面が開始時と
                                           #   終了時に警告し Microsoft 公式ダウンロードページ
                                           #   へのリンクを表示（既定はオン）

[[package.metadata.shun.steps]]            # 順序付きウィザードパイプライン（任意）
columns = 2              # 任意：モードグリッドの列数（既定はモードごとに 1 列）
kind = "mode"                              # mode | scope | license | content | pairing | install
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

## フィールド

| フィールド | 型 | 既定 | 意味 |
| --- | --- | --- | --- |
| `product` | string | パッケージ `name` | タイトル、ARP 表示名、フラッシュラベル |
| `publisher` | string | — | ARP Publisher フィールド |
| `logo` | path | — | シェルのロゴアセット（マニフェスト相対） |
| `payload` | path | — | 成果物にパックされるディレクトリ |
| `main-exe` | path | — | payload 相対のエントリポイント（ショートカットの宛先） |
| `install` | table | 両モードオン | `local` / `portable` スイッチ、`portable-marker` ファイル名、`desktop-shortcut` / `start-menu-shortcut` / `launch-after-install` ポリシー、`default-dir`（環境変数展開される既定位置の上書き）、`verbs`、`deep-links`、`aumid`、`icon`、`root-dir-folder`（ドライブルート直下に挟むフォルダー、既定は製品名） |
| `attachments` | テーブル配列 | なし | 追加リソース（アセットパック）：`key` / `title` / `dest` / `online.url`。ライト版はインストール時にダウンロード |
| `update` | table | なし | 更新ウォッチ：`sources`（ミラー基底 URL、順にプローブ）+ 最初に到達したソース配下で解決される `files` |
| `webview2` | table | `skip` | Windows ランタイム戦略 |
| `steps` | テーブル配列 | 既定パイプライン | 順序付きウィザードパイプライン（`mode`/`scope`/`license`/`content`/`pairing`/`install`、自由順序。`install` はちょうど 1 つ） |
| `script` | table | なし | payload スクリプトフック：`runner` + `hooks[]`（`phase`/`script`）——スクリプト設計ノート参照 |
| `variant` | テーブル配列 | なし | `shun build --variant` で選択される名前付きビルド構成 |
| `signing` | table | 未署名 | 成果物に適用する Authenticode / codesign プロファイル |
| `license-sysl` | table | なし | SySL ライセンスと公式翻訳をリポジトリからバンドル |
| `msix.logo-background` | color | `transparent` | 透過 MSIX ロゴの下に敷くプレート色 |
| `flash` | table | — | flash ターゲットの宣言 |

## ショートカットポリシー

`install.desktop-shortcut` と `install.start-menu-shortcut` はレーン
ごとに解決されます。ウィザードの各フェイスはチェックボックスを表示します
（既定でオン —— NSIS の慣行）。ヘッドレスレーン（`--silent`）は `ask` を
**存在**で解決します：既に存在するショートカットはその場で更新され、
存在しないものは新たに作られません。サイレント更新が、ユーザーが求めて
いないアイコンをまき散らすことは決してあってはなりません —— AV/EDR の
ヒューリスティクスが指摘するのはまさにその挙動ですし、ユーザーが削除した
ショートカットは削除されたままでなければなりません。`always` / `never`
はすべてのレーンで回答を固定します。明示的な `--shortcut-desktop` /
`--shortcut-menu` フラグ（アップデーターの契約）はすべてに優先します。
アンインストールはショートカットを無条件に除去します。

## インストール後の起動

`install.launch-after-install` は完了ページの慣例を解決します：`ask`
（既定）はウィザードのチェックボックスに従います——NSIS の
「インストール後に起動」トグルで、ヘッドレス実行ではチェック付き
として扱われます。`always` / `never` はそのトグルを持たないシェル
向けに固定します。

インストールフロー自体は何も起動しません。実行が成功した後に
シェルが `shun::targets::install::launch` を呼びます：インストール
ディレクトリ内の設定された `main-exe` をデタッチで起動します
（待機しないため、インストーラープロセスは直後に終了できます）。
解決された `InstallContext::launch_after_install` が false の場合、
またはエントリポイントが未宣言の場合は何もしません。宣言済みの
エントリポイントが見つからない場合は通常どおり `MissingEntry`
エラーになります。

## ドライブルートの保護

ドライブなど、裸のファイルシステムルート（`D:\` のほか `D:`、
UNC 共有ルート `\\server\share`、POSIX の `/` も同様）が対象の
場合、ペイロードが直接そこへ置かれることはありません：
`InstallContext::apply_config` がその下に一段フォルダーを挟みます。
既定は製品名で、`install.root-dir-folder` でカスタマイズできます。
ウィザードはルートが選択・入力された時点でパス欄を書き換えるため、
表示される宛先は常に実際のものです。ヘッドレスの `--dir=D:\` 実行
もフロー内で同じ保護を受けます。

## WebView2 戦略

| `type` | 同梱物 | 必要条件 | 備考 |
| --- | --- | --- | --- |
| `skip` | なし | システムの WebView2 | 標準アーティファクト |
| `evergreen-installer` | Evergreen オフラインインストーラー（約 127 MB） | インストール時の昇格 | システム全体のランタイムを登録 |
| `fixed-version` | 展開済みランタイムフォルダー | なし | シェルとインストール済みアプリがインストール／ポータブル両モードで共有する私有コピー |

実測（2026-09、v151 x64）：fixed-version ランタイムは**圧縮で
約 293 MB / 展開で約 661 MB**——許容済みのコストで、他のパッカーと
同水準です。アーティファクトが埋め込むコピーは**ちょうど 1 つ**：
シェルは payload のランタイムサブツリーを
`%LOCALAPPDATA%\shun\<product>\webview2` へステージングし、
`WEBVIEW2_BROWSER_EXECUTABLE_FOLDER` をそこへ向けて自己ブート
ストラップします。展開はハッシュを把握しており、再インストールや
先行ステージ済みファイルは書き換えずに採用されます
（"Reusing" 進捗イベント）——同じエンジンの二つ目のコピーは
生まれません。

### ランタイム未導入マシンの劣化

面のラダーはランタイム欠落で死なず、劣化するだけです。ラダー解決の
前、シェルはローダー自身の判定（`wry::webview_version()`、tauri が
webview 生成前に課すのと同じゲート）でランタイムを probe するため、
壊れたレジストリ残骸が webview 面を tauri の英語
"Could not find the WebView2 Runtime" ボックスへ送り込むことは
もうありません：

- `evergreen-installer` + `silent-install`（既定 `true`）：ランタイム
  の取得はインストールステップにあります——初期化は同梱
  インストーラー（payload の `webview2/` プレフィックスで発見）を
  ユーザーごとのキャッシュへデコードしてステージするだけです。ラダー
  解決前に何もインストールしません。ウィザード実行はまず同意確認で
  停止し（「ランタイムをインストール / スキップして本体のみ」）、
  許可された場合にのみ取得を実行します——インストーラーは自身の
  ウィンドウで可視に走り、マニフェストが要求すれば UAC を経由します。
  ヘッドレストラックは `silent-install` を同意として `/silent
  /install` で実行します。取得に成功するとマシンはその場で
  アップグレードされ、ローダーが利用可能なランタイムを報告します。
- インストーラーを同梱しないが `download-url` を宣言するビルドは、
  同じインストールステップの取得の中で、同じ同意の後に Evergreen
  インストーラーをその URL から取得します（ヘッドレスは
  `silent-install` ノブのまま）——lite フレーバーは自前の payload
  ではなくネットワークからマシンをアップグレードします。低速・
  切断ネットワークが最初の UI をブロックすることは決してありません：
  取得はインストールフロー内で行われ、劣化バナーはランタイムが
  来ると伝えます。
- 結果如何にかかわらず、ランタイムのないマシンは egui 面に落ちます。
  `warn-missing`（既定 `true`）でインストーラーを同梱しない場合、
  その面はウィザードの冒頭と末尾に一度ずつ警告し、Microsoft 公式の
  WebView2 ダウンロードページへのリンクを埋め込みます。両ノブは
  製品単位でオフにできます：

```toml
[package.metadata.shun.webview2]
type = "evergreen-installer"
silent-install = false                  # 同梱インストーラーを自動実行しない
warn-missing = false                    # 警告バナーなしの静かな劣化
```

知っておくべき上流の制限：tauri のランタイムゲートは
`WEBVIEW2_BROWSER_EXECUTABLE_FOLDER` を読まないため、
fixed-version のみ（Evergreen 未導入）のマシンは、webview 生成が
可能だったとしても egui 面に落ちます——シェルは必ず描画できる面を
優先します。

## MSIX ロゴプレート

Windows は `BackgroundColor="transparent"` を無視するあらゆる表面で、
パッケージデスクトップアプリのロゴを既定のシステムブルー
（`#0078D7`）の上に敷きます——App Installer ダイアログもその一つ
です。`msix.logo-background` は `#RRGGBB` 色を受け付けます：shun CLI
は透過ロゴをその色に合成してパッケージアセットとし、
`BackgroundColor` として宣言します。ブランドがすべての表面の
プレート色を握れます：

```toml
[package.metadata.shun.msix]
# identity-name / publisher / display-name / executable ...
logo-background = "#0F172A"
```

## オフラインシェル

インストーラーシェルは hikari webview UI に加えて egui フォール
バック UI を埋め込みます。同じマニフェストと payload で駆動され
（1 つのフロー、3 つのレンダラー）、エフェクトは描画せず、Windows
で WebView2 が欠けているとき（バナーは欠けた環境を明示します）、
または手動強制時に選択されます：

```bash
shun-installer --no-webview   # 同じウィザード、オフラインレンダラー（--fallback は旧スペル）
```

両 UI がオフラインスクリーンショットに対応します——ウィンドウ内容を
`PrintWindow` で取り込み、デスクトップ自動化なしで保存後、プロセスは
終了します：

```bash
shun-installer --screenshot=ui.png                 # hikari（webview）UI
shun-installer --no-webview --screenshot=ui.png    # egui オフライン UI
```

## シェル UI

`[package.metadata.shun.shell]`（スタンドアロンドキュメントでは `shell` キー）
がランタイムシェルを設定します：

```toml
[shell]
timeline = "left"          # top（上部の横並び）| left（左側の縦並び）
log-level = "all"          # all（既定）| files | scripts | off
log-order = "newest"      # newest（既定、最新が上）| oldest（末尾に追加）
language = "auto"          # auto | en | zh-Hans | zh-Hant | ja | ko | fr | ru | es | de | pt
faces = ["webview", "egui", "tui"]  # この製品のシェルが提示できる画面（既定は 3 つすべて）

[shell.theme]
mode = "system"            # system | light | dark
accent = [34, 211, 238]    # RGB チャンネル — --color-primary を上書き
```

`shell.faces` はこの製品のシェルが提示できる画面——`webview` /
`egui` / `tui`——を限定します。省略時は 3 つすべて。ヘッドレス
CLI 面（`--silent`、ARP のアンインストール文字列）は基盤であり
無効化できません。無効化された画面の要求はエラー、自動選択は
無効化された画面をスキップします。

### ウィザードの言語

ウィザードは**最初のステップ**で言語を尋ねます。10 言語のロケールを
それぞれ自国語のラベル（English、简体中文、日本語、…）で並べた
セレクターです。切り替えると UI 文字列とライセンス文書が即座に
再描画され（契約文書は `license-locales` / `locale-paths` に従って
選択ロケールのものに切り替わります）、選択はユーザーごとの設定
ファイルに記録されて次回も引き継がれます
（`<ローカルアプリデータ>/<製品>/installer-prefs.json`、例えば
`%LOCALAPPDATA%\ShunDemo\installer-prefs.json` に
`{ "language": "zh-Hans" }`）。ポータブル実行では選択はメモリ上だけ
に保たれます。ポータブルコピーはシステムに一切書き込みません。
`shell.language` は設定による固定値のままですが、記録された選択が
それより優先され、次いでシステムのロケールが使われます。

選択された言語はインストールフローにも渡ります。payload の
スクリプトには `SHUN_LANGUAGE` 環境変数として渡され、ディスク上の
インストールマニフェストにも記録されます（修復・更新の各パスから
参照できます）。インストールされたアプリケーション自身の設定に何を
書き込むかは payload のスクリプト次第です — shun が輸出するのは
事実だけです。

## ペアリングステップ

`pairing` ステップ種はこのマシンのために資格情報を要求します——
逆方向レーン：このマシンがコードを表示し、オペレーターが管理
パネルに入力します。1 つのペイン、2 つのレーン：

```toml
[[package.metadata.shun.steps]]
kind = "pairing"

[package.metadata.shun.steps.pairing.source]
kind = "gateway"                     # gateway | scripts
official = "https://gateway.example.test"   # 事前選択されるゲートウェイ基底 URL
# allow-custom = true                # カスタムゲートウェイ入力も提供（既定オン）

[package.metadata.shun.steps.pairing.identity]
node-id = true                       # ゲートウェイプロトコルの主キー
name = false                         # 任意のデバイス名（ホスト名で事前入力）
# tier = 1                           # 要求時に付与されるティア（製品が固定する場合）

# env-file = "pairing.env"           # gateway レーン：要求された資格情報の
                                     #   インストールディレクトリ内の書き込み先（.env 形式）
```

- `gateway` —— 内蔵 JSON-RPC クライアント（`pairing.request` が
  コードを発行、`pairing.await` がロングポール、`pairing.accept` が
  資格情報を配達）。対象マシンにスクリプトランタイムは不要です。
  このレーンには `node-id` と `env-file` が必要です。
- `scripts` —— 製品独自のアクティベーション：`request` / `await` /
  `record` の payload 相対 Python スクリプト（`python3`、バージョン
  なし名のみのホストでは `python` にフォールバック）。ペインの回答は
  スクリプトの **stdin** に 1 つの JSON オブジェクトとして渡されます
  ——argv は決して使いません。各フェーズの stdout は正確に 1 つの
  JSON 値である必要があります。`record` が要求結果を永続化します
  （`env-file` は運ばれますが消費されません）。各フェーズには
  デッドラインがあり、フリーズしたスクリプトは待たされず終了され
  ます。

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

## 更新ウォッチ

`[package.metadata.shun.update]`（スタンドアロンドキュメントでは
`update` キー）は、シェルが更新を probe するミラーソースと、その
配下で解決されるファイルを宣言します：

```toml
[update]
sources = [                                # ミラー基底 URL、順に試行
  "https://mirror.example.test/shun-demo",
  "https://releases.example.test/shun-demo",
]
files = ["latest", "app-setup.exe"]
```

実行時、シェルは `shun::update::resolve` を呼びます：各ソースに
短い probe（最初のファイルへの素朴な GET、10 秒タイムアウト）を
行い、最初に `200` を返したソースがその回の全体で勝ち、宣言された
各ファイルは勝ったソース配下の URL（`<source>/<file>`）に解決され
ます。到達不能・非 200 のソースは警告としてスキップされ、どこも
応答しなければ何も解決されません。シェルはその後、解決された URL
を自分で取得します——`latest` バージョンマーカーはテキストとして
（`fetch_text`）、成果物のダウンロードはオンライン payload パイプ
ライン（`OnlinePayload`：ダウンロード → 展開 → 検証）経由です。

## ビルドバリアント

`[[package.metadata.shun.variant]]`（単数形——テーブル配列の慣用）
は `shun build --variant <name>` フラグで選択される名前付き成果物
構成を宣言します——インストーラーマトリクスの軸です（アーキテク
チャと OS は `--target`、リソースパックは payload ディレクトリに
乗ります）：

```toml
[[package.metadata.shun.variant]]
name = "lite"                        # id + 成果物名のサフィックス
payload = "payload-lite"             # payload ディレクトリの上書き
faces = ["egui", "tui"]              # このバリアントのシェルの面上書き

[package.metadata.shun.variant.env]  # シェルの cargo ビルドに渡る
SHUN_FLAVOR = "lite"                 # （と build.rs）——フレーバーマーカー
```

バリアントの適用は payload ディレクトリとシェルの面リストを差し
替えます。`env` はシェルビルドに渡り、フレーバーがマーカーを埋め
込めます。不明な名前はエラーです。

## コード署名

`[package.metadata.shun.signing]` は成果物の署名を設定します——
`shun build` は実行の最後にインストーラーへ署名し、`shun sign` は
単独のファイルへ署名します（どちらも `--no-sign` でスキップ）。
未宣言＝未署名の成果物で、ローカルテストには十分です：

```toml
[package.metadata.shun.signing.windows]   # signtool による Authenticode
enabled = true
thumbprint = "<証明書の拇印>"              # ユーザー／マシンストアから選択（/sha1）
# timestamp-url = "http://timestamp.digicert.com"   # RFC 3161（/tr、既定値）

[package.metadata.shun.signing.macos]     # codesign
enabled = true
identity = "<署名アイデンティティ>"
```

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

UI には 10 ロケール（`en`、`zh-Hans`、`zh-Hant`、`ja`、`ko`、`fr`、`ru`、`es`、
`de`、`pt`）の既定テキストが同梱されています。`shell.language = "auto"` はシステムに追従し、
固定ロケールを指定すればそちらに固定されます。ロケール別ライセンスの上書きで
ローカライズされた契約書も機能します。

## SySL リポジトリからのライセンス

`[package.metadata.shun.license-sysl]` はビルド時に、ライセンス自身の
リポジトリから Synthetic Source License と公式翻訳をバンドルします
（`online` feature が必要）——ライセンスファイルをリポジトリに
vendor せずに、ウィザードが利用者の言語で合意を示せます：

```toml
[package.metadata.shun.license-sysl]
repo = "celestia-island/sysl"        # 既定
branch = "main"                      # 既定
locales = ["zh-Hans", "ja"]          # バンドルする翻訳（ルートの英語は常にバンドル）
```

ロケールタグは sysl リポジトリの i18n ディレクトリへ対応します
（`zh-Hans` → `zhs`、`zh-Hant` → `zht`。`ja`/`ko`/`fr`/`ru`/`es`/
`de`/`pt`/`ar` は直接）。バンドルされた文書は他のライセンス文書と
同じくライセンスステップで描画されます。

## スクリプトフック

`[package.metadata.shun.script]` は payload 相対のランナースクリプト
を宣言し、配布フローがフェーズ境界で実行します。製品ごとに 1 つの
ランナー——現在マウントされているのは `duckscript`（組み込み
インタープリター）だけです：

```toml
[package.metadata.shun.script]
runner = "duckscript"

[[package.metadata.shun.script.hooks]]
phase = "post-install"               # prepare | post-install | pre-uninstall
script = "scripts/post_install.ds"   # payload 相対。payload と共にパック
```

失敗したフックはフローを失敗させます。完全な契約——フェーズ、
フックから見える環境（`SHUN_LANGUAGE` を含む）、例——は
[スクリプト設計ノート](../design/scripting.md)にあります。
