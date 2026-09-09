<p align="center"><img src="../logo.webp" alt="Shun" width="240" /></p>

<h1 align="center">Shun</h1>

<p align="center"><strong>流驱动的载荷交付运行时——安装器、烧写器与便携模式</strong></p>

<div align="center">

[![License: SySL-1.0](https://img.shields.io/badge/License-SySL--1.0-blue.svg)](https://sysl.celestia.world)
[![Crates.io](https://img.shields.io/crates/v/shun)](https://crates.io/crates/shun)
[![docs.rs](https://docs.rs/shun/badge.svg)](https://docs.rs/shun)
[![Checks](https://img.shields.io/github/actions/workflow/status/celestia-island/shun/checks.yml)](https://github.com/celestia-island/shun/actions/workflows/checks.yml)

</div>

<div align="center">

[English](../README.md) ·
**简体中文** ·
[繁體中文](../zh-Hant/README.md) ·
[日本語](../ja/README.md) ·
[한국어](../ko/README.md) ·
[Français](../fr/README.md) ·
[Русский](../ru/README.md) ·
[Español](../es/README.md)

</div>

---

shun 打包桌面软件发布的**交付**半边。一份配置——应用自己的
`Cargo.toml`（`[package.metadata.shun]`，cargo-deb / cargo-wix 模式）——
同时驱动构建 CLI 与运行时壳：

- **载荷**打包一次，内嵌为单文件安装器或作为 sidecar 携带；
- **流**——选模式、选目标、流出真实进度；
- 可插拔 **targets**：
  - `install` —— 各平台 NSIS 式注册：Windows ARP 条目、快捷方式
    （带 AUMID）、Explorer 右键动词、深链接，用户级或机器级（自提权）；
    Linux `.desktop` 启动器 + 桌面动作；macOS `.app` 补全 + Launch
    Services——以及任何平台都不写系统状态的便携模式；
  - `flash` —— 镜像写入块设备并做写后校验。

向导本身是**声明式管线**（`mode | scope | license | content |
install`，自由排序），安装页显示真实的按阶段加权进度条与可折叠终端，
逐行记录每个文件操作——详细度由 `shell.log-level` 配置。

Windows 上壳有两副面孔：hikari WebView UI 与内嵌的**egui 降级界面**
（完全不需要 WebView2）——同一流程、同一清单（`--fallback` 强制）。
固定版本 WebView2 运行时可随载荷携带，安装壳与已装应用共享一份副本。

## 示例

一个 demo 端到端覆盖交付——真实 Tauri 2 载荷（`demo-app/`）、基于
[@celestia-island/hikari](https://github.com/celestia-island/hikari)
的安装壳（`shell/`）、一份清单：

```bash
just demo                                        # 暂存 → 构建 → 运行安装壳
just demo -- --fallback                          # 强制离线 egui 壳
cargo run --example demo_install                 # 生成 .shun 包 + 本机安装
cargo run --example demo_install -- --portable   # 便携安装（零系统状态）
cargo run --example demo_flash                   # 枚举可烧写设备
```

完整字段参考：[配置指南](./guides/configuration.md)
（[English](../en/guides/configuration.md)）。

## 状态

当前发布：**0.2.1**。crate 正面向 celestia 生态的三个真实消费者打磨——
WoWSP 安装器壳、shittim-chest 本地版、evernight 镜像烧写器。API 在次要
版本间跟随这三个消费者——预期来自其集成反馈的增量变更。

## 结构

| 路径 | 角色 |
| --- | --- |
| `src/config.rs` | 配置 schema + `[package.metadata.shun]` 加载器 |
| `src/flow.rs` | 流模型——壳渲染的进度与日志事件 |
| `src/payload.rs` | 载荷打包 / 清单 / 流式解压 |
| `src/targets/` | 安装 target（Windows/Linux/macOS 注册）与烧写 target |
| `demo-app/` | ShunDemo——Tauri 2 载荷应用（示例 UI、交付清单） |
| `shell/` | 安装壳：hikari UI（Tauri）+ egui 离线降级 |
| `docs/` | 分语言指南与设计笔记 |

## 开发

```bash
just fetch   # 拉取共享 celestia-devtools recipes（一次）
just ci      # fmt-check + clippy + test
```

工作经 `feat/*` / `fix/*` 分支以 squash 合并的 PR 落到 `master`。完整
惯例见 [AGENTS.md](../../AGENTS.md)。

## 许可证

SySL-1.0 —— 见 [LICENSE](../../LICENSE)。
