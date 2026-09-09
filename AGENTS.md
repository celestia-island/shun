# AGENTS.md — Shun Repository Rules for AI Agents

> 本文件改编自 Celestia 工作区规则（沿 shun-integration / wowsp 仓库的
> AGENTS.md 版式），只保留适用于本仓库（celestia-island/shun）的规则，并
> 记录了针对性调整（见 §9）。所有在本仓库工作的 AI agent / subagent
> **必须**遵守本文件。工作区级文件中的真实凭据与内网信息**永远不会**被
> 复制进本仓库（红线见 §6）。

---

## 1. Commit Message Format

```
<gitmoji> <Capitalized English summary ending with period.>
```

- 必须以一个 gitmoji 开头。白名单 = gitmoji.dev 完整规范集 + 组织增补
  （🔗 sync/copilot、🔄 sync/refresh、📜 license、🛡️ shield）。常用：
  ✨ 🐛 🔧 ♻️ 🔥 📝 🎨 ✅ 🚀 🌐 ⬆️ 🎉 📦。
  **权威实现是共享的 celestia-devtools commit-msg-lint**（CI 里的
  Commit Message Lint 工作流调用它）；白名单以该实现为准。
- 摘要为英文、首字母大写、以 `.` 结尾；**禁止 CJK 字符**。
- **禁止 Conventional Commits 前缀**（`feat:` / `fix:` 等）——emoji 本身
  就是类型标记；也**禁止冒号前缀句式**（`Topic: details`）。详细背景写进
  commit BODY（空行 + bullet），绝不写进摘要行。
- 禁止以裸版本号或填充短语开头（`v1.2.3` / `Bump version`）。
- **禁止 merge commit subject**（`Merge branch ...` / `Merge pull
  request ...`）：本仓只使用 squash merge。
- 豁免：`Revert "..."`（git revert 产物）；dependabot 等机器人的 subject。
- **PR 标题遵循完全相同的规则**（squash 后它就是 commit subject）。

## 2. CHANGELOG Policy（强制）

- **任何情况下不在仓库里维护 CHANGELOG / WHATSNEW / 修订历史文件。**
  合并的 PR 就是 changelog：squash commit（gitmoji + 一句话摘要）+ PR
  描述构成完整变更史，任意粒度用 `git log` 过滤即可。
- Release notes 写在 **git tag + GitHub Releases** 页面，绝不落在被跟踪
  的文件里。仓库里已不存在这类文件，不要新建；发现遗留的随触及它的 PR
  一并移除。

## 3. PR Workflow

每个阶段的工作必须遵循以下模式：

1. **从 master 切出 feature 分支**（`feat/<name>`，缺陷修复用
   `fix/<name>`）。有并行任务时用独立 `git worktree`。
2. **3 轮验证循环**：分析 → 改进 → 验证，重复三轮；**任何一轮失败，
   从零重新计数。**
3. 以 gitmoji 格式 **commit**。
4. **push** 分支。
5. 用 `gh pr create` **创建 PR**（标题遵循 §1；只在被要求或已批准的
   工作流步骤里开 PR，不要为琐碎变更单独开 PR——PR 号是有限资源，一个
   PR 打包一批可合并的功能）。
6. **squash merge**（满足 §5 门槛可自主合并）：subject 变为
   `<gitmoji> Summary. (#PRID)`。
7. 合并后**删除** feature 分支。

## 4. Branch Naming & Git Push Rules

- `master` — 生产分支。**只接受 squash merge 的 PR**，禁止直推；紧急
  修复走 `fix/<name>` 分支 + PR。
- `feat/<name>` — 新功能；`fix/<name>` — 缺陷修复；`chore/<name>` —
  维护；`refactor/<name>` — 无行为变化的重构。
- **`dev` — 已废弃，不要使用**（不要创建，也不要往它推送）。
- **禁止裸 `git push --force`**；feature 分支上的 rebase/amend 恢复一律
  优先 `git push --force-with-lease`，被拒时先 fetch 审查双方提交，拿不准
  就问用户。**master 上任何形式的 force push 绝对禁止。**

## 5. Merge & Release Rules

- **满足以下全部条件即可自主合并 PR**（无需逐 PR 人工确认）：
  1. 消息合规（§1）；
  2. 检查门槛：**代码级失败**（编译 / 测试 / clippy / fmt）必须修复，
     绝不带病合并；**环境性失败**（runner 抖动等）记录到 PR 并经本地
     验证通过后可豁免；
  3. PR 节约（§3.5）。
- **版本号随主 PR 走**：改版本就在功能 PR 里一并 bump（根 `Cargo.toml`
  的 `[package]` version 与 `[workspace.package]` version 两处），**不要**
  单独开纯 bump PR（除非用户明确要求）。
- **发布**：master 上以 `🔖 Release vX.Y.Z.` 提交后打 `v*` 标签推送——
  `release.yml` 出平台产物、`publish.yml` 发 crates.io（tag 必须在
  master 上）。发布前确认本地 `just ci` 全绿。

## 6. 敏感信息红线（强制，违反视为事故）

1. **禁止把任何真实密码 / 密钥 / token / 内网 IP 写进 git 树**（任何
   分支、任何文件，包括注释、示例、默认值、测试数据、README、docs）。
2. 需要密码时用环境变量 / 不入库的配置文件，或占位符；示例 IP 一律用
   RFC 5737 文档地址，示例值用明显假值。
3. 提交前自查：涉及配置 / 脚本 / 示例数据的改动，grep 一遍
   `password|secret|token|api_key` 确认无真实值。
4. 泄漏处置：立即删除 → 评估泄漏面 → 报告用户；**无论是否重写，凭据
   视为已公开，必须轮换**。

## 7. Build & Test

- Rust：`cargo fmt` / `cargo clippy` / `cargo test`（仓库封装
  `just fmt` / `just clippy` / `just test`，一键 `just ci`）。
- Web 壳前端：`shell/web` 下 `pnpm build`（改动前端源码后必须重建
  `dist/` 并一并提交——运行时嵌入的是构建产物）。
- 主要开发在 Windows 上进行；CI 的完整检查也是 windows runner
  （linux 只跑 lib）。shell crate 只在 Windows 编译其注册后端，
  Linux/macOS 侧代码靠 CI 的 ubuntu 任务与共享纯函数测试覆盖。
- **文档八语言同步**：改了 `docs/en/` 下的指南 / README，同一改动要
  同步到 es / fr / ja / ko / ru / zh-Hans / zh-Hant 对应文件；设计笔记
  （design/）按惯例只维护 en + zh-Hans。
- **跨仓依赖**：hikari 走 npm 发布包 `@celestia-island/hikari`，不要
  引入指向本机目录的 path 依赖。

## 8. CI 使用策略

1. **CI 是参考不是门禁**：合并前看一眼有没有**代码级失败**；环境性失败
   记录到 PR 即可豁免（§5.2）。**不要长时间盯 CI**——排队或挂起超过
   ~15 分钟按环境性处理。
2. CI 结构（`.github/workflows/`）：
   - `checks.yml` — fmt + clippy + test：windows 全量，ubuntu 仅 lib；
   - `commit-msg-lint.yml` / `pr-title-check.yml` — 共享
     celestia-devtools 校验（§1）；
   - `release.yml` / `publish.yml` — `v*` 标签触发的产物构建与
     crates.io 发布（publish 要求 tag 在 master 上）。
3. 同 PR 反复 push 触发的旧 run 可 `gh run cancel <id>` 释放配额。

## 9. 与工作区 AGENTS.md 的差异记录

- 节点表 / NFS / worktree 软链 / 部署等基础设施章节不适用——shun 是
  本地 Windows 开发 + GitHub Actions 托管 CI。
- 大文件下载纪律（工作区 §9）在此不涉及（shun 无模型 / 大资源拉取），
  保留常识：>5GB 下载先报量确认。
- shun 特有：文档八语言同步义务（§7）、web dist 随源码提交（§7）、
  发布走 tag 触发双工作流（§5）。
