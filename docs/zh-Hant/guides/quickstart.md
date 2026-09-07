# 快速開始

shun 分為兩半：**構建側**（打包 payload、解析交付清單）和**運行側**（驅動流程的殼）。

## 試試 demo

```bash
cargo run --example demo_flash                        # 列舉可燒錄裝置
cargo run --example demo_install                      # 產生 ShunDemo.shun + 本機安裝
cargo run --example demo_install -- --portable        # 可攜安裝（不寫登錄檔）
cargo run --example demo_install -- --uninstall       # 解除安裝（清除全部痕跡）
```

`demo_install` 產生安裝套件 `ShunDemo.shun`，以串流進度解壓，並在本機模式下執行
NSIS 式註冊：使用者級 ARP 項目（設定 → 應用程式）、開始功能表捷徑、自複製解除安裝器。
可攜模式只寫 `.shun-portable` 標記，絕不觸碰登錄檔。

## 執行 demo 殼

```bash
pnpm --dir shell/web install
cargo run -p shun_demo_shell
```

殼在建置期嵌入 demo payload（單檔安裝器模式），並渲染
`shell/Cargo.toml` → `[package.metadata.shun]` 宣告的交付模式。
