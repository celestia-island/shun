<p align="center"><img src="../logo.webp" alt="Shun" width="240" /></p>

<h1 align="center">Shun</h1>

<p align="center"><strong>流驱动的 payload 交付运行时——安装器、烧写器与便携模式</strong></p>

<div align="center">

[![License: SySL-1.0](https://img.shields.io/badge/License-SySL--1.0-blue.svg)](https://sysl.celestia.world)
[![GitHub](https://img.shields.io/badge/github-celestia--island%2Fshun-blue.svg)](https://github.com/celestia-island/shun)
[![Checks](https://img.shields.io/github/actions/workflow/status/celestia-island/shun/checks.yml)](https://github.com/celestia-island/shun/actions/workflows/checks.yml)
[![docs.rs](https://docs.rs/shun/badge.svg)](https://docs.rs/shun)

</div>

<div align="center">

[English](../README.md) ·
**简体中文**

</div>

---

shun 包含桌面软件发布的**交付**半边。一份配置文件同时驱动构建 CLI 和运行时壳：

- **payload**——应用目录打包一次，内嵌为单文件安装器或作为 sidecar 携带；
- **flow**——选择模式、选择目标、流式真实进度事件；
- 可插拔 **targets**：
  - `install` —— 直接 Windows 注册（用户级 ARP 条目、自拷贝卸载器、开始菜单快捷方式、深链）*以及* 零注册表的便携模式；
  - `flash` —— 镜像写入块设备并做写后校验。

在 Windows 上，双变体 WebView2 策略覆盖干净机器：标准发行物要求系统运行时，完整自包含发行物则**私有携带固定版本 WebView2 运行时**——一份副本由安装器壳与已装应用共享，横跨安装与便携模式，免管理员、零系统写入。

## 示例

demo 示例端到端交付一个替身应用，兼作真实机器上的集成检查：

```bash
cargo run --example demo_flash                        # 枚举可烧写设备
cargo run --example demo_install                      # 生成 ShunDemo.shun + 本机安装
cargo run --example demo_install -- --portable        # 便携安装（不写注册表）
cargo run --example demo_install -- --uninstall       # 卸载（清干净全部痕迹）
```

`demo_install` 生成安装包 `ShunDemo.shun`（zstd tar + SHA-256 清单）在工作目录中，
以流式进度解压，并在本机模式下执行 直接 Windows 注册。Tauri demo 壳
（`shell/`，基于 [@celestia-island/hikari](https://github.com/celestia-island/hikari)）
用完整 UI 渲染同一流程，构建时嵌入 payload。

交付清单本身位于 demo crate：

```toml
[package.metadata.shun]
product = "ShunDemo"
publisher = "celestia-island"
payload = "../examples/demo_payload"
main-exe = "bin/shun-demo.cmd"

[package.metadata.shun.install]
local = true
portable = true
```

完整字段参考见
[docs/en/guides/configuration.md](../en/guides/configuration.md)
（[简体中文](./guides/configuration.md)），包含 WebView2 策略矩阵。

## 状态

预发布；crate 正在面向 celestia 生态的三个真实消费者打磨——WoWSP 安装器壳、
shittim-chest 本地版和 evernight 镜像烧写器。活跃开发在 `dev` 分支；首个交付
流完成后 `master` 将接收初始发布提交。`0.1` 之前 API 不稳定。

## 结构

| 路径 | 角色 |
| --- | --- |
| `src/config.rs` | 配置 schema + `[package.metadata.shun]` 加载器 |
| `src/flow.rs` | 流模型——壳渲染的进度事件 |
| `src/payload.rs` | payload 打包 / 清单 / 流式解压 |
| `src/targets/install.rs` | 安装 target：注册后端、便携模式 |
| `src/targets/flash.rs` | 烧写 target：块设备写入 + 校验 |
| `shell/` | 安装流之上的 Tauri demo 壳（hikari UI） |
| `docs/` | 分语言指南与设计笔记 |

## 开发

```bash
just fetch   # 拉取共享 celestia-devtools recipes（一次）
just ci      # fmt-check + clippy + test
```

工作流：快速准备在 `dev` 分支；`master` 接收初始发布提交，之后全部通过 PR 合入。

## 许可证

SySL-1.0 —— 见 [LICENSE](../LICENSE)。
