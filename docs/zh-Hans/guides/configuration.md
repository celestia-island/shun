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
desktop-shortcut = "ask"                   # always | never | ask（向导复选框，默认勾选；无头运行按快捷方式是否存在解析 ask）
start-menu-shortcut = "always"             # always | never | ask（默认 always，向导只询问桌面那份）
launch-after-install = "ask"               # always | never | ask（完成页复选框，默认勾选；无该选项的壳可固定取值）
# default-dir = "%LOCALAPPDATA%\\Programs\\Wowsp"  # 覆盖默认安装位置（运行时展开 %VAR%/${VAR}）；已注册安装位置仍优先——见「安装位置解析」
deep-links = ["shundemo"]                  # 应用持有的 URL scheme（myapp://…）
aumid = "celestia-island.ShunDemo"         # 默认由 publisher + product 生成
icon = "assets/icon.png"                   # 载荷内启动器图标（Linux 的 Icon=）
root-dir-folder = "ShunDemo"               # 仅旧垫层通道生效（forbid-drive-root = false）：裸盘符根目录下垫的文件夹（D:\ → D:\ShunDemo；默认取产品名）
# forbid-drive-root = true                 # 直接拒绝裸盘符根目录目标（默认行为——见「根盘保护」）；false 恢复上面的垫层通道
# forbid-non-empty = true                  # 全新安装拒绝非空目录（默认行为——见「安装位置规则」）；false 恢复从前不设防的通道
# non-empty-allowlist = ["desktop.ini", "Thumbs.db", ".DS_Store"]  # 非空拒绝忽略的垃圾文件名（不区分大小写）
# [[package.metadata.shun.install.dir-rules]]  # 声明式安装位置规则（见「安装位置规则」）
# files = ["WorldOfWarships.exe"]              # 触发文件名，按目标目录与其直接子目录的直接子项检查
# action = "block"                             # block（默认）| relocate
# message = "This folder holds the game client."  # 英文原因，原样显示
# [package.metadata.shun.install.dir-rules.message-locales]  # 按向导语言覆盖（命中优先于 message）
# zh-Hans = "该目录是游戏客户端。"

[[package.metadata.shun.install.verbs]]    # 右键菜单动词（Explorer 动词 / Desktop Action）
key = "open-data"                          # 稳定动词 id
display = "打开数据目录"                    # 菜单文案
target = "data-folder"                     # data-folder | uninstall | app
# arguments = "--safe"                     # 仅 app 目标：附加命令行参数

[package.metadata.shun.webview2]           # 仅 Windows
type = "skip"                              # skip | evergreen-installer | fixed-version
# path = "WebView2Runtime"                 # fixed-version 专用：解压后的运行时目录
# silent-install = true                    # evergreen-installer：无运行时的机器上执行
                                           #   自带的安装器（默认开启——向导先询问，
                                           #   headless 静默执行）
# download-url = "https://…"               # evergreen-installer：不携带安装器时，安装期联网获取
                                           #   Evergreen 安装器（与自带安装器同一同意流程）
# warn-missing = true                      # skip / evergreen-installer：egui 降级界面在
                                           #   开始与结束时警告并附微软官方下载页链接
                                           #   （默认开启）

[[package.metadata.shun.steps]]            # 有序向导管线（可选）
columns = 2              # 可选：模式网格列数；默认每个模式一列
kind = "mode"                              # mode | scope | license | content | pairing | install
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
| `install` | table | 双模式全开 | `local` / `portable` 开关、`portable-marker` 标记文件名、`desktop-shortcut` / `start-menu-shortcut` / `launch-after-install` 策略、`default-dir`（环境变量展开的默认位置覆盖）、`verbs`、`deep-links`、`aumid`、`icon`、`forbid-drive-root`（拒绝裸盘符根目录目标；默认开启——见「根盘保护」）、`root-dir-folder`（退出拒绝后垫层通道里垫在裸盘符根目录下的文件夹，默认取产品名）、`forbid-non-empty`（全新安装拒绝非空目录；默认开启）、`non-empty-allowlist`（非空拒绝忽略的垃圾文件名）、`dir-rules`（声明式安装位置规则——见「安装位置规则」） |
| `attachments` | 表格数组 | 无 | 可选附件资源（资产包）：`key` / `title` / `dest` / `online.url`；精简版在安装时下载 |
| `update` | table | 无 | 更新监视：`sources`（镜像根 URL，按顺序探测）+ `files`，解析到第一个可达源之下 |
| `webview2` | table | `skip` | Windows 运行时策略 |
| `steps` | 表格数组 | 默认管线 | 有序向导管线（`mode`/`scope`/`license`/`content`/`pairing`/`install`，自由排序；恰好一个 `install`） |
| `script` | table | 无 | payload 脚本钩子：`runner` + `hooks[]`（`phase`/`script`）——见脚本设计笔记 |
| `variant` | 表格数组 | 无 | 由 `shun build --variant` 旗标选取的命名构建配置 |
| `signing` | table | 不签名 | 应用于构建产物的 Authenticode / codesign 配置 |
| `license-sysl` | table | 无 | 从 SySL 仓库打包许可证及官方译文 |
| `msix.logo-background` | 颜色 | `transparent` | 透明 MSIX 图标底下的底板色 |
| `flash` | table | — | 声明烧写目标 |

## 快捷方式策略

`install.desktop-shortcut` 与 `install.start-menu-shortcut` 按轨道解析。
向导各面显示复选框（默认勾选——NSIS 惯例）；无头轨道（`--silent`）按
**存在性**解析 `ask`：已存在的快捷方式原位刷新，缺失的不凭空创建。静默
更新绝不能撒出用户没有要过的图标——AV/EDR 启发式盯的正是这种行为，而且
用户删掉的快捷方式必须保持删除。`always` / `never` 在所有轨道固定取值；
显式 `--shortcut-desktop` / `--shortcut-menu` 旗标（更新器契约）压过
一切。卸载无条件移除快捷方式。

## 安装后启动

`install.launch-after-install` 决定完成页的惯例：`ask`（默认）跟随向导复选
框——即 NSIS 的「安装完成后立即启动」开关，无头运行时视为勾选；没有该选项
的壳可用 `always` / `never` 固定取值。

安装流程本身从不启动任何东西，由壳在安装成功后调用
`shun::targets::install::launch`：它在安装目录内以游离方式启动配置的
`main-exe`（不等待，安装器进程随即可以退出）；当
`InstallContext::launch_after_install` 解析为 false、或未声明入口点时，
调用是空操作。入口点已声明但文件缺失时，照常返回 `MissingEntry` 错误。

## 根盘保护

裸文件系统根目录目标——选中的盘符（如 `D:\`，`D:`、UNC 共享根
`\\server\share`、POSIX 的 `/` 同理）——不会接收载荷。自 0.6.30 起，
默认行为是**直接拒绝**该位置：`install.forbid-drive-root`（默认
`true`）让共享的目录防护将其拒之门外——向导各面会在行内标出该路径
并锁住「下一步」按钮，无头运行则大声失败——因为投放到根目录的载荷
会把文件撒满整个盘符根，卸载也会试图清空整块盘。设置
`forbid-drive-root = false` 可恢复 0.6.30 之前的垫层通道：
`InstallContext::apply_config` 会在根目录下自动垫一层文件夹，默认取
产品名，可用 `install.root-dir-folder` 自定义，向导弹窗也会在选中或
输入根目录的当下改写路径框。

## 目录预检钩子

`check-dir` 脚本阶段（见脚本设计笔记）运行在同一批防护之内、任何
写入之前，且**仅针对全新安装**：目录里已有本产品的安装（磁盘清单或
主程序存在）即为升级——其位置决定在落盘时已经做出，现在再否决会把
静默更新晾在半空。钩子把 `SHUN_DIR_BLOCKED` 设为人类可读的原因即可
否决该位置；向导原样显示该值，脚本因此可通过 `SHUN_LANGUAGE` 做
本地化。钩子报错时，检查连同其错误文本一起失败。声明式
`dir-rules` 表（见「安装位置规则」）与该阶段并行运行、**先于**
钩子——最具体的否决先答；钩子本身没有变化。

## 安装位置规则

各位置防护形制一致：目标越空，交付越安全。默认集合是上面的
根盘保护加上**非空拒绝**——`install.forbid-non-empty`（默认
`true`）让全新安装拒绝已装有外来文件的目标目录，因为在它们
旁边交付会把产品搅在一起：更新时的原子交换会把整个目录挪开，
卸载清扫也会走进其中。升级（磁盘清单或主程序存在）恒放行——
其位置决定在落盘时已经做出，再否决会把静默更新晾在半空。
`install.forbid-non-empty = false` 恢复从前来者不拒的通道；
`install.non-empty-allowlist`（默认 `["desktop.ini",
"Thumbs.db", ".DS_Store"]`，不区分大小写）列出拒绝逻辑忽略的
系统垃圾文件名，于是只装着垃圾的文件夹视同空目录。

`[[package.metadata.shun.install.dir-rules]]` 是 `check-dir`
钩子的数据化兄弟——对「这个位置不对」的声明式回答，出身于
WoWSP：玩家把插件装在游戏客户端旁边，更新的目录交换差点把
整个游戏文件夹挪开：

```toml
[[package.metadata.shun.install.dir-rules]]
files = ["WorldOfWarships.exe"]     # 触发文件名，按目标目录与其直接子目录的直接子项检查
action = "block"                    # block（默认）| relocate
message = "This folder holds the game client — pick another location."  # 英文原因，原样显示

[package.metadata.shun.install.dir-rules.message-locales]  # 按向导语言覆盖
zh-Hans = "该目录是游戏客户端，请另选位置。"
```

- `block`（默认）**仅否决全新安装**——已住在那里的安装继续原地
  更新（「绝不晾在半空」规则）。
- `relocate` 标记该位置不宜**共享**：全新选择像 `block` 一样被
  否决，且已存在的安装会被标记——静默轨道移交交互式向导，不再
  原地更新。
- `message-locales` 取代钩子脚本自己的 `SHUN_LANGUAGE` 分支：
  命中的条目优先于 `message`，后者是英文兜底。

全新安装的防护求值顺序——最具体的否决先答：**dir-rules →
check-dir 脚本钩子 → 非空拒绝**；空路径门禁与根盘保护先于它们
作答，可写性检查无条件执行。升级时只有 `relocate` 动作的规则
能让位置出错。

### 升级交换的安全底线

2026-10 的一次事故之后，原子交换本身多了两道硬底线——当时一个
与游戏客户端同住父目录的 WoWSP 安装，整个目录（连带游戏）被改名
挪进了备份槽，随后备份清扫把它摧毁：

- 本地模式的升级**拒绝交换**任何持有 shun 从未交付之内容的目录
  ——账面以磁盘清单、清单文件本身、便携标记、复制的卸载器与
  白名单垃圾文件为准——递归、任意深度；读不了的目录、解析不了的清单都算外来
  内容，`relocate` 规则向下探一层命中的桩文件（共享游戏根形状）
  同样算。安装会响亮地失败，而不是搬走外来内容：出路由上面的
  搬迁车道或另选全新位置给出。便携安装豁免——可执行文件旁边放
  数据正是该模式的设计。
- 被换出的备份**只有在 shun 能账清其中每一个文件（任意深度）时才会
  删除**。任何外来内容都会原地保留并给出点名警告；挡住备份槽
  的陈旧 `.shun-old` 会被改名挪到带时间戳的兄弟位置而不是被
  摧毁；删除失败会以警告形式报告，绝不静默吞掉。

### 搬迁

静默轨道（`--silent --dir=<旧目录>`）经规则发现被标记的安装后，
以 `--relocate-from <旧目录>` 重新拉起交互式向导（新的壳 CLI
旗标；它隐含交互轨道并与 `--silent` 互斥）。移交本身会打印原因并以退出码 `0` 结束、不安装任何
东西——更新器契约看到的是成功，用户眼前接手的交互式向导才是
结果的主人。已知局限：完全无交互的会话（没有用户桌面）发起的
静默更新会拉起一个无人看见的向导，这类安装只能停在旧版本，
直到有人交互登录。向导直接在位置步骤
打开，附本地化提示说明旧位置已不再有效，被标记的目录也会从
快捷候选中剔除。新安装落盘后，只有产品自己的清单所列文件——
外加磁盘清单、便携标记与复制的卸载器——会被清扫出旧目录；
外来文件（比如宿主游戏）绝不被触碰，旧目录本身只在清空后
移除。注册信息（ARP、快捷方式）跟随新位置——清扫从不注销任何
东西。取消向导则旧安装保持已安装、继续在旧版本上运行——只是再也无法
静默原地更新：此后每一次静默更新都会重新触发搬迁移交、向导再次
打开，直到用户完成搬迁。「绝不晾在半空」规则依然成立——安装
绝不会被弄坏，只是静默更新在等待搬迁。

## 安装位置解析

一次运行的起始目标 —— 向导弹窗预选的路径，以及命令行未携带
`--dir` 时无界面通道的安装目标 —— 遵循统一的优先级：产品的
**已注册安装位置**（ARP 的 `InstallLocation`，先查 HKCU 再查
HKLM）在其目录仍然存在时领跑候选链；其后是清单配置的
`install.default-dir`，再后是常规候选链（每用户 appdata、
Program Files、每个固定盘各一文件夹）。于是升级会落在上一版
本实际所在的目录，而不是把自定义路径的安装搬迁到钉死的默认
位置；显式 `--dir`（更新器契约）依然压倒一切。目录已不存在的
注册位置会被忽略 —— 被删掉的安装不得锚定下一次安装。

## WebView2 策略

| `type` | 携带物 | 前提 | 说明 |
| --- | --- | --- | --- |
| `skip` | 无 | 系统 WebView2 | 标准发行物 |
| `evergreen-installer` | Evergreen 离线安装器（约 127 MB） | 安装时提权 | 注册系统级运行时 |
| `fixed-version` | 解压后的运行时目录 | 无 | 私有副本由壳与已装应用共享，横跨安装与便携模式 |

实测（2026-09，v151 x64）：fixed-version 运行时**压缩约 293 MB /
解压约 661 MB**——已接受的成本，与其他打包器相当。发行物只内嵌
**一份**副本：壳把 payload 的运行时子树暂存到
`%LOCALAPPDATA%\shun\<product>\webview2` 并把
`WEBVIEW2_BROWSER_EXECUTABLE_FOLDER` 指向那里完成自举；解压感知
哈希，重装与已暂存文件会被沿用而非重写（"Reusing" 进度事件）——
同一引擎不会出现第二份。

### 无运行时机器的降级

面阶梯永远不会因缺少运行时而死亡——只会降级。阶梯解析前，壳用加载器
自身的判定（`wry::webview_version()`，正是 tauri 创建 webview 前的同一道
门禁）探测运行时，注册表残值再也无法把 webview 面送进 tauri 那个英文
"Could not find the WebView2 Runtime" 弹框：

- `evergreen-installer` + `silent-install`（默认 `true`）：运行时的获取
  位于安装步骤——初始化只解码并暂存自带的安装器（按 payload 的
  `webview2/` 前缀发现）到用户级缓存，面阶梯解析前不执行任何安装。
  向导运行会先停在同意询问上（「安装运行时 / 跳过，仅安装主程序」），
  仅在用户同意后才可见地执行获取——安装器以自身窗口运行，其清单要求
  提权时走 UAC。headless 轨道以 `silent-install` 作为同意，以
  `/silent /install` 静默执行。获取成功即原地升级机器——稳定后加载器
  即报告运行时可用。
- 不携带安装器但声明了 `download-url` 的构建，会在安装步骤的同一获取
  流程中、经同一同意之后从该地址联网获取 Evergreen 安装器（headless
  运行仍由 `silent-install` 把关）——lite 版通过网络而非自带 payload
  升级机器。慢网或断网永远不会阻塞首个界面：获取发生在安装流程内，
  降级横幅也会说明运行时正在到来。
- 无论结果如何，机器仍无运行时则落到 egui 面。`warn-missing`（默认
  `true`）且未携带安装器时，该面在向导开始与结束处各警告一次，并内嵌
  跳转到微软官方 WebView2 下载页的链接。两个开关均可按产品关闭：

```toml
[package.metadata.shun.webview2]
type = "evergreen-installer"
silent-install = false                  # 永不自动执行自带的安装器
warn-missing = false                    # 静默降级，不带警告横幅
```

一个值得了解的上游限制：tauri 的运行时门禁不读取
`WEBVIEW2_BROWSER_EXECUTABLE_FOLDER`，因此"仅 fixed-version、未安装
Evergreen"的机器即便本可创建 webview，也会落到 egui 面——壳优先选择
一定能渲染的面。

## MSIX 图标底板

Windows 会在每个忽略 `BackgroundColor="transparent"` 的表面上，把
打包桌面应用的图标垫在默认系统蓝（`#0078D7`）上——App Installer
对话框就是其中之一。`msix.logo-background` 接受 `#RRGGBB` 颜色：
shun CLI 把透明图标合成到该色上作为包资产，并把它声明为
`BackgroundColor`，品牌由此掌控每个表面的底板色：

```toml
[package.metadata.shun.msix]
# identity-name / publisher / display-name / executable ...
logo-background = "#0F172A"
```

## 离线壳

安装壳在 hikari webview UI 之外还内嵌了一个 egui 离线 UI。它由同一
份清单与 payload 驱动（一份流程，三个渲染器），不渲染特效，并在
Windows 缺少 WebView2 时被选中（横幅会明确说明缺失的环境）或被
手动强制：

```bash
shun-installer --no-webview   # 同一向导，离线渲染器（--fallback 为旧拼写）
```

两个 UI 都支持离线截图——窗口内容以 `PrintWindow` 捕获，无需桌面
自动化，保存后进程即退出：

```bash
shun-installer --screenshot=ui.png                 # hikari（webview）UI
shun-installer --no-webview --screenshot=ui.png    # egui 离线 UI
```

## 壳 UI

`[package.metadata.shun.shell]`（独立文档中使用 `shell` 键）配置运行时壳：

```toml
[shell]
timeline = "left"          # top（顶部横排）| left（左侧竖排）
log-level = "all"          # all（默认）| files | scripts | off
log-order = "newest"      # newest（默认，最新在顶）| oldest（追加在尾部）
language = "auto"          # auto | en | zh-Hans | zh-Hant | ja | ko | fr | ru | es | de | pt
faces = ["webview", "egui", "tui"]  # 本产品壳可呈现的界面（默认三项全开）
silent-progress = false   # 为 --silent 运行显示被动进度小窗（默认关闭）

[shell.theme]
mode = "system"            # system | light | dark
accent = [34, 211, 238]    # RGB 通道——覆盖 --color-primary
```

`shell.faces` 限定本产品壳可呈现的界面——`webview` / `egui` /
`tui`；缺省即三项全开。headless CLI 面（`--silent`、ARP 卸载串）是
基底，无法禁用。请求被禁用的界面是错误；自动选择会跳过被禁用的面。

`shell.silent-progress` 让无头通道带上被动进度小窗：`--silent` 运行会额外渲染一个置顶小窗，同步 stdout 正在报告的内容（当前步骤、有进度时的百分比）——没有页面、没有点击，stdout 原样不变。面向由更新器无人值守拉起安装器的产品：慢机器上安装不再被读成“什么都没发生”，也就没有人会把旧版本启动进正在交换的目录。这扇小窗走的是壳自身的设计语言：优先由 webview 车道渲染——无人值守更新所在的机器按定义已具备 WebView2（被替换的应用正需要它）——带自定义标题栏、主题令牌与本地化文案；当 webview 车道起不来（运行时缺失、face 被禁用、显式 `--no-webview`）时，由同样造型的 egui 小窗接替。没有桌面会话的环境（服务、CI）自动保持纯控制台。
### 向导语言

向导在**第一步**询问语言：选择器列出全部十个支持的语言，每项都以
自身语言标注（English、简体中文、日本語……）。切换后 UI 文案与许可
文档立即随之切换——协议文档按 `license-locales` / `locale-paths`
跟随所选语言——并且该选择会记入每用户偏好文件，下次运行自动沿用
（`<本地应用数据目录>/<产品>/installer-prefs.json`，例如
`%LOCALAPPDATA%\ShunDemo\installer-prefs.json`，内容形如
`{ "language": "zh-Hans" }`）。便携运行只把选择保存在内存中——便携
副本不写任何系统状态。`shell.language` 仍是配置层面的固定项，但
记住的选择优先于它，其后才是系统语言。

所选语言还会随安装流程传递：payload 脚本通过 `SHUN_LANGUAGE`
环境变量拿到它，同时它也被写入磁盘上的安装清单（修复/更新等后续
流程可以读取）。往已安装应用自身的配置里写什么由 payload 的脚本
决定——shun 只导出事实。

## 配对步骤

`pairing` 步骤为本机申领凭据——反向通道：本机显示验证码，操作员
在控制面板里输入它。一个窗格，两条通道：

```toml
[[package.metadata.shun.steps]]
kind = "pairing"

[package.metadata.shun.steps.pairing.source]
kind = "gateway"                     # gateway | scripts
official = "https://gateway.example.test"   # 预选的网关根 URL
# allow-custom = true                # 同时提供自定义网关输入（默认开启）

[package.metadata.shun.steps.pairing.identity]
node-id = true                       # 网关协议以此为主键
name = false                         # 可选设备名（按主机名预填）
# tier = 1                           # 申领时赋予的层级（产品固定时声明）

# env-file = "pairing.env"           # gateway 通道：申领到的凭据写入安装目录内的
                                     #   位置（.env 风格）
```

- `gateway` —— 内置 JSON-RPC 客户端（`pairing.request` 铸码、
  `pairing.await` 长轮询、`pairing.accept` 交付凭据）。目标机器无需
  任何脚本运行时；该通道要求 `node-id` 与 `env-file`。
- `scripts` —— 产品自有激活：`request` / `await` / `record` 三份
  payload 相对路径的 Python 脚本（`python3`，宿主机只有无版本号
  名字时回退 `python`）。窗格答案以单个 JSON 对象从脚本 **stdin**
  传入——绝不走 argv；每个阶段的 stdout 必须恰好一个 JSON 值；
  `record` 持久化申领结果（`env-file` 会随配置携带但不被消费）。
  每个阶段都有时限：挂死的脚本会被杀死，而不是干等。

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

## 构建变体

`[[package.metadata.shun.variant]]`（单数——表格数组惯用法）声明
`shun build --variant <name>` 旗标选取的命名产物配置——安装器矩阵
的各条轴（架构与系统来自 `--target`，资源包跟随 payload 目录）：

```toml
[[package.metadata.shun.variant]]
name = "lite"                        # id + 产物名后缀
payload = "payload-lite"             # payload 目录覆盖
faces = ["egui", "tui"]              # 该变体壳的界面覆盖

[package.metadata.shun.variant.env]  # 传递给壳的 cargo 构建
SHUN_FLAVOR = "lite"                 # （及其 build.rs）——风味标记
```

应用变体会替换 payload 目录与壳的界面清单；`env` 随壳构建传递，
风味可借此嵌入标记。未知名字是错误。

## 代码签名

`[package.metadata.shun.signing]` 配置构建产物的签名——`shun build`
在流程末尾为安装器签名，`shun sign` 为独立文件签名（均可
`--no-sign` 跳过）。缺省即不签名，本地测试无妨：

```toml
[package.metadata.shun.signing.windows]   # 经 signtool 的 Authenticode
enabled = true
thumbprint = "<证书指纹>"                 # 从用户/机器证书库选取（/sha1）
# timestamp-url = "http://timestamp.digicert.com"   # RFC 3161（/tr，默认值）

[package.metadata.shun.signing.macos]     # codesign
enabled = true
identity = "<签名身份>"
```

## 许可与自定义步骤

```toml
license = "docs/LICENSE.md"                # markdown，渲染于许可步骤

[license-locales]                          # 分语言许可覆盖
zh-Hans = "docs/LICENSE.zh-Hans.md"
ja = "docs/LICENSE.ja.md"

[[licenses]]                               # 追加许可文档
title = "Copyright notice"                 # 可选：正文上方的标题
path = "NOTICE.md"                         # markdown，相对清单文件
[licenses.locale-paths]                    # 该文档的分语言覆盖
zh-Hans = "NOTICE.zh-Hans.md"

[[custom-steps]]                           # 注入 markdown 内容步骤
key = "whats-new"
after = "license"
title = "What's New"
markdown = "docs/whats-new.md"
```

`license` + `license-locales` 是单文档简写；表格数组 `licenses` 声明
更多许可文档，每份可带可选 `title` 与自己的 `locale-paths`。两者可以
同时使用：简写文档排在最前，数组按声明顺序依次排列。匹配当前语言的
路径（`license-locales` 或 `locale-paths`）优先于基础文档。许可步骤
一次显示一份文档，多份时以「上一份/下一份」翻页；唯一的同意复选框
覆盖全部文档。解析后的 JSON 中每份文档存于 `licenses`（title + body），
而旧的 `body` 字符串把所有正文用分隔线拼接，只读 `body` 的渲染器也能
完整展示整个协议。

界面内置十种语言的默认文案（`en`、`zh-Hans`、`zh-Hant`、`ja`、`ko`、`fr`、
`ru`、`es`、`de`、`pt`）；`shell.language = "auto"` 跟随系统，固定语言可直接指定，
分语言许可覆盖保证本地化协议正常工作。

## 从 SySL 仓库取许可证

`[package.metadata.shun.license-sysl]` 构建期从许可证自己的仓库打包
Synthetic Source License 及其官方译文（需 `online` feature）——向导
无需在仓库里存放许可证文件即可按用户语言展示协议：

```toml
[package.metadata.shun.license-sysl]
repo = "celestia-island/sysl"        # 默认
branch = "main"                      # 默认
locales = ["zh-Hans", "ja"]          # 要打包的译文（根目录英文始终打包）
```

语言标签映射到 sysl 仓库的 i18n 目录（`zh-Hans` → `zhs`、
`zh-Hant` → `zht`；`ja`/`ko`/`fr`/`ru`/`es`/`de`/`pt`/`ar` 直接
对应）。打包出的文档像其他许可文档一样渲染在许可步骤。

## 脚本钩子

`[package.metadata.shun.script]` 声明 payload 相对路径的运行器脚本，
交付流程在阶段边界执行它们。每产品一个运行器——目前只挂载
`duckscript`（内嵌解释器）：

```toml
[package.metadata.shun.script]
runner = "duckscript"

[[package.metadata.shun.script.hooks]]
phase = "post-install"               # prepare | post-install | pre-uninstall
script = "scripts/post_install.ds"   # payload 相对路径，随载荷打包
```

失败的钩子会中断整个流程。完整契约——阶段、钩子可见的环境
（含 `SHUN_LANGUAGE`）与示例——见[脚本设计笔记](../design/scripting.md)。
