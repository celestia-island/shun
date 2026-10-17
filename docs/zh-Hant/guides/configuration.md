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

[[package.metadata.shun.attachments]]
key = "models"
title = "2D/3D model pack"
dest = "models"
[package.metadata.shun.attachments.online]
url = "https://example.test/models.shun"   # 可選附件資源（精簡版在安裝時下載）

[package.metadata.shun.install]            # install target（預設）
local = true                               # 註冊安裝（ARP、解除安裝器、捷徑）
portable = true                            # 可攜模式（.shun-portable 標記，零登錄檔）
portable-marker = ".shun-portable"          # 可攜副本寫入的標記檔名（應用程式偵測自有標記時可覆寫）
desktop-shortcut = "ask"                   # always | never | ask（精靈核取方塊，預設勾選；無頭執行按捷徑是否存在解析 ask）
start-menu-shortcut = "always"             # always | never | ask（預設 always，精靈只詢問桌面那份）
scope = "ask"                              # user（預設）| machine | ask
deep-links = ["shundemo"]                  # 應用程式持有的 URL scheme（myapp://…）
root-dir-folder = "ShunDemo"              # 裸磁碟根目錄下自動墊的資料夾（D:\ → D:\ShunDemo；預設取產品名）

[[package.metadata.shun.install.verbs]]    # 右鍵選單動詞（Explorer 動詞 / 桌面動作）
key = "open-data"                          # 穩定的動詞 id
display = "Open data folder"               # 選單文字
target = "data-folder"                     # data-folder | uninstall | app
# arguments = "--safe"                     # 僅 app 目標：額外 CLI 參數

[package.metadata.shun.webview2]           # 僅 Windows
type = "skip"                              # skip | evergreen-installer | fixed-version
# path = "WebView2Runtime"                 # fixed-version 專用：解壓後的執行時目錄
# silent-install = true                    # evergreen-installer：無執行階段的機器上執行
                                           #   自帶的安裝器（預設開啟——精靈先詢問，
                                           #   headless 靜默執行）
# download-url = "https://…"               # evergreen-installer：不攜帶安裝器時，安裝期透過網路取得
                                           #   Evergreen 安裝器（與自帶安裝器同一同意流程）
# warn-missing = true                      # skip / evergreen-installer：egui 降級介面在開始
                                           #   與結束時警告並附微軟官方下載頁連結（預設開啟）

[[package.metadata.shun.steps]]            # 有序的精靈管線（可選）
columns = 2              # 可選：模式網格欄數；預設每個模式一欄
kind = "mode"                              # mode | scope | license | content | pairing | install
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

## 欄位

| 欄位 | 型別 | 預設 | 含義 |
| --- | --- | --- | --- |
| `product` | string | 套件 `name` | 標題、ARP 顯示名稱、燒錄標籤 |
| `publisher` | string | — | ARP Publisher 欄位 |
| `logo` | path | — | 殼的 logo 資產（相對清單檔案） |
| `payload` | path | — | 打包進發行物的目錄 |
| `main-exe` | path | — | payload 內進入點（捷徑目標） |
| `install` | table | 雙模式全開 | `local` / `portable` 開關、`portable-marker` 標記檔名、`desktop-shortcut` / `start-menu-shortcut` / `launch-after-install` 策略、`default-dir`（環境變數展開的預設位置覆寫）、`verbs`、`deep-links`、`aumid`、`icon`、`root-dir-folder`（裸磁碟根目錄下墊的資料夾，預設取產品名） |
| `attachments` | 表格陣列 | 無 | 選用附件資源（資產包）：`key` / `title` / `dest` / `online.url`；精簡版在安裝時下載 |
| `update` | table | 無 | 更新監視：`sources`（鏡像根 URL，按順序探測）+ `files`，解析到第一個可達源之下 |
| `webview2` | table | `skip` | Windows 執行階段策略 |
| `steps` | 表格陣列 | 預設管線 | 有序精靈管線（`mode`/`scope`/`license`/`content`/`pairing`/`install`，自由排序；恰好一個 `install`） |
| `script` | table | 無 | payload 指令碼鉤子：`runner` + `hooks[]`（`phase`/`script`）——見指令碼設計筆記 |
| `variant` | 表格陣列 | 無 | 由 `shun build --variant` 選取的命名建置設定 |
| `signing` | table | 不簽署 | 套用於建置產物的 Authenticode / codesign 設定 |
| `license-sysl` | table | 無 | 從 SySL 儲存庫打包授權與官方譯文 |
| `msix.logo-background` | 顏色 | `transparent` | 透明 MSIX 圖示底下的底板色 |
| `flash` | table | — | 宣告燒錄目標 |

## 捷徑策略

`install.desktop-shortcut` 與 `install.start-menu-shortcut` 按軌道解析。
精靈各面顯示核取方塊（預設勾選——NSIS 慣例）；無頭軌道（`--silent`）按
**存在性**解析 `ask`：已存在的捷徑原位重新整理，缺失的不憑空建立。靜默
更新絕不能撒出使用者沒有要過的圖示——AV/EDR 啟發式盯的正是這種行為，
而且使用者刪掉的捷徑必須保持刪除。`always` / `never` 在所有軌道固定
取值；明示 `--shortcut-desktop` / `--shortcut-menu` 旗標（更新器契約）
壓過一切。解除安裝無條件移除捷徑。

## 安裝後啟動

`install.launch-after-install` 決定完成頁的慣例：`ask`（預設）跟隨
精靈的核取方塊——即 NSIS 的「安裝完成後立即啟動」開關，無頭執行
視為勾選；沒有該選項的殼可用 `always` / `never` 固定取值。

安裝流程本身從不啟動任何東西，由殼在安裝成功後呼叫
`shun::targets::install::launch`：它在安裝目錄內以游離方式啟動設定
的 `main-exe`（不等待，安裝器行程隨即可以退出）；當
`InstallContext::launch_after_install` 解析為 false、或未宣告進入
點時，呼叫是空操作。進入點已宣告但檔案缺失時，照常回傳
`MissingEntry` 錯誤。

## 根磁碟保護

裸檔案系統根目錄目標——選中的磁碟（如 `D:\`，`D:`、UNC 共享根
`\\server\share`、POSIX 的 `/` 同理）——不會直接接收載荷：
`InstallContext::apply_config` 會在其下自動墊一層資料夾，預設取
產品名，可用 `install.root-dir-folder` 自訂。精靈視窗在選取或
輸入根目錄的當下即改寫路徑框，顯示的永遠是真實目標；無介面的
`--dir=D:\` 執行同樣在流程內受此保護。

## WebView2 策略

| `type` | 攜帶物 | 前提 | 說明 |
| --- | --- | --- | --- |
| `skip` | 無 | 系統 WebView2 | 標準發行物 |
| `evergreen-installer` | Evergreen 離線安裝器（約 127 MB） | 安裝時提權 | 註冊系統級執行階段 |
| `fixed-version` | 解壓後的執行階段目錄 | 無 | 私有副本由殼與已裝應用共享，橫跨安裝與可攜模式 |

實測（2026-09，v151 x64）：fixed-version 執行階段**壓縮約 293 MB /
解壓約 661 MB**——已接受的成本，與其他打包器相當。發行物只內嵌
**一份**副本：殼把 payload 的執行階段子樹暫存到
`%LOCALAPPDATA%\shun\<product>\webview2` 並把
`WEBVIEW2_BROWSER_EXECUTABLE_FOLDER` 指向那裡以完成自舉；解壓感知
雜湊，重新安裝與既有檔案會被沿用而非重寫（「Reusing」進度事件）
——同一引擎不會出現第二份。

### 無執行階段機器的降級

面階梯永遠不會因缺少執行階段而死亡——只會降級。階梯解析前，殼用
載入器自身的判定（`wry::webview_version()`，正是 tauri 建立
webview 前的同一道門禁）探測執行階段，登錄檔殘值再也無法把
webview 面送進 tauri 那個英文 "Could not find the WebView2
Runtime" 彈框：

- `evergreen-installer` + `silent-install`（預設 `true`）：執行階段
  的取得位於安裝步驟——初始化只解碼並暫存自帶的安裝器（按
  payload 的 `webview2/` 前綴發現）到使用者級快取，面階梯解析前
  不執行任何安裝。精靈執行會先停在同意詢問（「安裝執行階段 /
  跳過，僅安裝主程式」），僅在同意後才可見地執行取得——安裝器以
  自身視窗執行，其清單要求提權時走 UAC。無頭軌道以
  `silent-install` 作為同意，以 `/silent /install` 靜默執行。取得
  成功即原地升級機器——穩定後載入器即回報執行階段可用。
- 不攜帶安裝器但宣告了 `download-url` 的建置，會在安裝步驟的同一
  取得流程中、經同一同意之後從該位址連網取得 Evergreen 安裝器
  （無頭執行仍由 `silent-install` 把關）——lite 版透過網路而非
  自帶 payload 升級機器。慢網或斷網永遠不會阻塞首個介面：取得
  發生在安裝流程內，降級橫幅也會說明執行階段即將到來。
- 無論結果如何，機器仍無執行階段則落到 egui 面。`warn-missing`
  （預設 `true`）且未攜帶安裝器時，該面在精靈開始與結束處各警告
  一次，並內嵌跳轉到微軟官方 WebView2 下載頁的連結。兩個開關
  均可按產品關閉：

```toml
[package.metadata.shun.webview2]
type = "evergreen-installer"
silent-install = false                  # 永不自動執行自帶的安裝器
warn-missing = false                    # 靜默降級，不帶警告橫幅
```

一個值得了解的上游限制：tauri 的執行階段門禁不讀取
`WEBVIEW2_BROWSER_EXECUTABLE_FOLDER`，因此「僅 fixed-version、未安裝
Evergreen」的機器即便本可建立 webview，也會落到 egui 面——殼優先
選擇一定能渲染的面。

## MSIX 圖示底板

Windows 會在每個忽略 `BackgroundColor="transparent"` 的表面上，把
打包桌面應用的圖示墊在預設系統藍（`#0078D7`）上——App Installer
對話框就是其中之一。`msix.logo-background` 接受 `#RRGGBB` 顏色：
shun CLI 把透明圖示合成到該色上作為套件資產，並把它宣告為
`BackgroundColor`，品牌由此掌控每個表面的底板色：

```toml
[package.metadata.shun.msix]
# identity-name / publisher / display-name / executable ...
logo-background = "#0F172A"
```

## 離線殼

安裝殼除了 hikari webview UI 之外，還內嵌一個 egui 離線 UI。它由
同一份清單與 payload 驅動（一份流程，三個渲染器），不渲染特效，
並在 Windows 缺少 WebView2 時被選中（橫幅會明確說明缺失的環境）
或手動強制：

```bash
shun-installer --no-webview   # 同一精靈，離線渲染器（--fallback 為舊拼寫）
```

兩個 UI 都支援離線截圖——視窗內容以 `PrintWindow` 擷取，無需桌面
自動化，存檔後行程即退出：

```bash
shun-installer --screenshot=ui.png                 # hikari（webview）UI
shun-installer --no-webview --screenshot=ui.png    # egui 離線 UI
```

## 殼 UI

`[package.metadata.shun.shell]`（獨立文件中使用 `shell` 鍵）配置執行時殼：

```toml
[shell]
timeline = "left"          # top（頂部橫排）| left（左側直排）
log-level = "all"          # all（預設）| files | scripts | off
log-order = "newest"      # newest（預設，最新在頂）| oldest（追加在尾部）
language = "auto"          # auto | en | zh-Hans | zh-Hant | ja | ko | fr | ru | es | de | pt
faces = ["webview", "egui", "tui"]  # 本產品殼可呈現的介面（預設三項全開）

[shell.theme]
mode = "system"            # system | light | dark
accent = [34, 211, 238]    # RGB 色頻——覆寫 --color-primary
```

`shell.faces` 限定本產品殼可呈現的介面——`webview` / `egui` /
`tui`；缺省即三項全開。headless CLI 面（`--silent`、ARP 解除安裝
字串）是基底，無法停用。請求被停用的介面是錯誤；自動選擇會跳過
被停用的面。

### 精靈語言

精靈會在**第一步**詢問語言：選擇器列出全部十個支援的語言，每項都以
自身語言標注（English、简体中文、日本語……）。切換後 UI 文案與授權
文件立即跟著切換——授權文件按 `license-locales` / `locale-paths`
跟隨所選語言——而且該選擇會記入每使用者偏好檔，下次執行自動沿用
（`<本機應用程式資料目錄>/<產品>/installer-prefs.json`，例如
`%LOCALAPPDATA%\ShunDemo\installer-prefs.json`，內容形如
`{ "language": "zh-Hans" }`）。可攜執行只把選擇保存在記憶體中——
可攜副本不寫任何系統狀態。`shell.language` 仍是設定層面的固定項，
但記住的選擇優先於它，其後才是系統語言。

所選語言還會隨安裝流程傳遞：payload 指令碼透過 `SHUN_LANGUAGE`
環境變數取得它，同時它也被寫入磁碟上的安裝清單（修復/更新等後續
流程可以讀取）。要往已安裝應用程式自身的設定裡寫什麼由 payload 的
指令碼決定——shun 只匯出事實。

## 配對步驟

`pairing` 步驟為本機申領憑證——反向通道：本機顯示驗證碼，操作員在
控制面板裡輸入它。一個窗格，兩條通道：

```toml
[[package.metadata.shun.steps]]
kind = "pairing"

[package.metadata.shun.steps.pairing.source]
kind = "gateway"                     # gateway | scripts
official = "https://gateway.example.test"   # 預選的閘道根 URL
# allow-custom = true                # 同時提供自訂閘道輸入（預設開啟）

[package.metadata.shun.steps.pairing.identity]
node-id = true                       # 閘道協定以此為主鍵
name = false                         # 可選裝置名（按主機名預填）
# tier = 1                           # 申領時賦予的層級（產品固定時宣告）

# env-file = "pairing.env"           # gateway 通道：申領到的憑證寫入安裝目錄內
                                     #   的位置（.env 風格）
```

- `gateway` —— 內建 JSON-RPC 用戶端（`pairing.request` 鑄碼、
  `pairing.await` 長輪詢、`pairing.accept` 交付憑證）。目標機器無需
  任何指令碼執行環境；該通道要求 `node-id` 與 `env-file`。
- `scripts` —— 產品自有啟用：`request` / `await` / `record` 三份
  payload 相對路徑的 Python 指令碼（`python3`，宿主機只有無版本號
  名字時回退 `python`）。窗格答案以單一 JSON 物件從指令碼 **stdin**
  傳入——絕不走 argv；每個階段的 stdout 必須恰好一個 JSON 值；
  `record` 持久化申領結果（`env-file` 會隨設定攜帶但不被消費）。
  每個階段都有時限：掛死的指令碼會被終結，而不是乾等。

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

## 更新監視

`[package.metadata.shun.update]`（獨立文件中使用 `update` 鍵）宣告
殼探測更新用的鏡像源，以及解析到鏡像之下的檔案：

```toml
[update]
sources = [                                # 鏡像根 URL，按順序嘗試
  "https://mirror.example.test/shun-demo",
  "https://releases.example.test/shun-demo",
]
files = ["latest", "app-setup.exe"]
```

執行時殼呼叫 `shun::update::resolve`：對每個源做一次短探測（GET
第一個檔案，10 秒逾時），第一個應答 `200` 的源贏得整輪解析，宣告
的每個檔案都解析到獲勝源之下的 URL（`<source>/<file>`）。不可達
或非 200 的源以警告記錄跳過；全部無應答則什麼都不解析。隨後殼自行
拉取解析出的 URL——`latest` 版本標記以文字讀取（`fetch_text`），
產物下載走線上 payload 流水線（`OnlinePayload`：下載 → 解壓 →
校驗）。

## 建置變體

`[[package.metadata.shun.variant]]`（單數——表格陣列慣用法）宣告
`shun build --variant <name>` 旗標選取的命名產物設定——安裝器矩陣
的各條軸（架構與系統來自 `--target`，資源包跟隨 payload 目錄）：

```toml
[[package.metadata.shun.variant]]
name = "lite"                        # id + 產物名後綴
payload = "payload-lite"             # payload 目錄覆寫
faces = ["egui", "tui"]              # 該變體殼的介面覆寫

[package.metadata.shun.variant.env]  # 傳遞給殼的 cargo 建置
SHUN_FLAVOR = "lite"                 # （及其 build.rs）——風味標記
```

套用變體會替換 payload 目錄與殼的介面清單；`env` 隨殼建置傳遞，
風味可藉此嵌入標記。未知名稱是錯誤。

## 程式碼簽署

`[package.metadata.shun.signing]` 設定建置產物的簽署——`shun build`
在流程末尾為安裝器簽署，`shun sign` 為獨立檔案簽署（均可
`--no-sign` 跳過）。缺省即不簽署，本機測試無妨：

```toml
[package.metadata.shun.signing.windows]   # 經 signtool 的 Authenticode
enabled = true
thumbprint = "<憑證指紋>"                 # 從使用者/機器憑證庫選取（/sha1）
# timestamp-url = "http://timestamp.digicert.com"   # RFC 3161（/tr，預設值）

[package.metadata.shun.signing.macos]     # codesign
enabled = true
identity = "<簽署身分>"
```

## 授權與自訂步驟

```toml
license = "docs/LICENSE.md"                # markdown，渲染於授權步驟

[license-locales]                          # 分語言授權覆寫
zh-Hant = "docs/LICENSE.zh-Hant.md"
ja = "docs/LICENSE.ja.md"

[[licenses]]                               # 追加授權文件
title = "Copyright notice"                 # 可選：正文上方的標題
path = "NOTICE.md"                         # markdown，相對清單檔案
[licenses.locale-paths]                    # 該文件的分語言覆寫
zh-Hant = "NOTICE.zh-Hant.md"

[[custom-steps]]                           # 注入 markdown 內容步驟
key = "whats-new"
after = "license"
title = "What's New"
markdown = "docs/whats-new.md"
```

`license` + `license-locales` 是單文件簡寫；表格陣列 `licenses` 宣告
更多授權文件，每份可帶可選 `title` 與自己的 `locale-paths`。兩者可以
同時使用：簡寫文件排在最前，陣列按宣告順序依次排列。符合目前語言的
路徑（`license-locales` 或 `locale-paths`）優先於基礎文件。授權步驟
一次顯示一份文件，多份時以「上一份/下一份」翻頁；唯一的同意核取方塊
涵蓋全部文件。解析後的 JSON 中每份文件存於 `licenses`（title + body），
而舊的 `body` 字串把所有正文用分隔線串接，只讀 `body` 的渲染器也能
完整呈現整份協議。

介面內建十種語言的預設文案（`en`、`zh-Hans`、`zh-Hant`、`ja`、`ko`、`fr`、
`ru`、`es`、`de`、`pt`）；`shell.language = "auto"` 跟隨系統，固定語言可直接指定，
分語言授權覆寫保證本地化協議正常運作。

## 從 SySL 儲存庫取授權

`[package.metadata.shun.license-sysl]` 建置期從授權自己的儲存庫打包
Synthetic Source License 及其官方譯文（需 `online` feature）——
精靈無需在儲存庫裡存放授權檔案即可按使用者語言展示協議：

```toml
[package.metadata.shun.license-sysl]
repo = "celestia-island/sysl"        # 預設
branch = "main"                      # 預設
locales = ["zh-Hans", "ja"]          # 要打包的譯文（根目錄英文始終打包）
```

語言標籤映射到 sysl 儲存庫的 i18n 目錄（`zh-Hans` → `zhs`、
`zh-Hant` → `zht`；`ja`/`ko`/`fr`/`ru`/`es`/`de`/`pt`/`ar` 直接
對應）。打包出的文件像其他授權文件一樣渲染在授權步驟。

## 指令碼鉤子

`[package.metadata.shun.script]` 宣告 payload 相對路徑的執行器
指令碼，交付流程在階段邊界執行它們。每產品一個執行器——目前只
掛載 `duckscript`（內嵌直譯器）：

```toml
[package.metadata.shun.script]
runner = "duckscript"

[[package.metadata.shun.script.hooks]]
phase = "post-install"               # prepare | post-install | pre-uninstall
script = "scripts/post_install.ds"   # payload 相對路徑，隨 payload 打包
```

失敗的鉤子會中斷整個流程。完整契約——階段、鉤子可見的環境（含
`SHUN_LANGUAGE`）與示例——見[指令碼設計筆記](../design/scripting.md)。
