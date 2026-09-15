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

[[package.metadata.shun.attachments]]
key = "models"
title = "2D/3D model pack"
dest = "models"
[package.metadata.shun.attachments.online]
url = "https://example.test/models.shun"   # 可选附件资源（精简版在安装时下载）

[package.metadata.shun.update]             # 更新监视（可选）
sources = [                                # 镜像根 URL，按顺序探测
  "https://mirror.example.test/shun-demo",
  "https://releases.example.test/shun-demo",
]
files = ["latest", "app-setup.exe"]        # 解析到获胜源之下

[package.metadata.shun.install]            # install target（默认）
local = true                               # 注册安装（ARP、卸载器、快捷方式）
portable = true                            # 便携模式（.shun-portable 标记，零注册表）
portable-marker = ".shun-portable"          # 便携副本写入的标记文件名（应用检测自有标记时可覆盖）
desktop-shortcut = "ask"                   # always | never | ask（向导复选框，默认勾选）
start-menu-shortcut = "always"             # always | never | ask（默认 always，向导只询问桌面那份）
launch-after-install = "ask"               # always | never | ask（完成页复选框，默认勾选；无该选项的壳可固定取值）
deep-links = ["shundemo"]                  # 应用持有的 URL scheme（myapp://…）
aumid = "celestia-island.ShunDemo"         # 默认由 publisher + product 生成
icon = "assets/icon.png"                   # 载荷内启动器图标（Linux 的 Icon=）

[[package.metadata.shun.install.verbs]]    # 右键菜单动词（Explorer 动词 / Desktop Action）
key = "open-data"                          # 稳定动词 id
display = "打开数据目录"                    # 菜单文案
target = "data-folder"                     # data-folder | uninstall | app
# arguments = "--safe"                     # 仅 app 目标：附加命令行参数

[package.metadata.shun.webview2]           # 仅 Windows
type = "skip"                              # skip | evergreen-installer | fixed-version
# path = "WebView2Runtime"                 # fixed-version 专用：解压后的运行时目录

[[package.metadata.shun.steps]]            # 有序向导管线（可选）
columns = 2              # 可选：模式网格列数；默认每个模式一列
kind = "mode"                              # mode | scope | license | content | install
align = "center"                           # 按步骤覆盖：center | start（默认由 kind 决定）

[[package.metadata.shun.steps]]
kind = "content"
title = "发布说明"                          # content 步骤带有标题…
markdown = "notes.md"                      # …与文档，构建期内联

[[package.metadata.shun.steps]]
kind = "install"                           # 恰好一个 install 步骤

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
| `install` | table | 双模式全开 | `local` / `portable` 开关、`portable-marker` 标记文件名、`desktop-shortcut` / `start-menu-shortcut` / `launch-after-install` 策略 |
| `attachments` | 表格数组 | 无 | 可选附件资源（资产包）：`key` / `title` / `dest` / `online.url`；精简版在安装时下载 |
| `update` | table | 无 | 更新监视：`sources`（镜像根 URL，按顺序探测）+ `files`，解析到第一个可达源之下 |
| `webview2` | table | `skip` | Windows 运行时策略 |
| `msix.logo-background` | 颜色 | `transparent` | 透明 MSIX 图标底下的底板色 |
| `flash` | table | — | 声明烧写目标 |

## 安装后启动

`install.launch-after-install` 决定完成页的惯例：`ask`（默认）跟随向导复选
框——即 NSIS 的「安装完成后立即启动」开关，无头运行时视为勾选；没有该选项
的壳可用 `always` / `never` 固定取值。

安装流程本身从不启动任何东西，由壳在安装成功后调用
`shun::targets::install::launch`：它在安装目录内以游离方式启动配置的
`main-exe`（不等待，安装器进程随即可以退出）；当
`InstallContext::launch_after_install` 解析为 false、或未声明入口点时，
调用是空操作。入口点已声明但文件缺失时，照常返回 `MissingEntry` 错误。

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
log-level = "all"          # all（默认）| files | scripts | off
log-order = "newest"      # newest（默认，最新在顶）| oldest（追加在尾部）
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

## 更新监视

`[package.metadata.shun.update]`（独立文档中使用 `update` 键）声明壳
探测更新用的镜像源，以及解析到镜像之下的文件：

```toml
[update]
sources = [                                # 镜像根 URL，按顺序尝试
  "https://mirror.example.test/shun-demo",
  "https://releases.example.test/shun-demo",
]
files = ["latest", "app-setup.exe"]
```

运行时壳调用 `shun::update::resolve`：对每个源做一次短探测（GET 第一个
文件，10 秒超时），第一个应答 `200` 的源赢得整轮解析，声明的每个文件
都解析到获胜源之下的 URL（`<source>/<file>`）。不可达或非 200 的源以
警告记录跳过；全部无应答则什么都不解析。随后壳自行拉取解析出的 URL
——`latest` 版本标记以文本读取（`fetch_text`），产物下载走在线 payload
流水线（`OnlinePayload`：下载 → 解压 → 校验）。

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
