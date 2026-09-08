<p align="center"><img src="../logo.webp" alt="Shun" width="240" /></p>

<h1 align="center">Shun</h1>

<p align="center"><strong>フロー駆動ペイロード配送ランタイム — インストーラー、フラッシャー、ポータブルモード</strong></p>

<div align="center">

[![License: SySL-1.0](https://img.shields.io/badge/License-SySL--1.0-blue.svg)](https://sysl.celestia.world)
[![GitHub](https://img.shields.io/badge/github-celestia--island%2Fshun-blue.svg)](https://github.com/celestia-island/shun)
[![Checks](https://img.shields.io/github/actions/workflow/status/celestia-island/shun/checks.yml)](https://github.com/celestia-island/shun/actions/workflows/checks.yml)
[![docs.rs](https://docs.rs/shun/badge.svg)](https://docs.rs/shun)

</div>

<div align="center">

[English](../README.md) ·
[简体中文](../zh-Hans/README.md) ·
[繁體中文](../zh-Hant/README.md) ·
**日本語** ·
[한국어](../ko/README.md) ·
[Français](../fr/README.md) ·
[Русский](../ru/README.md) ·
[Español](../es/README.md)

</div>

---

shun はデスクトップソフトウェア公開の「配送」側を担います。1 つの設定ドキュメントが
ビルド CLI とランタイムシェルの両方を駆動します：

- **payload** — アプリディレクトリを一度パックし、シングルファイルインストーラーに
  埋め込むかサイドカーとして携帯；
- **flow** — モードを選択、ターゲットを選択、リアルな進捗イベントをストリーミング；
- プラグイン可能な **targets**：
  - `install` — NSIS 風登録（ユーザー単位 ARP エントリー、自己コピー uninstaller、
    スタートメニューショートカット、ディープリンク）*および* レジストリ非接触の
    ポータブルモード；
  - `flash` — イメージをブロックデバイスに書き込み、検証を実施。

Windows では、デュアルバリアント WebView2 戦略がクリーンマシンをカバーします：
標準アーティファクトはシステムランタイムを要求し、完全自己完結アーティファクトは
**固定バージョン WebView2 ランタイムを私的に携帯** — 1 部のコピーをシェルと
インストール済みアプリで共有し、インストールとポータブルの両モードにわたって機能、
管理者権限不要、システム書き込みゼロ。

## 例

包括的なデモが配信をエンドツーエンドで網羅します。payload は実際の
Tauri 2 アプリ（`demo-app/`、サンプル UI 付き）で、インストーラーシェル
（`shell/`、[@celestia-island/hikari](https://github.com/celestia-island/hikari)
製）がビルド時に組み込み、全体が単一の配信マニフェストで宣言されます：

```bash
just demo                                               # デモアプリを stage → ビルド → シェル実行
just demo -- --fallback                                 # オフライン egui シェルを強制（WebView2 不要）
cargo run --example demo_flash                        # フラッシュ候補デバイスの列挙
cargo run --example demo_install                      # ShunDemo.shun 生成 + ローカルインストール
cargo run --example demo_install -- --portable        # ポータブルインストール（レジストリ不使用）
cargo run --example demo_install -- --uninstall       # アンインストール（痕跡を完全除去）
```

`demo_install` はインストーラーパッケージ `ShunDemo.shun`（zstd tar + SHA-256
マニフェスト）をワーキングディレクトリに生成し、ストリーミング進捗で展開し、
ローカルモードでは NSIS 風登録を実行します。Tauri デモシェル（`shell/`、
[@celestia-island/hikari](https://github.com/celestia-island/hikari) ベース）が
同じフローをフル UI で描画し、ビルド時に payload を埋め込みます。

配布マニフェスト自体はデモ crate にあります：

```toml
[package.metadata.shun]
product = "ShunDemo"
publisher = "celestia-island"
payload = "../examples/demo_payload"
main-exe = "bin/shun-demo.exe"

[package.metadata.shun.install]
local = true
portable = true
```

完全なフィールドリファレンスは
payload ルートには小さなコミット済みデータのみ置かれ、アプリのバイナリは
`just demo-payload` が `bin/` へ stage するビルド産物です（コミットされません）。

[docs/en/guides/configuration.md](../en/guides/configuration.md)
（[日本語ガイドは準備中][]) を参照してください。WebView2 戦略マトリクスを含みます。

## ステータス

プレリリース。celestia エコシステムの 3 つの実消費者 — WoWSP インストーラー
シェル、shittim-chest ローカル、evernight イメージフラッシャー — に対して
crate を安定化中。活発な開発は `dev` ブランチで行われ、最初の配布フロー完了後に
`master` が初期リリースコミットを受け取ります。`0.1` まで API は不安定です。

## 構成

| パス | 役割 |
| --- | --- |
| `src/config.rs` | 設定スキーマ + `[package.metadata.shun]` ローダー |
| `src/flow.rs` | フローモデル — シェルが描画する進捗イベント |
| `src/payload.rs` | payload パック / マニフェスト / ストリーミング展開 |
| `src/targets/install.rs` | インストールターゲット：登録バックエンド、ポータブルモード |
| `src/targets/flash.rs` | フラッシュターゲット：ブロックデバイス書き込み + 検証 |
| `shell/` | インストーラーシェル：hikari UI（Tauri）+ egui オフラインフォールバック |
| `docs/` | ロケール別ガイドと設計ノート |

## 開発

```bash
just fetch   # 共有 celestia-devtools レシピをステージ（1 回）
just ci      # fmt-check + clippy + test
```

ワークフロー：迅速な準備は `dev` ブランチで行い、`master` が初期リリース
コミットを受け取った後はすべて PR 経由でランドします。

## ライセンス

SySL-1.0 — [LICENSE](../LICENSE) を参照。
