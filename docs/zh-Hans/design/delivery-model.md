# 交付模型

shun 把交付拆成三个正交的部分：

## Payload

应用目录打包成 zstd 压缩的 tar，附带 SHA-256 清单（`shun-manifest.json`）。
归档可以嵌入安装器二进制（`include_bytes!`，单文件安装器模式）或作为
sidecar 携带。解压时逐条目校验清单并流式产出进度事件。

## Flow

一次交付运行是一串 `FlowEvent`——`started`、`progress { step, percent }`、
`completed`、`failed`——由壳 UI 直接渲染。安装流解压 payload、落盘清单
（供卸载消费），随后要么注册（本机模式），要么写入便携标记（便携模式）。

## Targets

- **install** —— 各平台直接注册：Windows 为每用户 ARP 条目、自拷贝
  卸载器、开始菜单/桌面快捷方式（带 AUMID 盖章）与可选的 Explorer
  右键动词；Linux 为每用户 `.desktop` 启动器 + Desktop Actions（含
  卸载入口）；macOS 为 `.app` 包补全 + Launch Services 注册。便携
  模式在任何平台都不碰系统状态。卸载按清单清除全部痕迹。
- **flash** —— 块设备写入与写后校验（镜像烧写）。后端随 evernight
  烧写器落地；trait 接口与设备枚举今天已就绪。

## WebView2（Windows）

壳自身就是 Tauri 应用，因此 WebView2 运行时是它自己 UI 的硬前提。交付
清单选择策略：要求系统运行时、内嵌 Evergreen 离线安装器，或私有携带
固定版运行时——一份副本由壳与已装应用共享，横跨安装与便携模式。

## 目录字段

安装目标行的结构是 `[文件夹徽标 | 路径输入框 | 浏览]` —— 徽标在字段
内侧左端，浏览按钮固定在行的右端（shittim-chest 文件选择器的观感），
提示语在下方。

浏览动作是一个**选择器接缝**，而非写死的对话框。该字段是 hikari
文件选择器组件的候选落点，其后端可选：

1. **浏览器原生** —— `<input type="file">` / `showDirectoryPicker()`。
   适合内容类选择（上传）；对安装目标不可用——浏览器刻意隐藏绝对
   路径，目录句柄只暴露叶子名。
2. **hikari 应用内选择器** —— 应用窗口内的统一弹层浏览器（主题、
   键盘导航、远程根）。属 hikari 后续组件工作。
3. **应用钩子** —— 由宿主提供选择器。Tauri2 应用即此场景：dialog
   插件打开的是 webview **之外**的真实系统窗口（Tauri2 没有应用内
   对话框），前两种后端都覆盖不到。shun 壳经全局 Tauri API 落到
   该后端；egui 降级壳直接调系统对话框（`rfd`）。

自动链为 钩子 → 原生 → 手动输入；组件保留三种后端可寻址，嵌入方可
强制指定其一。
