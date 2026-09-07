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

- **install** —— NSIS 式注册（用户级 ARP 条目、自拷贝卸载器、开始菜单
  快捷方式、深链），外加零注册表的便携模式。卸载按清单移除全部痕迹。
- **flash** —— 块设备写入与写后校验（镜像烧写）。后端随 evernight
  烧写器落地；trait 接口与设备枚举今天已就绪。

## WebView2（Windows）

壳自身就是 Tauri 应用，因此 WebView2 运行时是它自己 UI 的硬前提。交付
清单选择策略：要求系统运行时、内嵌 Evergreen 离线安装器，或私有携带
固定版运行时——一份副本由壳与已装应用共享，横跨安装与便携模式。
