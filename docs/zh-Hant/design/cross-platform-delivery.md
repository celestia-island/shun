# 設計筆記：跨平台交付

狀態：**Windows 面已驗證且缺口已補齊（2026-09）；Linux 與 macOS
的執行時註冊後端已實作；行動端（Android/iOS）與鴻蒙已完成預研——
暫緩，不做。** 本筆記記錄：(a) 對自動打包註冊面的一次集中測試在
Windows 後端上證明了什麼；(b) 曾經缺失的桌面捷徑 / 工作列識別 /
右鍵選單面——現已實作，以及塑造它們的真機安全性原則發現；(c) Linux
與 macOS 後端；(d) Android、iOS 與鴻蒙的可行性結論（暫緩）。

(a) 的可執行清單落在 `tests/registration_shortcuts.rs` 與
`tests/msix_pack.rs`。

## 1. 集中 Windows 測試證明了什麼（2026-09，真機）

驗證方式是在一台 Windows 11 真機上執行真實的 `InstallFlow`，再讓
Windows 自己讀回來（WScript.Shell COM 解析器、登錄檔、Windows SDK
的 MakeAppx）——而不是檢查我們自己的輸出：

| 面 | 結果 |
| --- | --- |
| 開始功能表 `.lnk` 能被 shell 解析 | **通過** —— `TargetPath` 與 `WorkingDirectory` 與安裝上下文宣告完全一致；`mslnk` 手工建構的合成 PIDL 可被正確解析 |
| 非 ASCII 安裝路徑（中文目錄） | **通過** —— `順測試目錄` 安裝目錄經 COM 解析器逐位元組回讀一致 |
| `.lnk` 二進位對照 MS-SHLLINK | **通過** —— 標頭、CLSID `{00021401-…}`、旗標位（target ID list + relative path + working dir + unicode）、無快速鍵 |
| ARP 條目（HKCU） | **通過** —— 完整 NSIS 等價欄位集：DisplayName/Version/Publisher/InstallLocation/DisplayIcon，加 `UninstallString`/`ModifyPath`/`RepairString`（均帶引號，各自經 `/uninstall` 開啟解除安裝器介面）與 `EstimatedSize` 為 DWORD；EstimatedSize 與 payload 清單總量一致 |
| 解除安裝清理 | **通過** —— ARP 機碼、捷徑、payload、解除安裝器、目錄全部移除（`tests/install_local.rs` 覆蓋，此處複核） |
| MSIX 清單產生 | **通過** —— 識別、四段補零版本號、XML 逸出字串、正斜線進入點、runFullTrust |
| MSIX 真實打包（MakeAppx 10.0.26100） | **通過** —— 合法 OPC zip，含 `[Content_Types].xml` + `AppxManifest.xml`；產出的 `dist/shundemo-0.1.0-x64.msix` **無簽章區塊**（符合設計：商店散佈代簽，或需信任自簽憑證） |

## 2. Windows 補缺落地（2026-09 實作）

以下各項在驗證輪之後全部落地，每個面都在真機上由註冊測試套件
驅動（`tests/registration_shortcuts.rs`）。

### 桌面捷徑 —— `install.desktop-shortcut`

由原則（`always` | `never` | `ask`，NSIS 核取方塊慣例——egui 精靈為
`ask` 顯示預設勾選的開關，無頭執行按勾選處理）解析後，與開始功能表
捷徑一同寫入。桌面用 **`SHGetKnownFolderPath(FOLDERID_Desktop)`**
解析——絕不用 `%USERPROFILE%\Desktop`，桌面被重新導向（OneDrive、
網域原則）時後者是錯的。解除安裝時無條件移除（安裝與解除安裝之間
配置可能已變化）。

**真機實測**：安全性原則（防毒軟體/EDR 的假捷徑與勒索防護）通常會
**專門攔截在桌面建立 `.lnk`**——驗證機上連提權 shell 的
`echo x > Desktop\probe.lnk` 都被拒，而 `.tmp` 隨便寫。因此桌面
捷徑是**盡力而為**：寫入被拒時降級為警告，絕不使安裝失敗（開始
功能表捷徑與 ARP 才是關鍵面）。測試套件會偵測機器原則，對正常
路徑與優雅降級路徑分別斷言。

### 工作列識別 —— `install.aumid`

程式化工作列釘選依舊**被平台設計封鎖**（無受支援的 API；釘選 hack
已在 Win10 移除）。已落地的是識別的那一半：每個捷徑都透過 Shell
COM 屬性存放區（`IShellLink` → `IPersistFile` → `IPropertyStore`，見
`src/targets/aumid.rs`——`mslnk` 只寫位元組）蓋上
**`System.AppUserModel.ID`**，讓工作列分組、Jump List 與*使用者發起*
的釘選表現正確。預設 AUMID 由 `{publisher}.{product}` 產生；
`install.aumid` 可覆寫，應用程式應把同一值傳給
`SetCurrentProcessExplicitAppUserModelID`。MSIX 安裝經包識別免費獲
得識別。蓋章同樣盡力而為（出於同一原則原因：驗證機拒絕
`IPropertyStore::SetValue` 寫 `.lnk`——0x80030005——那裡降級為警
告）。

### 右鍵選單動詞 —— `[[install.verbs]]`

第一層已交付：每使用者 Explorer 動詞，位於
`HKCU\Software\Classes\Applications\<exe>\shell\<verb>\command`——
即有文件記載的 Application Registration 面，免提權，出現在應用程式
的 exe 及其捷徑上。三種動詞目標在所有實作它們的平台上映射為命令
列：`data-folder`（開啟安裝目錄）、`uninstall`（執行解除安裝器）、
`app`（進入點 + 參數）。解除安裝只刪自己建立的動詞機碼，隨後僅在
空時刪 `shell`/`Applications` 容器（別人註冊的動詞得以倖存）。第二層
（檔案類型關聯）與第三層（MSIX `FileExplorerExtension`）仍為後續。

### 深層連結 —— `install.deep-links`

交付模型初稿就承諾過的能力，現在三個後端全部落地、全部每使用者：
Windows 把每個 scheme 註冊為 `HKCU\Software\Classes\<scheme>` 協定類
別（空的 `URL Protocol` 標記 + 以 `%1` 接收 URL 的開啟命令）；Linux 在
啟動器上宣告 `MimeType=x-scheme-handler/<scheme>;` 並用 `xdg-mime`
認領預設；macOS 合成的 `Info.plist` 攜帶 `CFBundleURLTypes`。scheme
統一正規化為小寫 `[a-z0-9+.-]`（`"MyApp://"` → `myapp`）。解除安裝刪
除 Windows 協定類別；移除 Linux 啟動器即孤立該 handler
（`mimeapps.list` 行失效——已知並接受）。

### 為什麼安裝程式不請求 UAC 提權

桌面捷徑的發現引出了這個問題，答案有三條：

1. **shun 寫的全部是每使用者面** —— HKCU、使用者的開始功能表與桌面、
   `%LOCALAPPDATA%`。沒有任何一處需要提權權杖，加 UAC 彈出視窗買
   不到任何東西，只添上最壞的那種提示噪音（訓練使用者無腦點
   「是」）。這與 wowsp NSIS 範本、VS Code / Chrome「使用者安裝程
   式」做的是同一個取捨。
2. **提權也修不了 `.lnk` 被攔截**：攔截是安全性產品按「桌面目錄 +
   `.lnk` 副檔名」下刀的檔案系統過濾器——過濾器按原則攔截處理程
   序，不看 ACL，提權處理程序同樣被攔（Windows 受控資料夾存取同
   理：除非應用程式被列入白名單，管理員也被攔）。正確的應對就是
   已交付的：降級為警告，保住開始功能表捷徑與 ARP。
3. **提權後寫「使用者面」是正確性陷阱**：提權處理程序解析的使用者
   設定檔可能不同（管理員帳戶的桌面、`%APPDATA%`、登錄檔 hive 都
   可能不是安裝使用者的）——NSIS 全使用者捷徑的經典 bug 來源。真
   正需要提權的步驟讓**步驟自身**提權：WebView2 Evergreen 引導器自
   帶 `requireAdministrator` 清單，shell 保持 `asInvoker` 並委派。

機器級範圍（Program Files、HKLM ARP、全使用者捷徑）是某些產品真正
需要的**模式**——它已按此落地：刻意的可選（`install.scope`），
永不預設。見第 6 節。

### 健全性發現——均已修復

- 產品名含檔名非法字元（`/\:*?"<>|`、結尾的點/空格）時，所有
  檔案系統與登錄檔面（`.lnk` 名、ARP 機碼路徑）統一做**字詞淨化**
  ——產品名裡的 `\` 不再巢狀出登錄檔子機碼。
- ARP 的 `UninstallString` 傳 `/uninstall`，而安裝 shell 的無頭
  解析只認 `--uninstall`——在 Windows 設定裡點「解除安裝」實際會開
  啟精靈而不是解除安裝。兩種拼寫現在都接受。

## 3. Linux 與 macOS（執行時後端已實作；打包產物待續）

**兩者都可行，且都與 Windows 共用同一條硬限制：任何平台的程式化
工作列/Dock 釘選都不存在。** 執行時 `Registration` 後端已落地；
建置側的打包產物（經 `tauri-bundler` 出 deb/rpm、DMG）因需要各自的
原生建置宿主，仍為後續工作。

### Linux —— `LinuxRegistration`（src/targets/freedesktop.rs）

Windows 後端做的每件事都映射到 freedesktop 慣例，全部每使用者
（`~/.local/share/...`）、免提權：

- **啟動器註冊** = 寫 `<product>.desktop`（Name、Exec、Icon 來自
  `install.icon`、Categories，以及關鍵的 **`StartupWMClass`** = 進入
  點可執行檔名稱——讓使用者發起的工作列/Dock 釘選歸組到正確圖示的
  欄位）到 `~/.local/share/applications`，再對它跑
  `update-desktop-database`（工具缺席時跳過是安全的——桌面環境會
  惰性重新掃描）；
- **右鍵動詞 + 解除安裝入口** = `Actions=` + `[Desktop Action <id>]` 組
  ——永遠包含一個 **Uninstall** 動作，因為 GNOME Software / KDE
  Discover 只列出各自套件後端追蹤的應用程式：shun 安裝的應用程式
  永遠不會出現在那裡。三種動詞目標映射為 `xdg-open`、解除安裝器、
  進入點 + 參數；
- **可執行位元恢復** —— payload 封存對每個條目都是 0644，後端把
  進入點與拷貝出的解除安裝器 chmod 回 0755；
- **註銷** = 刪 `.desktop` + 重新整理資料庫。

`.desktop` 寫入器是純資料管道，在每個平台編譯（並被單元測試）；
只有處理程序呼叫那一半是 Linux 門控的。工作列/Dock**釘選依舊不可
能**（無跨桌面 API：GNOME 我的最愛是內部 gsettings 機碼、KDE 釘選
項在未有文件記載的 appletsrc 裡；視為使用者動作）。檔案管理器右鍵
選單（Nautilus 腳本 / Dolphin service menus）仍不在範圍內。

**打包格式**（仍為建置側、後續）：**tarball/可攜（shun 已有）+ deb
（cargo-deb）+ rpm（cargo-generate-rpm）** 是最優子集——恰好是
`tauri-bundler` 的輸出集（它是可為非 Tauri payload 重用的函式庫，
[cargo-packager] 亦然）。AppImage = 中等；Flatpak = 中高；snap =
高，緩做。誠實的「全發行版皆可執行」限制：glibc 只向前相容，且
**musl 靜態建置帶不動 WebView 應用程式**（webkit2gtk 拖著整個 GTK C
堆疊）——交付 shell 可以做 musl 靜態，但被交付的 Tauri 應用程式必
須按支援的最老 webkit2gtk-4.1 基線建置（Ubuntu 22.04 / Debian
12 / Fedora 37+ 時代）。

### macOS —— `MacOSRegistration`（src/targets/macos.rs）

- **註冊** = 定位進入點可執行檔所在的 `.app` 包（最近的 `.app`
  祖先）；payload 沒帶 `Info.plist` 時合成最小的一份（`plist.rs`，
  純函式、全平台單元測試）；恢復進入點的可執行位元；**遞迴剝離
  繼承來的 `com.apple.quarantine`**（瀏覽器給安裝器蓋了隔離戳，
  而 macOS 的拷貝保留 xattr——不剝離的話交付出的應用程式會繼承
  使用者已經答覆過的 Gatekeeper 攔截）；然後 `lsregister -f` 該
  包——Spotlight 與 Launchpad 隨之跟進。註銷 = `lsregister -u`
  （檔案由通用解除安裝流程刪除）。裸可執行 payload（無 `.app`）
  註冊為空操作——按可攜慣例；
- **Dock 釘選**：**無受支援的 API**（`defaults write com.apple.dock`
  + `killall Dock` 的 hack 會踐踏使用者偏好且在新版 macOS 上不可
  靠）——Launchpad/Spotlight 的存在感（LS 註冊）即等價物；
- **簽署對真實散佈仍是必須的流程工作**：Developer ID + hardened
  runtime + `notarytool` + staple；且下載來的 shell **會被轉置**執
  行——自身路徑假設要相應處理；
- **打包產物**（後續）：DMG 走 tauri-bundler / `hdiutil`，`.pkg`
  僅用於管理員流程，Homebrew cask 是管道。發佈 **universal2**（雙
  重建置 + `lipo` + 重新簽署）。

[cargo-deb]: https://github.com/kornelski/cargo-deb
[cargo-generate-rpm]: https://crates.io/crates/cargo-generate-rpm
[cargo-packager]: https://github.com/crabnebula-dev/cargo-packager

## 4. Android 與 iOS（預研）

**先重構認知：行動端的「安裝」是平台所有、簽章驗證的行為。不存在
把應用程式目錄的 zstd tar 串流寫到使用者自選位置的等價物。** shun
在行動端的誠實角色是**建置/打包/簽署管線**（「行動端的
cargo-dist」），不是安裝器執行時。

### Android

- **打包機制**：APK = 簽章 zip（DEX + 資源 + 每 ABI `.so`）；
  管線 `aapt2` → `d8`/`r8` → 打包 → `zipalign` → `apksigner`。
  AAB 是 Play 發佈格式（新應用程式強制）；側載需要具體 APK
  （`bundletool build-apks` + 重新簽署）。Rust 目標為帶宿主工具的
  Tier 2。
- **2025–2026 仍在維護的工具**：Tauri 2 的 `tauri android build`
  （包裝 cargo-mobile2 + Gradle；keystore 走 `keystore.properties`）、
  **cargo-ndk**（維護中）、**`apk` crate**（無 Gradle 的
  aapt2/d8/zipalign/apksigner）。**xbuild 與 cargo-apk 已死/休眠**
  ——不要建立在它們之上。Tauri 2 行動端官方已穩定；Android 側比
  iOS 側成熟。
- **執行時安裝器：不可行/無意義。** OS 套件管理員從簽章 APK 安裝並
  強制使用者確認（僅裝置擁有者/MDM 靜默）；**APK 本身就是安裝器**。
  Play 政策還**禁止自我更新/下載可執行程式碼**——shun 式更新器執
  行時對任何經 Play 散佈的應用程式都是違規。側載在設計上被設限：
  **未經驗證開發者的應用程式自 2026-09-30 起進入多步流程並等待
  24 小時**。
- **捷徑**：主畫面圖示與應用程式捷徑（`shortcuts.xml`、
  `ShortcutManager`、`requestPinShortcut`）**只能由應用程式宣告**——
  不存在安裝期注入 API。建置期等價物是往 shun 打的包裡產生
  `shortcuts.xml`/intent-filter。

### iOS

- **打包機制**：IPA = 帶 `Payload/App.app` +
  `embedded.mobileprovision`（App ID + 權利 + 憑證 + Ad Hoc UDID
  白名單）的 zip。管道：App Store、TestFlight、Ad Hoc（每年每類
  100 台）、企業。**簽署工具鏈（codesign、xcodebuild、鑰匙圈）僅存
  於 macOS**——硬性宿主要求。
- **執行時安裝器：不可行**，除兩個縫隙：(a) Ad Hoc OTA 清單
  （`itms-services://?...manifest.plist`）——容易、合法、小眾；
  (b) 歐盟 Web Distribution / 替代市集機制——真實存在，但被 Apple
  資格門檻 + 公證 + 2026 年 10 月費率條款（5% 核心技術佣金）擋在
  門外，且僅限歐盟。免費 Apple ID 側載（AltStore/Sideloadly）是
  7 天/3 應用程式的愛好者路徑，不可產品化。
- **捷徑**：任何管道下安裝期都不存在任何東西——主畫面圖示、URL
  scheme、Universal Link 均為應用程式宣告且經簽章校驗。
- **egui 後備**：Android = `android-activity` + winit + wgpu
  （Vulkan/GLES）；iOS = winit + wgpu（Metal）經 FFI 內嵌 UIKit 宿
  主 + Xcode 專案。兩者都沒有一站式方案——shun 打包管線恰好是缺
  失的那塊。

## 5. 鴻蒙（預研——已決定暫緩）

**結論先行：2026 年「shun 支援鴻蒙」只能誠實地指一件事——一個產出
發佈簽署 HAP/APP 產物的建置期打包/簽署目標，外加 `hdc install`
開發機流程。執行時安裝器（shun 的 NSIS 那一半）在 HarmonyOS NEXT
上既無法律基礎也無技術基礎。**

生態（2025–2026）：HarmonyOS NEXT（5.0，2024-10）砍掉了 APK 相容
層；要瞄準的線是 HarmonyOS 6+（API 20/23），僅中國、僅應用市場，
約佔中國 OS 市場兩成。OpenHarmony 是開源基底；商業鴻蒙是其上的
華為產品——打包器瞄準商業版。

- **包格式**：HAP（zip：`module.json5`、ArkTS 位元組碼、原生
  `libs/<abi>/*.so`）；HSP/HAR 共享包；`.app` = 應用市場提交包
  （`pack.info`）。工具鏈可 CLI 使用：`ohpm` + `hvigorw
  assembleHap` + `hap-sign-tool` + `app_packing_tool.jar`（官方支援
  無頭 CI 建置）。
- **簽章**：SHA256withECDSA；`.p12` 金鑰庫 + `.cer` 憑證 + `.p7b`
  profile（套件名、權限、debug 時含裝置 UDID 白名單）；憑證**由
  華為經 AppGallery Connect 簽發**（個人註冊免費——沒有 Apple 式
  年費）。
- **Rust**：`aarch64/armv7/x86_64-unknown-linux-ohos` 為**帶宿主
  工具的 Tier 2**（1.78 起 rustup 可直接安裝）。社群 `ohos.rs`
  工具鏈（`cargo-ohos`、`napi-ohos`、`ohos-openssl`）是黏合劑；
  Rust 核心 + ArkTS 殼是已驗證架構（RustDesk OHOS）。**egui
  受阻**：winit 沒有上游 OHOS 後端（僅社群 beta）。**Tauri**：官方
  但未合入的 `feat/open-harmony` 分支（wry/tao 補丁、`cargo tauri
  ohos` CLI）今天可用但變動快——預計每幾個月重新固定版本。
- **散佈誠實性**：消費者側載實質關閉（僅應用市場；`hdc install`
  需開發者模式 + 華為簽章 + UDID）。指定裝置發佈：每年 100 台、
  90 天有效期。企業散佈目前限擎雲企業 PC。**鴻蒙電腦**是真實的
  （ARM、商店散佈、尚無側載）——華為已*表態*未來開放 PC 側載；
  這是唯一可能讓桌面交付執行時在那裡成立的觀察項，是承諾不是
  能力。

工作量表：HAP 打包目標**中等**；Rust 交叉編譯步驟**易–中**（SDK
clang 包裝為連結器、TLS 走 ohos-openssl）；Tauri-on-OHOS 打包
**中–難**（上游未合）；egui 後備**難**（無 winit）；執行時安裝/
燒錄**不可行**。

## 6. 安裝範圍與宣告式精靈（2026-09 實作）

### 安裝範圍 —— `install.scope = user | machine | ask`

每使用者仍是預設（見上文「為什麼安裝程式不請求 UAC 提權」）。
`machine` ——或 `ask` 被答成「所有使用者」——把每個註冊面翻到其機
器級等價物：ARP 條目落在 **HKLM**、捷徑進**全使用者開始功能表**
（`%ProgramData%`）、桌面捷徑寫**公用桌面**
（`FOLDERID_PublicDesktop`）、動詞與深層連結落
`HKLM\Software\Classes`。shell 在流程執行**之前**偵測解析結果，
未提權時以 `runas` 攜帶使用者的選擇重新啟動自身（`--silent
--mode=… --dir=… --scope=machine`）：UAC 確認是唯一的彈出視窗、
只出現在真正需要它的模式上——正是引導器模式，兌現承諾。解除安裝
同樣對稱（ARP 的 `UninstallString` 拉起解除安裝器、同樣方式提
權）。機器範圍僅限 Windows；Linux/macOS 後端明確拒絕。整合測試在
非提權執行器上自動跳過（管理員 shell 裡跑 `cargo` 即真機驗證）。

### 精靈管線 —— `[[package.metadata.shun.steps]]`

精靈從固定的 模式 → 安裝 序列變為**宣告式、有序、自由組合**的
管線。五種步驟：

```toml
[[package.metadata.shun.steps]]
kind = "mode"                    # 交付方式 + 目錄；內嵌 `ask` 原則的
                                 # 開關（桌面捷徑、安裝範圍）
[[package.metadata.shun.steps]]
kind = "scope"                   # 獨立的使用者/機器安裝範圍選擇
[[package.metadata.shun.steps]]
kind = "license"                 # 授權協議（license / license-locales）
[[package.metadata.shun.steps]]
kind = "content"                 # 自訂 markdown 面板
title = "發佈說明"
markdown = "notes.md"         # 相對清單檔案
[[package.metadata.shun.steps]]
kind = "install"                 # 交付執行（必須恰好一個）
```

不宣告 `steps` = 預設管線（模式 → 有授權則授權 → 安裝），舊的
`custom-steps` 按各自 `after` 機碼注入；兩者同時宣告是配置錯誤，
`install` 步驟零個或多個同樣是。安裝範圍之問與桌面捷徑開關出現在
`ask` 原則遇到的任何面板：內嵌於模式步驟，或獨立成步（`scope`）—
—由開發者自由組合。內容與授權文件在**建置期**讀取並內嵌進
`shun-steps.json`（`ShunConfig::resolve_steps`），執行時安裝器不攜
帶任何檔案相依。egui 降級介面渲染完整管線（逐步軌道、授權門檻、
上一步/下一步導覽）；Tauri 的 `ShellView` 把解析後的步驟暴露給
web 前端。

## 7. 綜合狀態（2026-09）

| 能力 | Win | Linux | macOS | Android | iOS | 鴻蒙 |
| --- | --- | --- | --- | --- | --- | --- |
| 執行時安裝 + 註冊 | ✅ 使用者級 + 機器級 | ✅ `.desktop` 後端（使用者級） | ✅ `.app` 後端（使用者級） | 暫緩 | 暫緩 | 暫緩 |
| 桌面/開始功能表捷徑 | ✅ 兩者（原則驅動） | ✅ 啟動器條目 | ✅（Launchpad/Spotlight） | 無 | 無 | 無 |
| 工作列/Dock 釘選 | 僅識別（AUMID 蓋章） | 僅識別（StartupWMClass） | 僅識別（LS） | 無 | 無 | 無 |
| 右鍵選單 | ✅ 第一層動詞 | ✅ Desktop Actions | NSServices 後續 | 暫緩 | 暫緩 | 暫緩 |
| 深層連結 | ✅ 協定類別 | ✅ MimeType + xdg-mime | ✅ CFBundleURLTypes | 暫緩 | 暫緩 | 暫緩 |
| 解除安裝敘事 | ✅ ARP + 自刪 | ✅ Action 式 | ✅ LS 註銷 + 檔案 | 系統 | 系統 | 系統 |
| 打包產物 | ✅ 安裝器 + MSIX | tarball ✅；deb/rpm 待續 | bundle ✅；DMG 待續 | 暫緩 | 暫緩 | 暫緩 |
| 簽章現實 | Authenticode / 商店 | 可選 | 必須（流程工作） | keystore / Play | Apple 憑證 | 華為 AGC |

**已決定暫緩（2026-09）**：Android、iOS 與鴻蒙——第 4–5 節的研究
結論仍然成立，但均不排期。

**後續排程**：

1. 經 `tauri-bundler` 出 deb/rpm 打包產物（Linux CI），DMG 走
   macOS 建置線。
2. Windows 右鍵選單第二層（檔案類型關聯）——若有消費者需要。
3. **永不做**：任何手機 OS 上的執行時安裝器/捷徑注入；任何地方的
   工作列/Dock 程式化釘選；snap（除非複雜度被證明值得）。
