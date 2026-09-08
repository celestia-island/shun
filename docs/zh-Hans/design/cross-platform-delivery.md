# 设计笔记：跨平台交付

状态：**Windows 面已验证且缺口已补齐（2026-09）；Linux 与 macOS
的运行时注册后端已实现；移动端（Android/iOS）与鸿蒙已完成预研——
暂缓，不做。** 本笔记记录：(a) 对自动打包注册面的一次集中测试在
Windows 后端上证明了什么；(b) 曾经缺失的桌面快捷方式 / 任务栏标识 /
右键菜单面——现已实现，以及塑造它们的真机安全策略发现；(c) Linux
与 macOS 后端；(d) Android、iOS 与鸿蒙的可行性结论（暂缓）。

(a) 的可执行清单落在 `tests/registration_shortcuts.rs` 与
`tests/msix_pack.rs`。

## 1. 集中 Windows 测试证明了什么（2026-09，真机）

验证方式是在一台 Windows 11 真机上运行真实的 `InstallFlow`，再让
Windows 自己读回来（WScript.Shell COM 解析器、注册表、Windows SDK
的 MakeAppx）——而不是检查我们自己的输出：

| 面 | 结果 |
| --- | --- |
| 开始菜单 `.lnk` 能被 shell 解析 | **通过** —— `TargetPath` 与 `WorkingDirectory` 与安装上下文声明完全一致；`mslnk` 手工构造的合成 PIDL 可被正确解析 |
| 非 ASCII 安装路径（中文目录） | **通过** —— `顺测试目录` 安装目录经 COM 解析器逐字节回读一致 |
| `.lnk` 二进制对照 MS-SHLLINK | **通过** —— 头、CLSID `{00021401-…}`、标志位（target ID list + relative path + working dir + unicode）、无热键 |
| ARP 条目（HKCU） | **通过** —— 完整 NSIS 等价字段集：DisplayName/Version/Publisher/InstallLocation/DisplayIcon/UninstallString（带引号）、`NoModify`/`NoRepair`/`EstimatedSize` 为 DWORD；EstimatedSize 与载荷清单总量一致 |
| 卸载清理 | **通过** —— ARP 键、快捷方式、载荷、卸载器、目录全部移除（`tests/install_local.rs` 覆盖，此处复核） |
| MSIX 清单生成 | **通过** —— 标识、四段补零版本号、XML 转义字符串、正斜杠入口、runFullTrust |
| MSIX 真实打包（MakeAppx 10.0.26100） | **通过** —— 合法 OPC zip，含 `[Content_Types].xml` + `AppxManifest.xml`；产出的 `dist/shundemo-0.1.0-x64.msix` **无签名块**（符合设计：商店分发代签，或需信任自签证书） |

## 2. Windows 补缺落地（2026-09 实现）

以下各项在验证轮之后全部落地，每个面都在真机上由注册测试套件
驱动（`tests/registration_shortcuts.rs`）。

### 桌面快捷方式 —— `install.desktop-shortcut`

由策略（`always` | `never` | `ask`，NSIS 复选框惯例——egui 向导为
`ask` 显示默认勾选的开关，无头运行按勾选处理）解析后，与开始菜单
快捷方式一同写入。桌面用 **`SHGetKnownFolderPath(FOLDERID_Desktop)`**
解析——绝不用 `%USERPROFILE%\Desktop`，桌面被重定向（OneDrive、域
策略）时后者是错的。卸载时无条件移除（安装与卸载之间配置可能已
变化）。

**真机实测**：安全策略（杀软/EDR 的假快捷方式与勒索防护）通常会
**专门拦截在桌面创建 `.lnk`**——验证机上连提权 shell 的
`echo x > Desktop\probe.lnk` 都被拒，而 `.tmp` 随便写。因此桌面
快捷方式是**尽力而为**：写入被拒降级为警告，绝不使安装失败（开始
菜单快捷方式与 ARP 才是关键面）。测试套件会探测机器策略，对正常
路径与优雅降级路径分别断言。

### 任务栏标识 —— `install.aumid`

编程式任务栏固定依旧**被平台设计封锁**（无受支持 API；固定 hack
已在 Win10 移除）。已落地的是标识的一半：每个快捷方式都通过 Shell
COM 属性存储（`IShellLink` → `IPersistFile` → `IPropertyStore`，见
`src/targets/aumid.rs`——`mslnk` 只写字节）盖上
**`System.AppUserModel.ID`**，让任务栏分组、Jump List 与*用户发起*
的固定表现正确。默认 AUMID 由 `{publisher}.{product}` 生成；
`install.aumid` 可覆盖，应用应把同一值传给
`SetCurrentProcessExplicitAppUserModelID`。MSIX 安装经包标识免费获
得标识。盖章同样尽力而为（出于同一策略原因：验证机拒绝
`IPropertyStore::SetValue` 写 `.lnk`——0x80030005——那里降级为警
告）。

### 右键菜单动词 —— `[[install.verbs]]`

第一层已交付：每用户 Explorer 动词，位于
`HKCU\Software\Classes\Applications\<exe>\shell\<verb>\command`——
即文档化的 Application Registration 面，免提权，出现在应用的 exe
及其快捷方式上。三种动词目标在所有实现它们的平台上映射为命令行：
`data-folder`（打开安装目录）、`uninstall`（运行卸载器）、`app`
（入口 + 参数）。卸载只删自己创建的动词键，随后仅在空时删
`shell`/`Applications` 容器（别人注册的动词得以幸存）。第二层
（文件类型关联）与第三层（MSIX `FileExplorerExtension`）仍为后续。

### 深链接 —— `install.deep-links`

交付模型初稿就承诺过的能力，现在三个后端全部落地、全部每用户：
Windows 把每个 scheme 注册为 `HKCU\Software\Classes\<scheme>` 协议类
（空的 `URL Protocol` 标记 + 以 `%1` 接收 URL 的打开命令）；Linux 在
启动器上声明 `MimeType=x-scheme-handler/<scheme>;` 并用 `xdg-mime`
认领默认；macOS 合成的 `Info.plist` 携带 `CFBundleURLTypes`。scheme
统一规范化为小写 `[a-z0-9+.-]`（`"MyApp://"` → `myapp`）。卸载删除
Windows 协议类；移除 Linux 启动器即孤立该 handler
（`mimeapps.list` 行失效——已知并接受）。

### 为什么安装程序不请求 UAC 提权

桌面快捷方式的发现引出了这个问题，答案有三条：

1. **shun 写的全部是每用户面** —— HKCU、用户的开始菜单与桌面、
   `%LOCALAPPDATA%`。没有任何一处需要提权令牌，加 UAC 弹窗买不到
   任何东西，只添最坏的那种提示噪音（训练用户无脑点「是」）。
   这与 wowsp NSIS 模板、VS Code / Chrome「用户安装程」做的是同一
   个取舍。
2. **提权也修不了 `.lnk` 被拦截**：拦截是安全产品按「桌面目录 +
   `.lnk` 扩展名」下刀的文件系统过滤器——过滤器按策略拦截进程，
   不看 ACL，提权进程同样被拦（Windows 受控文件夹访问同理：除非
   应用被加白，管理员也被拦）。正确的应对就是已交付的：降级为
   警告，保住开始菜单快捷方式与 ARP。
3. **提权后写「用户面」是正确性陷阱**：提权进程解析的用户配置
   可能不同（管理员账户的桌面、`%APPDATA%`、注册表 hive 都可能不
   是安装用户的）——NSIS 全用户快捷方式的经典 bug 来源。真正需要
   提权的步骤让**步骤自身**提权：WebView2 Evergreen 引导器自带
   `requireAdministrator` 清单，shell 保持 `asInvoker` 并委托。

机器级范围（Program Files、HKLM ARP、全用户快捷方式）是某些产品
真正需要的**模式**——它已按此落地：刻意的可选（`install.scope`），
永不默认。见第 6 节。

### 健壮性发现——均已修复

- 产品名含文件名非法字符（`/\:*?"<>|`、结尾的点/空格）时，所有
  文件系统与注册表面（`.lnk` 名、ARP 键路径）统一做**词干净化**
  ——产品名里的 `\` 不再嵌套出注册表子键。
- ARP 的 `UninstallString` 传 `/uninstall`，而安装 shell 的无头
  解析只认 `--uninstall`——在 Windows 设置里点「卸载」实际会打开
  向导而不是卸载。两种拼写现在都接受。

## 3. Linux 与 macOS（运行时后端已实现；打包产物待续）

**两者都可行，且都与 Windows 共享同一条硬限制：任何平台的编程式
任务栏/程序坞固定都不存在。** 运行时 `Registration` 后端已落地；
构建侧的打包产物（经 `tauri-bundler` 出 deb/rpm、DMG）因需要各自
的原生构建宿主，仍为后续工作。

### Linux —— `LinuxRegistration`（src/targets/freedesktop.rs）

Windows 后端做的每件事都映射到 freedesktop 惯例，全部每用户
（`~/.local/share/...`）、免提权：

- **启动器注册** = 写 `<product>.desktop`（Name、Exec、Icon 来自
  `install.icon`、Categories，以及关键的 **`StartupWMClass`** = 入口
  可执行文件名——让用户发起的任务栏/程序坞固定归组到正确图标的
  字段）到 `~/.local/share/applications`，再对它跑
  `update-desktop-database`（工具缺席时跳过是安全的——桌面环境会
  惰性重扫）；
- **右键动词 + 卸载入口** = `Actions=` + `[Desktop Action <id>]` 组
  ——永远包含一个 **Uninstall** 动作，因为 GNOME Software / KDE
  Discover 只列出各自包后端追踪的应用：shun 安装的应用永远不会
  出现在那里。三种动词目标映射为 `xdg-open`、卸载器、入口 + 参数；
- **可执行位恢复** —— 载荷档案对每个条目都是 0644，后端把入口与
  拷贝出的卸载器 chmod 回 0755；
- **注销** = 删 `.desktop` + 刷新数据库。

`.desktop` 写入器是纯数据管道，在每个平台编译（并被单元测试）；
只有进程调用那一半是 Linux 门控的。任务栏/程序坞**固定依旧不可能**
（无跨桌面 API：GNOME 收藏夹是内部 gsettings 键、KDE 固定项在未记
文档的 appletsrc 里；视为用户动作）。文件管理器右键菜单（Nautilus
脚本 / Dolphin service menus）仍不在范围内。

**打包格式**（仍为构建侧、后续）：**tarball/便携（shun 已有）+ deb
（cargo-deb）+ rpm（cargo-generate-rpm）** 是最优子集——恰好是
`tauri-bundler` 的输出集（它是可为非 Tauri 载荷复用的库，
[cargo-packager] 亦然）。AppImage = 中等；Flatpak = 中高；snap =
高，缓做。诚实的「全发行版通跑」限制：glibc 只向前兼容，且 **musl
静态构建带不动 WebView 应用**（webkit2gtk 拖着整个 GTK C 栈）——
交付 shell 可以做 musl 静态，但被交付的 Tauri 应用必须按支持的最老
webkit2gtk-4.1 基线构建（Ubuntu 22.04 / Debian 12 / Fedora 37+
时代）。

### macOS —— `MacOSRegistration`（src/targets/macos.rs）

- **注册** = 定位入口可执行文件所在的 `.app` 包（最近的 `.app`
  祖先）；载荷没带 `Info.plist` 时合成最小的一份（`plist.rs`，纯
  函数、全平台单元测试）；恢复入口的可执行位；**递归剥离继承来的
  `com.apple.quarantine`**（浏览器给安装器盖了隔离戳，而 macOS 的
  拷贝保留 xattr——不剥离的话交付出的应用会继承用户已经答复过的
  Gatekeeper 拦截）；然后 `lsregister -f` 该包——Spotlight 与
  Launchpad 随之跟进。注销 = `lsregister -u`（文件由通用卸载流程
  删除）。裸可执行载荷（无 `.app`）注册为空操作——按便携惯例；
- **程序坞固定**：**无受支持 API**（`defaults write com.apple.dock`
  + `killall Dock` 的 hack 会践踏用户偏好且在新版 macOS 上不可
  靠）——Launchpad/Spotlight 的存在感（LS 注册）即等价物；
- **签名对真实分发仍是必须的流程工作**：Developer ID + hardened
  runtime + `notarytool` + staple；且下载来的 shell **会被转置**
  运行——自路径假设要相应处理；
- **打包产物**（后续）：DMG 走 tauri-bundler / `hdiutil`，`.pkg`
  仅用于管理员流程，Homebrew cask 是渠道。发布 **universal2**（双
  构建 + `lipo` + 重签）。

[cargo-deb]: https://github.com/kornelski/cargo-deb
[cargo-generate-rpm]: https://crates.io/crates/cargo-generate-rpm
[cargo-packager]: https://github.com/crabnebula-dev/cargo-packager

## 4. Android 与 iOS（预研）

**先重构认知：移动端的「安装」是平台所有、签名验证的行为。不存在
把应用目录的 zstd tar 流式写到用户自选位置的等价物。** shun 在移动
端的诚实角色是**构建/打包/签名管线**（「移动端的 cargo-dist」），
不是安装器运行时。

### Android

- **打包机制**：APK = 签名 zip（DEX + 资源 + 每 ABI `.so`）；
  管线 `aapt2` → `d8`/`r8` → 打包 → `zipalign` → `apksigner`。
  AAB 是 Play 发布格式（新应用强制）；侧载需要具体 APK
  （`bundletool build-apks` + 重签）。Rust 目标为带宿主工具的
  Tier 2。
- **2025–2026 仍在维护的工具**：Tauri 2 的 `tauri android build`
  （包装 cargo-mobile2 + Gradle；keystore 走 `keystore.properties`）、
  **cargo-ndk**（维护中）、**`apk` crate**（无 Gradle 的
  aapt2/d8/zipalign/apksigner）。**xbuild 与 cargo-apk 已死/休眠**
  ——不要建立在其上。Tauri 2 移动端官方已稳定；Android 侧比 iOS
  侧成熟。
- **运行时安装器：不可行/无意义。** OS 包管理器从签名 APK 安装并
  强制用户确认（仅设备所有者/MDM 静默）；**APK 本身就是安装器**。
  Play 政策还**禁止自更新/下载可执行代码**——shun 式更新器运行时
  对任何 Play 分发的应用都是违规。侧载在设计上退化：**未验证开发
  者的应用自 2026-09-30 起进入多步流程并等待 24 小时**。
- **快捷方式**：主屏图标与应用快捷方式（`shortcuts.xml`、
  `ShortcutManager`、`requestPinShortcut`）**只能由应用声明**——
  不存在安装期注入 API。构建期等价物是往 shun 打的包里生成
  `shortcuts.xml`/intent-filter。

### iOS

- **打包机制**：IPA = 带 `Payload/App.app` +
  `embedded.mobileprovision`（App ID + 权利 + 证书 + Ad Hoc UDID
  白名单）的 zip。渠道：App Store、TestFlight、Ad Hoc（每年每类
  100 台）、企业。**签名工具链（codesign、xcodebuild、钥匙串）仅
  存于 macOS**——硬性宿主要求。
- **运行时安装器：不可行**，除两个缝隙：(a) Ad Hoc OTA 清单
  （`itms-services://?...manifest.plist`）——容易、合法、小众；
  (b) 欧盟 Web Distribution / 替代市场机制——真实存在，但被 Apple
  资格门槛 + 公证 + 2026 年 10 月费率条款（5% 核心技术佣金）把门，
  且仅限欧盟。免费 Apple ID 侧载（AltStore/Sideloadly）是 7 天/3
  应用的爱好者路径，不可产品化。
- **快捷方式**：任何渠道下安装期都不存在任何东西——主屏图标、URL
  scheme、Universal Link 均为应用声明且经签名校验。
- **egui 后备**：Android = `android-activity` + winit + wgpu
  （Vulkan/GLES）；iOS = winit + wgpu（Metal）经 FFI 内嵌 UIKit 宿
  主 + Xcode 工程。两者都没有一站式方案——shun 打包管线恰好是缺失
  的那块。

## 5. 鸿蒙（预研——已决定暂缓）

**结论先行：2026 年「shun 支持鸿蒙」只能诚实地指一件事——一个产出
发布签名 HAP/APP 产物的构建期打包/签名目标，外加 `hdc install` 开发
机流程。运行时安装器（shun 的 NSIS 那一半）在 HarmonyOS NEXT 上既无
法律基础也无技术基础。**

生态（2025–2026）：HarmonyOS NEXT（5.0，2024-10）砍掉了 APK 兼容
层；要瞄准的线是 HarmonyOS 6+（API 20/23），仅中国、仅应用市场，
约占中国 OS 市场两成。OpenHarmony 是开源底座；商业鸿蒙是其上的华
为产品——打包器瞄准商业版。

- **包格式**：HAP（zip：`module.json5`、ArkTS 字节码、原生
  `libs/<abi>/*.so`）；HSP/HAR 共享包；`.app` = 应用市场提交包
  （`pack.info`）。工具链可 CLI 使用：`ohpm` + `hvigorw
  assembleHap` + `hap-sign-tool` + `app_packing_tool.jar`（官方支
  持无头 CI 构建）。
- **签名**：SHA256withECDSA；`.p12` 密钥库 + `.cer` 证书 + `.p7b`
  profile（包名、权限、debug 时含设备 UDID 白名单）；证书**由华为
  经 AppGallery Connect 签发**（个人注册免费——没有 Apple 式年
  费）。
- **Rust**：`aarch64/armv7/x86_64-unknown-linux-ohos` 为**带宿主
  工具的 Tier 2**（1.78 起 rustup 直装）。社区 `ohos.rs` 工具链
  （`cargo-ohos`、`napi-ohos`、`ohos-openssl`）是粘合剂；Rust 内核
  + ArkTS 壳是已验证架构（RustDesk OHOS）。**egui 受阻**：winit
  没有上游 OHOS 后端（仅社区 beta）。**Tauri**：官方但未合入的
  `feat/open-harmony` 分支（wry/tao 补丁、`cargo tauri ohos` CLI）
  今天可用但移动快——预计每几个月重钉版本。
- **分发诚实性**：消费者侧载实质关闭（仅应用市场；`hdc install`
  需开发者模式 + 华为签名 + UDID）。指定设备发布：每年 100 台、
  90 天有效期。企业分发目前限擎云企业 PC。**鸿蒙电脑**是真实的
  （ARM、商店分发、尚无侧载）——华为已*表态*未来开放 PC 侧载；
  这是唯一可能让桌面交付运行时在那儿成立的观察项，是承诺不是能
  力。

工作量表：HAP 打包目标**中等**；Rust 交叉步骤**易–中**（SDK clang
包装为链接器、TLS 走 ohos-openssl）；Tauri-on-OHOS 打包**中–难**
（上游未合）；egui 后备**难**（无 winit）；运行时安装/烧录
**不可行**。

## 6. 安装范围与声明式向导（2026-09 实现）

### 安装范围 —— `install.scope = user | machine | ask`

每用户仍是默认（见上文「为什么安装程序不请求 UAC 提权」）。
`machine` ——或 `ask` 被答成「所有用户」——把每个注册面翻到其机器
级等价物：ARP 条目落在 **HKLM**、快捷方式进**全用户开始菜单**
（`%ProgramData%`）、桌面快捷方式写**公共桌面**
（`FOLDERID_PublicDesktop`）、动词与深链接落
`HKLM\Software\Classes`。shell 在流程运行**之前**检测解析结果，
未提权时以 `runas` 携带用户的选择重启自身（`--silent --mode=…
--dir=… --scope=machine`）：UAC 确认是唯一的弹窗、只出现在真正
需要它的模式上——正是引导器模式，兑现承诺。卸载同样对称（ARP
的 `UninstallString` 拉起卸载器、同样方式提权）。机器范围仅限
Windows；Linux/macOS 后端明确拒绝。集成测试在非提权运行器上自
动跳过（管理员 shell 里跑 `cargo` 即真机验证）。

### 向导管线 —— `[[package.metadata.shun.steps]]`

向导从固定的 模式 → 安装 序列变为**声明式、有序、自由组合**的
管线。五种步骤：

```toml
[[package.metadata.shun.steps]]
kind = "mode"                    # 交付方式 + 目录；内嵌 `ask` 策略的
                                 # 开关（桌面快捷方式、安装范围）
[[package.metadata.shun.steps]]
kind = "scope"                   # 独立的用户/机器安装范围选择
[[package.metadata.shun.steps]]
kind = "license"                 # 许可协议（license / license-locales）
[[package.metadata.shun.steps]]
kind = "content"                 # 自定义 markdown 面板
title = "发布说明"
markdown = "notes.md"         # 相对清单文件
[[package.metadata.shun.steps]]
kind = "install"                 # 交付运行（必须恰好一个）
```

不声明 `steps` = 默认管线（模式 → 有许可则许可 → 安装），旧的
`custom-steps` 按各自 `after` 键注入；两者同时声明是配置错误，
`install` 步骤零个或多个同样是。安装范围之问与桌面快捷方式开
关出现在 `ask` 策略遇到的任何面板：内嵌于模式步骤，或独立成
步（`scope`）——由开发者自由组合。内容与许可文档在**构建期**读
取并内联进 `shun-steps.json`（`ShunConfig::resolve_steps`），运行
时安装器不携带任何文件依赖。egui 降级界面渲染完整管线（逐步轨
道、许可门槛、上一步/下一步导航）；Tauri 的 `ShellView` 把解析后
的步骤暴露给 web 前端。

## 7. 组合状态（2026-09）

| 能力 | Win | Linux | macOS | Android | iOS | 鸿蒙 |
| --- | --- | --- | --- | --- | --- | --- |
| 运行时安装 + 注册 | ✅ 用户级 + 机器级 | ✅ `.desktop` 后端（用户级） | ✅ `.app` 后端（用户级） | 暂缓 | 暂缓 | 暂缓 |
| 桌面/开始菜单快捷方式 | ✅ 两者（策略驱动） | ✅ 启动器条目 | ✅（Launchpad/Spotlight） | 无 | 无 | 无 |
| 任务栏/程序坞固定 | 仅标识（AUMID 盖章） | 仅标识（StartupWMClass） | 仅标识（LS） | 无 | 无 | 无 |
| 右键菜单 | ✅ 第一层动词 | ✅ Desktop Actions | NSServices 后续 | 暂缓 | 暂缓 | 暂缓 |
| 深链接 | ✅ 协议类 | ✅ MimeType + xdg-mime | ✅ CFBundleURLTypes | 暂缓 | 暂缓 | 暂缓 |
| 卸载叙事 | ✅ ARP + 自删 | ✅ Action 式 | ✅ LS 注销 + 文件 | 系统 | 系统 | 系统 |
| 打包产物 | ✅ 安装器 + MSIX | tarball ✅；deb/rpm 待续 | bundle ✅；DMG 待续 | 暂缓 | 暂缓 | 暂缓 |
| 签名现实 | Authenticode / 商店 | 可选 | 必须（流程工作） | keystore / Play | Apple 证书 | 华为 AGC |

**已决定暂缓（2026-09）**：Android、iOS 与鸿蒙——第 4–5 节的研究
结论仍然成立，但均不排期。

**后续排队**：

1. 经 `tauri-bundler` 出 deb/rpm 打包产物（Linux CI），DMG 走
   macOS 构建线。
2. Windows 右键菜单第二层（文件类型关联）——若有消费者需要。
3. **永不做**：任何手机 OS 上的运行时安装器/快捷方式注入；任何
   地方的任务栏/程序坞编程固定；snap（除非复杂度被证明值得）。
