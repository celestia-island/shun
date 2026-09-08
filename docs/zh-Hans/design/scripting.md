# 设计笔记：安装期脚本化

状态：**运行器已定 —— duckscript。Python 已调研并验证可嵌入；是否启用
待定。** duckscript 是唯一的脚本运行器；JavaScript 方案放弃
（duckscript 背后的 cargo-make 工具链是完整的，不足之处以调用 Python
兜底）。本笔记记录该决定与 Python 嵌入调研结论。

## duckscript（运行器）

[`duckscriptsdk`](https://crates.io/crates/duckscriptsdk)（Apache-2.0，
cargo-make 的脚本语言）可作为普通依赖嵌入：把命令集装进
`Context`，将 shun 内置接口注册为自定义命令，脚本即可获得流程控制
与 std 的 fs/env/net 能力。可行性证明见
`tests/scripting_duckscript.rs`。另注：justfile 本身不可内嵌（`just`
crate 是 CLI，没有稳定库 API）—— duckscript 是该家族里可内嵌的那
一个。

```toml
[package.metadata.shun.script]
runner = "duckscript"

[[package.metadata.shun.script.hooks]]
phase = "prepare"                 # prepare | post-install | pre-uninstall
script = "installer/prepare.dk"   # 由 `shun build` 打包
```

shun 内置接口将以 duckscript 命令形式注册：`shun_progress`、
`shun_emit`、`shun_fetch`（带校验的下载）等，加上 SDK 自带的 std
命令（fs、env、http、process、semver……）。

需要在 shun 封装里归一化的坑：duckscript 参数里 Windows 反斜杠是
转义符（传正斜杠路径）；赋值必须用 `x = 命令 参数` 的输出捕获语法。

## 嵌入式 Python（已调研，可行）

**结论：可以 —— Rust 层面能干净地嵌入一个小型 CPython。** 证明在
`tests/scripting_python.rs`，挂在选启的 `python-probe` feature 后面：
[PyO3](https://pyo3.rs) 的 `auto-initialize` 把解释器嵌进进程，真实
Python 求值（标准库可用）、调用自定义 Rust 函数、Python 异常映射为
Rust 错误，全链路跑通。该 feature 绝不进默认构建；CI 上只有 Windows
腿（`--all-features` 对着预装的 CPython 解析）会执行它。

自包含安装器的携带选项，仿照 WebView2 策略表：

| 选项 | 携带物 | 说明 |
| --- | --- | --- |
| `system`（默认） | 无 | duckscript 的 `process` 命令可调用已安装的 python；不存在时优雅降级 |
| `embeddable` | Windows embeddable 包（约 12–16 MB） | 官方 `python-3.x.x-embed-amd64.zip`：`python3xx.dll` + 标准库 zip + `._pth`，免管理员、零注册表 —— 与 fixed-version WebView2 同哲学的私有运行时 |
| `standalone` | python-build-standalone（约 30–60 MB）） | [Astral 接管维护](https://astral.sh/blog/python-build-standalone)的发行版（`uv` 同款）；跨平台、版本锁定、功能完整；除非需要 pip/原生依赖，否则杀鸡用牛刀 |

草案：

```toml
[package.metadata.shun.script.python]    # 可选的重型兜底
type = "embeddable"                      # system | embeddable | standalone
```

已否决/搁置的替代方案：

- **RustPython**（MIT，纯 Rust）—— 自我声明未达生产可用，标准库有
  缺口、不支持 C 扩展模块；将来或有转机，但今天不适合安装器。
- **PyOxidizer / `pyembed`** —— 更高层的嵌入封装，但项目处于维护
  模式；这里用 PyO3 本体就够了。

接入前待决问题：

- 体积预算：对选择启用的产品，发行物 +12–16 MB 是否可接受？
  （嵌入按清单逐产品启用，默认发行物不受影响。）
- 版本耦合：PyO3 链接的是构建主机的 CPython；随包发行的运行时必须
  匹配。以"构建时就指向我们将来发行的那份发行版"来锁定
  （`PYO3_PYTHON` → 解开的 embeddable/standalone 目录）。
- 隔离模型：进程内嵌入解释器（保住单文件安装器体验）vs 子进程
  （崩溃隔离更简单）—— 或按钩子类型二选一。
- 哪些钩子允许升级到 Python（仅 prepare，还是也包括安装后修复
  路径？）。
