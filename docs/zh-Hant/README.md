<p align="center"><img src="../logo.webp" alt="Shun" width="240" /></p>

<h1 align="center">Shun</h1>

<p align="center"><strong>流驅動的載荷交付執行時——安裝器、燒寫器與便攜模式</strong></p>

<div align="center">

[![License: SySL-1.0](https://img.shields.io/badge/License-SySL--1.0-blue.svg)](https://sysl.celestia.world)
[![Crates.io](https://img.shields.io/crates/v/shun)](https://crates.io/crates/shun)
[![docs.rs](https://docs.rs/shun/badge.svg)](https://docs.rs/shun)
[![Checks](https://img.shields.io/github/actions/workflow/status/celestia-island/shun/checks.yml)](https://github.com/celestia-island/shun/actions/workflows/checks.yml)

</div>

<div align="center">

[English](../README.md) ·
[简体中文](../zh-Hans/README.md) ·
**繁體中文** ·
[日本語](../ja/README.md) ·
[한국어](../ko/README.md) ·
[Français](../fr/README.md) ·
[Русский](../ru/README.md) ·
[Español](../es/README.md)

</div>

---

shun 打包桌面軟體發布的**交付**半邊。一份配置——應用自己的
`Cargo.toml`（`[package.metadata.shun]`，cargo-deb / cargo-wix 模式）——
同時驅動構建 CLI 與執行時殼：

- **載荷**打包一次，內嵌為單檔案安裝器或作為 sidecar 攜帶；
- **流**——選模式、選目標、流出真實進度；
- 可插拔 **targets**：
  - `install` —— 各平台 NSIS 式註冊：Windows ARP 條目、捷徑（帶 AUMID）、Explorer 右鍵動詞、深連結，使用者級或機器級（自提權）；Linux `.desktop` 啟動器 + 桌面動作；macOS `.app` 補全 + Launch Services——以及任何平台都不寫系統狀態的便攜模式；
  - `flash` —— 映像寫入區塊裝置並做寫後校驗。

向導本身是**宣告式管線**（`mode | scope | license | content | install`，自由排序），安裝頁顯示真實的按階段加權進度條與可摺疊終端機，逐行記錄每個檔案操作——詳細度由 `shell.log-level` 配置。

Windows 上殼有兩副面孔：hikari WebView UI 與內嵌的**egui 降級介面**（完全不需要 WebView2）——同一流程、同一清單（`--fallback` 強制）。固定版本 WebView2 執行時可隨載荷攜帶，安裝殼與已裝應用共享一份副本。

## 範例

一個 demo 端到端覆蓋交付——真實 Tauri 2 載荷（`demo-app/`）、基於 [@celestia-island/hikari](https://github.com/celestia-island/hikari) 的安裝殼（`shell/`）、一份清單：

```bash
just demo                                        # # 暫存 → 构建 → 執行安裝殼
just demo -- --fallback                          # # 強制離線 egui 殼
cargo run --example demo_install                 # # 生成 .shun 包 + 本機安裝
cargo run --example demo_install -- --portable   # # 便攜安裝（零系統狀態）
cargo run --example demo_flash                   # # 列舉可燒寫裝置
```

完整欄位參考：[配置指南](./guides/configuration.md)
（[English](../en/guides/configuration.md)）.

## 狀態

當前發布：**0.2.0**。crate 正面向 celestia 生態的三個真實消費者打磨——WoWSP 安裝器殼、shittim-chest 本地版、evernight 映像燒寫器。API 在次要版本間跟隨這三個消費者——預期來自其整合回饋的增量變更。

## 結構

| 路徑 | 角色 |
| --- | --- |
| `src/config.rs` | 配置 schema + `[package.metadata.shun]` 載入器 |
| `src/flow.rs` | 流模型——殼渲染的進度與日誌事件 |
| `src/payload.rs` | 載荷打包 / 清單 / 串流解壓 |
| `src/targets/` | 安裝 target（Windows/Linux/macOS 註冊）與燒寫 target |
| `demo-app/` | ShunDemo——Tauri 2 載荷應用（範例 UI、交付清單） |
| `shell/` | 安裝殼：hikari UI（Tauri）+ egui 離線降級 |
| `docs/` | 分語言指南與設計筆記 |

## 開發

```bash
just fetch   # # 拉取共享 celestia-devtools recipes（一次）
just ci      # # fmt-check + clippy + test
```

工作經 `feat/*` / `fix/*` 分支以 squash 合併的 PR 落到 `master`。完整慣例見 [AGENTS.md](../../AGENTS.md)。

## 授權條款

SySL-1.0 —— 見 [LICENSE](../../LICENSE)。
