# 快速开始

shun 分为两半：**构建侧**（打包 payload、解析交付清单）和**运行侧**（驱动流程的壳）。

## 试试 demo

```bash
cargo run --example demo_flash                        # 枚举可烧写设备
cargo run --example demo_install                      # 生成 ShunDemo.shun + 本机安装
cargo run --example demo_install -- --portable        # 便携安装（不写注册表）
cargo run --example demo_install -- --uninstall       # 卸载（清干净全部痕迹）
```

`demo_install` 生成安装包 `ShunDemo.shun`，以流式进度解压，并在本机模式下执行
NSIS 式注册：用户级 ARP 条目（设置 → 应用）、开始菜单快捷方式、自拷贝卸载器。
便携模式只写 `.shun-portable` 标记，绝不触碰注册表。

## 运行 demo 壳

```bash
pnpm --dir shell/web install
cargo run -p shun_demo_shell
```

壳在构建期嵌入 demo payload（单文件安装器模式），并渲染
`shell/Cargo.toml` → `[package.metadata.shun]` 声明的交付模式。
