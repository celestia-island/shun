# 设计笔记：安装期脚本化（草稿，未实现）

状态：**已记录需求，一个运行器已验证可内嵌，设计待定。** 尚未接入
交付运行时；写下来是为了让约束条件在迭代之间不丢失。

## 目标

安装器可以引用打包进发行物的脚本。shun CLI 在构建期完成编译/打包，
运行时由内嵌引擎执行 —— 不依赖系统解释器，脚本运行期也不默认联网
（除非脚本主动请求）。两个候选运行器：

### 方案 A —— duckscript（已验证可内嵌）

[`duckscriptsdk`](https://crates.io/crates/duckscriptsdk)（Apache-2.0，
cargo-make 的脚本语言）可以作为普通依赖嵌入：把命令集装进
`Context`，将 shun 内置接口注册为自定义命令，脚本即可获得流程控制
与 std 的 fs/env/net 能力。可行性证明见
`tests/scripting_duckscript.rs` —— 变量、`writefile`/`readfile`、
`assert_eq`，以及一个记录调用数的自定义 `shun_note` 命令，一份脚本
跑通，无 unsafe、无 C 依赖。另注：justfile 本身**不可内嵌**（`just`
crate 是 CLI，没有稳定库 API）—— duckscript 是该家族里可内嵌的那
一个，最接近"安装时执行我们项目里的这些脚本"的诉求。

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

### 方案 B —— JavaScript（boa）

生态更丰富、Web 开发者上手即用；类型支持以 npm 包发布
（`@celestia-island/shun-script`），附带 `.d.ts` 声明。但建设成本更
高：boa 的沙箱与异步人体工学、源码打包步骤。

## 内置接口（需求，与运行器无关）

- **fs** —— 读写/移动/删除，流式复制。
- **hash** —— 流式校验和（至少 SHA-256 家族）。
- **crypto** —— 流式加解密；下载数字的签名校验。
- **net** —— 带进度的 HTTP(S) 请求、可断点续传的下载。
- **deps** —— 动态拉取依赖（脚本或归档），先校验再使用。
- **flow** —— 产出/消费 `FlowEvent`、追加步骤、设置失败信息。

## 待决问题

- 是否 duckscript 先行（内嵌成本低、足够覆盖安装期胶水），用
  `runner` 字段给 boa 留门 —— 还是第一天就双运行器？
- 沙箱模型：按清单做能力级命令隔离，还是单一可信脚本模型（发行物
  本身已签名）？
- `deps` 是否允许任意 URL，还是仅限声明它的清单所属的发布源。
- duckscript 参数里的 Windows 路径转义（反斜杠是转义符；须传正斜杠
  形式或加引号）—— 需要在 shun 命令封装里定一条归一化规则。
