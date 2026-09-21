# 設計筆記：安裝期腳本化

狀態：**執行器已定 —— duckscript。Python 已調研並驗證可嵌入；是否
啟用待定。** duckscript 是唯一的腳本執行器；JavaScript 方案放棄
（duckscript 背後的 cargo-make 工具鏈是完整的，不足之處以呼叫
Python 保底）。本筆記記錄該決定與 Python 嵌入調研結論。

## duckscript（執行器）

[`duckscriptsdk`](https://crates.io/crates/duckscriptsdk)（Apache-2.0，
cargo-make 的腳本語言）可作為普通相依嵌入：把命令集裝進
`Context`，將 shun 內建介面註冊為自訂命令，腳本即可獲得流程控制
與 std 的 fs/env/net 能力。可行性證明見
`tests/scripting_duckscript.rs`。另註：justfile 本身不可內嵌（`just`
crate 是 CLI，沒有穩定函式庫 API）—— duckscript 是該家族裡可內嵌
的那一個。

```toml
[package.metadata.shun.script]
runner = "duckscript"

[[package.metadata.shun.script.hooks]]
phase = "prepare"                 # prepare | post-install | pre-uninstall
script = "installer/prepare.dk"   # 由 `shun build` 打包
```

shun 內建介面將以 duckscript 命令形式註冊：`shun_progress`、
`shun_emit`、`shun_fetch`（帶校驗的下載）等，加上 SDK 自帶的 std
命令（fs、env、http、process、semver……）。

指令碼鉤子執行時，交付流程會把精靈語言以 `SHUN_LANGUAGE`
環境變數匯出給每個指令碼步驟（即在精靈第一步選擇的語言，同時也
記錄在磁碟上的安裝清單中）。未選擇語言時該變數不存在。shun 只
匯出事實——把語言寫進已安裝應用自身的設定，是 payload 指令碼
自己的事。

需要在 shun 封裝裡正規化的坑：duckscript 參數裡 Windows 反斜線是
逸出字元（傳正斜線路徑）；賦值必須用 `x = 命令 參數` 的輸出擷取
語法。

## 嵌入式 Python —— 實測記錄（2026-09 探針）

以下全部真實跑過兩遍：宿主 CPython 3.13.5 一遍，**攜帶的
embeddable 執行時**一遍（解包 `python-3.13.5-embed-amd64.zip`，把
PyO3 的 `pyembed_runner` 範例放在旁邊，`python313.dll`/標準函式庫
都從攜帶目錄載入 —— `sys.prefix` 已確認指向攜帶目錄）：

| 能力 | 結果 |
| --- | --- |
| 真實 HTTPS（urllib + TLS） | 通過（本機網路直連 pypi.org 被掐；example.com/騰訊鏡像正常） |
| 串流 SHA-256 + HMAC | 通過 |
| AES-CTR 往返、RSA-2048 簽章/驗證 | 通過 —— 經預裝進攜帶執行時的 `cryptography` wheel（`pip --target runtime/Lib/site-packages` + 在 `python313._pth` 啟用 `import site`） |
| 機器碼 | MachineGuid（winreg）、MAC（`uuid.getnode`）、C: 磁碟區序號（ctypes `GetVolumeInformationW`）—— 全通過 |
| TPM | ctypes 走 `tbs.dll` 的呼叫路徑正確；探針機的 firmware 關閉了 TPM，`Tbsi_Context_Create` 回傳 `TBS_E_TPM_NOT_FOUND`（0x8028400F —— 注意不是 0x80284002，那是參數結構傳 NULL 導致的 `TBS_E_BAD_PARAMETER`）。呼叫機制已驗證；TPM 啟用的機器上同一段程式碼可讀 `TPM_PT_MANUFACTURER` |

實測體積：embeddable zip **10.9 MB** / 解包 **20.4 MB** /
+cryptography wheel **32.4 MB**；PyO3 runner 二進位本身約 0.2 MB。
帶原生 `.pyd` 的第三方 wheel（如 cryptography）原樣可用 —— 直接隨
攜帶執行時一起發行。

為正式接入記錄的坑：嵌入式直譯器 drop 時不會 finalize —— 跑完腳本
要顯式 flush stdio（見 runner 範例）；`eval` 只吃運算式；對攜帶執
行時用 pip 需要 `--target` 加 `._pth` 改動（或改用自帶 pip 的
python-build-standalone 執行時）。

## WebView2 fixed-version 內嵌 —— 實測

問題：安裝器能否把 WebView2 引擎本體帶在身上，同時驅動自己的 UI
與裝出去的應用程式？**機械上可行 —— 已端到端跑通**；代價在體積。

- v151.0.4129.101 x64 fixed-version cab：**307,241,094 位元組 ≈
  293 MB** 壓縮，**661.1 MB 解包**。
- demo 殼對著解包後的攜帶執行時執行（`WEBVIEW2_BROWSER_EXECUTABLE_FOLDER`
  —— 它本來就是 `webview2_available` 偵測的第一優先級）：UI 正常渲染
  （離線截圖驗證），且**全部 6 個渲染處理程序都來自攜帶目錄**而非
  系統 Evergreen。
- 結論：可行；約 300 MB 的成本**已接受**（與同類打包器相當）。雙份
  副本的顧慮已從機制上消除：發行物只內嵌一份 —— 安裝殼用 payload
  裡的執行時子目錄自舉（`extract_prefix` 暫存 + 解壓雜湊重用），
  安裝殼與裝出的應用程式共用它；詳見 configuration.md 的 WebView2
  原則節。對什麼都沒有的機器，egui 降級介面仍是零成本保底。


## 嵌入式 Python（已調研，可行）

**結論：可以 —— Rust 層面能乾淨地嵌入一個小型 CPython。** 證明在
`tests/scripting_python.rs`，掛在選擇啟用的 `python-probe` feature
後面：[PyO3](https://pyo3.rs) 的 `auto-initialize` 把直譯器嵌進處理
程序，真實 Python 求值（標準函式庫可用）、呼叫自訂 Rust 函式、
Python 例外映射為 Rust 錯誤，全鏈路跑通。該 feature 絕不進預設建
置；CI 上只有 Windows 腿（`--all-features` 對著預裝的 CPython 解
析）會執行它。

自包含安裝器的攜帶選項，仿照 WebView2 原則表：

| 選項 | 攜帶物 | 說明 |
| --- | --- | --- |
| `system`（預設） | 無 | duckscript 的 `process` 命令可呼叫已安裝的 python；不存在時優雅降級 |
| `embeddable` | Windows embeddable 包（約 12–16 MB） | 官方 `python-3.x.x-embed-amd64.zip`：`python3xx.dll` + 標準函式庫 zip + `._pth`，免管理員、零登錄檔 —— 與 fixed-version WebView2 同哲學的私有執行時 |
| `standalone` | python-build-standalone（約 30–60 MB） | [Astral 接管維護](https://astral.sh/blog/python-build-standalone)的發行版（`uv` 同款）；跨平台、版本鎖定、功能完整；除非需要 pip/原生相依，否則殺雞用牛刀 |

草案：

```toml
[package.metadata.shun.script.python]    # 可選的重型保底
type = "embeddable"                      # system | embeddable | standalone
```

已否決/擱置的替代方案：

- **RustPython**（MIT，純 Rust）—— 自我宣告未達生產可用，標準函式
  庫有缺口、不支援 C 擴充模組；將來或有轉機，但今天不適合安裝器。
- **PyOxidizer / `pyembed`** —— 更高層的嵌入封裝，但專案處於維護模
  式；這裡用 PyO3 本體就夠了。

接入前待決問題：

- 體積預算：對選擇啟用的產品，發行物 +12–16 MB 是否可接受？
  （嵌入按清單逐產品啟用，預設發行物不受影響。）
- 版本耦合：PyO3 連結的是建置主機的 CPython；隨包發行的執行時必須
  相符。以「建置時就指向我們將來發行的那份發行版」來鎖定
  （`PYO3_PYTHON` → 解開的 embeddable/standalone 目錄）。
- 隔離模型：處理程序內嵌入直譯器（保住單檔安裝器體驗）vs 子處理程
  序（當機隔離更簡單）—— 或按鉤子類型二選一。
- 哪些鉤子允許升級到 Python（僅 prepare，還是也包括安裝後修復路
  徑？）。
