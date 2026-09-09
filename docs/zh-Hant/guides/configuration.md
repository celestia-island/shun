# 交付清單參考

交付流程宣告在應用程式自己的 `Cargo.toml` 裡，位於 `[package.metadata.shun]`
——即 cargo-deb / cargo-wix 的慣例。產品身份預設取 `[package]`（`name`、
`version`），表格內其餘欄位客製化交付流程。

```toml
[package.metadata.shun]
product = "ShunDemo"                       # 預設：套件名稱
publisher = "celestia-island"              # ARP Publisher 欄位
logo = "docs/logo.webp"                    # 殼的 logo 資產
payload = "examples/demo_payload"          # 打包進發行物的目錄
main-exe = "bin/shun-demo.exe"             # payload 內進入點

[package.metadata.shun.install]            # install target（預設）
local = true                               # 註冊安裝（ARP、解除安裝器、捷徑）
portable = true                            # 可攜模式（.shun-portable 標記，零登錄檔）
portable-marker = ".shun-portable"          # 可攜副本寫入的標記檔名（應用程式偵測自有標記時可覆寫）
desktop-shortcut = "ask"                   # always | never | ask（精靈核取方塊，預設勾選）
scope = "ask"                              # user（預設）| machine | ask
deep-links = ["shundemo"]                  # 應用程式持有的 URL scheme（myapp://…）

[[package.metadata.shun.install.verbs]]    # 右鍵選單動詞（Explorer 動詞 / 桌面動作）
key = "open-data"                          # 穩定的動詞 id
display = "Open data folder"               # 選單文字
target = "data-folder"                     # data-folder | uninstall | app
# arguments = "--safe"                     # 僅 app 目標：額外 CLI 參數

[package.metadata.shun.webview2]           # 僅 Windows
type = "skip"                              # skip | evergreen-installer | fixed-version
# path = "WebView2Runtime"                 # fixed-version 專用：解壓後的執行時目錄

[[package.metadata.shun.steps]]            # 有序的精靈管線（可選）
kind = "mode"                              # mode | scope | license | content | install
align = "center"                           # 按步驟覆寫：center | start（預設由 kind 決定）

[[package.metadata.shun.steps]]
kind = "content"
title = "Release notes"                    # content 步驟帶有標題…
markdown = "notes.md"                      # …與文件，建置期內嵌

[[package.metadata.shun.steps]]
kind = "install"                           # 恰好一個 install 步驟

[package.metadata.shun.flash]              # flash target（可選）
require-removable = true                   # 拒絕非可移除裝置
```

## 殼 UI

`[package.metadata.shun.shell]`（獨立文件中使用 `shell` 鍵）配置執行時殼：

```toml
[shell]
timeline = "left"          # top（頂部橫排）| left（左側直排）
log-level = "all"          # all（預設）| files | scripts | off
language = "auto"          # auto | en | zh-Hans | zh-Hant | ja | ko | fr | ru | es

[shell.theme]
mode = "system"            # system | light | dark
accent = [34, 211, 238]    # RGB 色頻——覆寫 --color-primary
```

## Payload 來源

`[package.metadata.shun.source]` 選擇安裝時 payload 的來源：

```toml
[source]
type = "embedded"          # payload 封存內嵌於安裝器本體
```

```toml
[source]
type = "online"            # 安裝器自行下載 payload
url = "https://github.com/<org>/<repo>/releases/latest/download/ShunDemo.shun"
```

線上安裝器以單次流水線完成 **下載 → 解壓 → 校驗**：位元組到達即按清單校驗，
進度事件同時回報下載與解壓兩個階段（多層進度）。把 `url` 指向發布源
（GitHub Releases 或任意 HTTP 主機），發布新套件即完成安裝器更新。

## 授權與自訂步驟

```toml
license = "docs/LICENSE.md"                # markdown，渲染於授權步驟

[license-locales]                          # 分語言授權覆寫
zh-Hant = "docs/LICENSE.zh-Hant.md"
ja = "docs/LICENSE.ja.md"

[[custom-steps]]                           # 注入 markdown 內容步驟
key = "whats-new"
after = "license"
title = "What's New"
markdown = "docs/whats-new.md"
```

介面內建八種語言的預設文案（`en`、`zh-Hans`、`zh-Hant`、`ja`、`ko`、`fr`、
`ru`、`es`）；`shell.language = "auto"` 跟隨系統，固定語言可直接指定，
分語言授權覆寫保證本地化協議正常運作。
