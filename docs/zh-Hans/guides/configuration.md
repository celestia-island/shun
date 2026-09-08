# 交付清单参考

交付流程声明在应用自己的 `Cargo.toml` 里，位于 `[package.metadata.shun]`
——即 cargo-deb / cargo-wix 的惯例。产品身份默认取 `[package]`（`name`、
`version`），表格内其余字段定制交付流程。

```toml
[package.metadata.shun]
product = "ShunDemo"                       # 默认：包名
publisher = "celestia-island"              # ARP Publisher 字段
logo = "docs/logo.webp"                    # 壳的 logo 资产
payload = "examples/demo_payload"          # 打包进发行物的目录
main-exe = "bin/shun-demo.exe"             # payload 内入口点

[package.metadata.shun.install]            # install target（默认）
local = true                               # 注册安装（ARP、卸载器、快捷方式）
portable = true                            # 便携模式（.shun-portable 标记，零注册表）

[package.metadata.shun.webview2]           # 仅 Windows
type = "skip"                              # skip | evergreen-installer | fixed-version
# path = "WebView2Runtime"                 # fixed-version 专用：解压后的运行时目录

[package.metadata.shun.flash]              # flash target（可选）
require-removable = true                   # 拒绝非可移动设备
```

## 字段

| 字段 | 类型 | 默认 | 含义 |
| --- | --- | --- | --- |
| `product` | string | 包 `name` | 标题、ARP 显示名、烧写标签 |
| `publisher` | string | — | ARP Publisher 字段 |
| `logo` | path | — | 壳的 logo 资产（相对清单文件） |
| `payload` | path | — | 打包进发行物的目录 |
| `main-exe` | path | — | payload 内入口点（快捷方式目标） |
| `install` | table | 双模式全开 | `local` / `portable` 开关 |
| `webview2` | table | `skip` | Windows 运行时策略 |
| `msix.logo-background` | 颜色 | `transparent` | 透明 MSIX 图标底下的底板色 |
| `flash` | table | — | 声明烧写目标 |

## WebView2 策略

| `type` | 携带物 | 前提 | 说明 |
| --- | --- | --- | --- |
| `skip` | 无 | 系统 WebView2 | 标准发行物 |
| `evergreen-installer` | Evergreen 离线安装器（约 127 MB） | 安装时提权 | 注册系统级运行时 |
| `fixed-version` | 解压后的运行时目录 | 无 | 私有副本由壳与已装应用共享，横跨安装与便携模式 |

## 壳 UI

`[package.metadata.shun.shell]`（独立文档中使用 `shell` 键）配置运行时壳：

```toml
[shell]
timeline = "left"          # top（顶部横排）| left（左侧竖排）
language = "auto"          # auto | en | zh-Hans | zh-Hant | ja | ko | fr | ru | es

[shell.theme]
mode = "system"            # system | light | dark
accent = [34, 211, 238]    # RGB 通道——覆盖 --color-primary
```

## Payload 来源

`[package.metadata.shun.source]` 选择安装时 payload 的来源：

```toml
[source]
type = "embedded"          # payload 归档内嵌于安装器本体
```

```toml
[source]
type = "online"            # 安装器自行下载 payload
url = "https://github.com/<org>/<repo>/releases/latest/download/ShunDemo.shun"
```

在线安装器以单次流水线完成 **下载 → 解压 → 校验**：字节到达即按清单校验，
进度事件同时上报下载与解压两个阶段（多层进度）。把 `url` 指向发布源
（GitHub Releases 或任意 HTTP 主机），发布新包即完成安装器更新。

## 许可与自定义步骤

```toml
license = "docs/LICENSE.md"                # markdown，渲染于许可步骤

[license-locales]                          # 分语言许可覆盖
zh-Hans = "docs/LICENSE.zh-Hans.md"
ja = "docs/LICENSE.ja.md"

[[custom-steps]]                           # 注入 markdown 内容步骤
key = "whats-new"
after = "license"
title = "What's New"
markdown = "docs/whats-new.md"
```

界面内置八种语言的默认文案（`en`、`zh-Hans`、`zh-Hant`、`ja`、`ko`、`fr`、
`ru`、`es`）；`shell.language = "auto"` 跟随系统，固定语言可直接指定，
分语言许可覆盖保证本地化协议正常工作。
