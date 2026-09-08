<p align="center"><img src="../logo.webp" alt="Shun" width="240" /></p>

<h1 align="center">Shun</h1>

<p align="center"><strong>流驅動的 payload 交付運行時——安裝器、燒寫器與可攜模式</strong></p>

<div align="center">

[![License: SySL-1.0](https://img.shields.io/badge/License-SySL--1.0-blue.svg)](https://sysl.celestia.world)
[![GitHub](https://img.shields.io/badge/github-celestia--island%2Fshun-blue.svg)](https://github.com/celestia-island/shun)
[![Checks](https://img.shields.io/github/actions/workflow/status/celestia-island/shun/checks.yml)](https://github.com/celestia-island/shun/actions/workflows/checks.yml)
[![docs.rs](https://docs.rs/shun/badge.svg)](https://docs.rs/shun)

</div>

<div align="center">

[English](../README.md) ·
[简体中文](../zh-Hans/README.md) ·
**繁體中文**

</div>

---

shun 包含桌面軟體發布的**交付**半邊。一份設定檔同時驅動建置 CLI 和執行時殼：

- **payload**——應用程式目錄打包一次，內嵌為單檔安裝器或作為 sidecar 攜帶；
- **flow**——選擇模式、選擇目標、串流真實進度事件；
- 可插拔 **targets**：
  - `install` —— NSIS 式註冊（使用者級 ARP 條目、自複製解除安裝器、開始功能表捷徑、深層連結）*以及* 零登錄檔的可攜模式；
  - `flash` —— 映像寫入區塊裝置並做寫後校驗。

在 Windows 上，雙變體 WebView2 策略覆蓋乾淨機器：標準發行物要求系統執行時，完整自包含發行物則**私有攜帶固定版本 WebView2 執行時**——一份副本由安裝器殼與已裝應用共享，橫跨安裝與可攜模式，免管理員、零系統寫入。

## 範例

一個完整的 demo 端到端覆蓋交付全流程。payload 是真實的 Tauri 2 應用
（`demo-app/`，帶示例介面），安裝器殼（`shell/`，基於
[@celestia-island/hikari](https://github.com/celestia-island/hikari)）
建置時嵌入它，全部由同一份交付清單宣告：

```bash
just demo                                               # 暫存 demo 應用 → 建置 → 執行安裝器殼
just demo -- --fallback                                 # 強制離線 egui 殼（無 WebView2）
cargo run --example demo_flash                        # 列舉可燒錄裝置
cargo run --example demo_install                      # 產生 ShunDemo.shun + 本機安裝
cargo run --example demo_install -- --portable        # 可攜安裝（不寫登錄檔）
cargo run --example demo_install -- --uninstall       # 解除安裝（清除全部痕跡）
```

`demo_install` 產生安裝套件 `ShunDemo.shun`（zstd tar + SHA-256 清單）在工作目錄中，
以串流進度解壓，並在本機模式下執行 NSIS 式註冊。Tauri demo 殼
（`shell/`，基於 [@celestia-island/hikari](https://github.com/celestia-island/hikari)）
用完整 UI 渲染同一流程，建置時嵌入 payload。

交付清單位於 demo 應用自己的 `Cargo.toml`
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

完整欄位參考見
payload 根目錄保留少量已入庫資料檔；應用二進位是建置產物，由
`just demo-payload` 暫存進 `bin/`（永不入庫）。

[docs/en/guides/configuration.md](../en/guides/configuration.md)
（[繁體中文](./guides/configuration.md)），包含 WebView2 策略矩陣。

## 狀態

預發布；crate 正在面向 celestia 生態的三個真實消費者打磨——WoWSP 安裝器殼、
shittim-chest 本地版和 evernight 映像燒寫器。活躍開發在 `dev` 分支；首個交付
流完成後 `master` 將接收初始發布提交。`0.1` 之前 API 不穩定。

## 結構

| 路徑 | 角色 |
| --- | --- |
| `src/config.rs` | 設定 schema + `[package.metadata.shun]` 載入器 |
| `src/flow.rs` | 流模型——殼渲染的進度事件 |
| `src/payload.rs` | payload 打包 / 清單 / 串流解壓 |
| `src/targets/install.rs` | 安裝 target：註冊後端、可攜模式 |
| `src/targets/flash.rs` | 燒寫 target：區塊裝置寫入 + 校驗 |
| `shell/` | 安裝器殼：hikari UI（Tauri）+ egui 離線降級 |
| `docs/` | 分語言指南與設計筆記 |

## 開發

```bash
just fetch   # 拉取共享 celestia-devtools recipes（一次）
just ci      # fmt-check + clippy + test
```

工作流：快速準備在 `dev` 分支；`master` 接收初始發布提交，之後全部透過 PR 合入。

## 授權條款

SySL-1.0 —— 見 [LICENSE](../LICENSE)。
