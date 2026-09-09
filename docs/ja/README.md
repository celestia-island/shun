<p align="center"><img src="../logo.webp" alt="Shun" width="240" /></p>

<h1 align="center">Shun</h1>

<p align="center"><strong>フロー駆動のペイロード配布ランタイム——インストーラー、フラッシャー、ポータブルモード</strong></p>

<div align="center">

[![License: SySL-1.0](https://img.shields.io/badge/License-SySL--1.0-blue.svg)](https://sysl.celestia.world)
[![Crates.io](https://img.shields.io/crates/v/shun)](https://crates.io/crates/shun)
[![docs.rs](https://docs.rs/shun/badge.svg)](https://docs.rs/shun)
[![Checks](https://img.shields.io/github/actions/workflow/status/celestia-island/shun/checks.yml)](https://github.com/celestia-island/shun/actions/workflows/checks.yml)

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

shun はデスクトップソフトウェア配布の**配布**側半分をパッケージします。アプリ自身の
`Cargo.toml`（`[package.metadata.shun]`、cargo-deb / cargo-wix パターン）という一つの設定が、
ビルド CLI とランタイムシェルの両方を駆動します：

- **ペイロード**は一度だけパッケージ化され、単一ファイルインストーラーに埋め込むかサイドカーとして携帯；
- **フロー**——モードを選び、ターゲットを選び、実際の進捗をストリーム；
- プラグイン式 **targets**：
  - `install` —— プラットフォームごとの NSIS 型登録：Windows ARP エントリー、ショートカット（AUMID 付き）、Explorer コンテキストメニュー動詞、ディープリンク、ユーザー単位またはマシン全体（自己昇格）；Linux は `.desktop` ランチャー + デスクトップアクション；macOS は `.app` 補完 + Launch Services——そしてどのプラットフォームでもシステム状態を書かないポータブルモード；
  - `flash` —— イメージをブロックデバイスへ書き込み、書き込み後検証。

ウィザード自体は**宣言型パイプライン**（`mode | scope | license | content | install`、自由順序）で、インストール画面にはフェーズ加重の実際の進捗バーと折りたたみ式ターミナルがあり、ファイル操作を一行ずつ記録——冗長度は `shell.log-level` で設定。

Windows ではシェルが二つの顔を持ちます：hikari WebView UI と、WebView2 を一切必要としない組み込みの **egui フォールバック**——同じフロー、同じマニフェスト（`--fallback` で強制）。固定バージョンの WebView2 ランタイムはペイロード内に同梱でき、シェルとインストール済みアプリが同一コピーを共有。

## 例

一つの demo が配布をエンドツーエンドでカバー——実際の Tauri 2 ペイロード（`demo-app/`）、[@celestia-island/hikari](https://github.com/celestia-island/hikari) 製インストーラーシェル（`shell/`）、一つのマニフェスト：

```bash
just demo                                        # # ステージ → ビルド → インストーラーシェル実行
just demo -- --fallback                          # # オフライン egui シェルを強制
cargo run --example demo_install                 # # .shun パッケージ生成 + ローカルインストール
cargo run --example demo_install -- --portable   # # ポータブルインストール（システム状態ゼロ）
cargo run --example demo_flash                   # # フラッシュ候補デバイスを列挙
```

全フィールド参照：[設定ガイド](./guides/configuration.md)
（[English](../en/guides/configuration.md)）.

## ステータス

現在のリリース：**0.2.0**。crate は celestia エコシステムの 3 つの実消費者——WoWSP インストーラーシェル、shittim-chest ローカル、evernight イメージフラッシャー——に合わせて安定化中。API はマイナーバージョン間でこの 3 消費者に追従——統合フィードバックによる追加変更を想定。

## 構成

| パス | 役割 |
| --- | --- |
| `src/config.rs` | 設定スキーマ + `[package.metadata.shun]` ローダー |
| `src/flow.rs` | フローモデル——シェルが描画する進捗・ログイベント |
| `src/payload.rs` | ペイロードのパック / マニフェスト / ストリーム展開 |
| `src/targets/` | インストール target（Windows/Linux/macOS 登録）とフラッシュ target |
| `demo-app/` | ShunDemo——Tauri 2 ペイロードアプリ（サンプル UI、配布マニフェスト） |
| `shell/` | インストーラーシェル：hikari UI（Tauri）+ egui オフラインフォールバック |
| `docs/` | 言語別ガイドと設計ノート |

## 開発

```bash
just fetch   # # 共有 celestia-devtools レシピを取得（一度だけ）
just ci      # # fmt-check + clippy + test
```

作業は `feat/*` / `fix/*` ブランチから squash マージされた PR で `master` に届きます。規約の全文は [AGENTS.md](../../AGENTS.md)。

## ライセンス

SySL-1.0 —— [LICENSE](../../LICENSE) を参照。
