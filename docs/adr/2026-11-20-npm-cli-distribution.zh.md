# 分发 CLI 套件和单工具

[English](2026-11-20-npm-cli-distribution.md)

状态: accepted

## 问题

用户需要在没有 Rust 工具链的情况下安装 `fsz` 和 `rmem`，也可以只装其中一个。公开仓库还需要一次发布操作，同时生成匹配的 GitHub Release 资产和 npm 包，不应手动重新构建或上传文件。

## 决策

项目维护者选定在公开源码仓库中由 tag 驱动发布。[发布工作流](../../.github/workflows/release.yml)只接受 `vX.Y.Z` 格式的正式版本 tag，并要求对应提交位于仓库默认分支的历史中。推送该 tag 后，工作流自动启动。工作流为 Linux x64（glibc）、macOS arm64 和 Windows x64 构建原生二进制，在当前仓库中创建 GitHub Release，并从同一批二进制发布 npm 版本 `X.Y.Z`。Cargo 包版本保持独立。

Release 任务使用工作流内置的 `GITHUB_TOKEN`，并只为该任务授予 `contents: write` 权限。npm 发布使用 Actions 仓库 secret `NPM_TOKEN`。不需要独立发布仓库或额外的 Release token。

公共套件包 `@melonyrame/devops-tool` 注册两条命令。`@melonyrame/fsz` 和 `@melonyrame/rmem` 各注册一条命令。它们通过可选依赖共用 `@melonyrame/devops-tool-linux-x64`、`@melonyrame/devops-tool-darwin-arm64` 和 `@melonyrame/devops-tool-win32-x64`。启动时只执行已安装的二进制，不运行安装脚本，也不进行本地编译。全局入口使用 Node；只装 Bun 时，可以通过 `bunx --bun` 运行工具。当前启动器不支持仅有 Bun 时在 Unix 直接运行全局命令。套件包和单工具包切换时先卸载旧包，不强制覆盖。

重试时，只有 Release 中的 `SOURCE_COMMIT` 与 tag 对应的源码提交一致，且校验和通过，工作流才复用该 Release。npm 步骤跳过已存在的包版本。npm 已发布版本不可修改；变更后的版本必须使用新的 `vX.Y.Z` tag。

### 维护者配置和发布步骤

首次发布前，创建名为 `NPM_TOKEN` 的 Actions 仓库 secret。npm 必须允许该凭据执行无人值守发布，并允许它创建或更新上述六个公开 `@melonyrame` 包。确保发布工作流已经位于默认分支，并为仓库启用 GitHub Actions。

先确保目标提交已经进入默认分支，再创建并推送 tag：

```sh
git switch <默认分支>
git pull --ff-only
git tag -a v1.2.3 -m "v1.2.3"
git push origin v1.2.3
```

推送 tag 就是发布触发器，不需要手动运行工作流。如果工作流在创建部分产物后失败，重新运行失败的 GitHub Actions 工作流。某个版本已有 Release 资产或 npm 包后，不要移动或重新创建对应 tag。

[独立安装器](../../packaging/install.sh)从当前仓库的 Releases 下载并校验二进制，安装到 `$HOME/.local/bin` 或 `INSTALL_DIR`，不需要 JavaScript 运行时。Windows 使用 Git Bash 运行该脚本。卸载时删除安装的文件。

## 备选方案

- 只提供套件包或只提供单工具包：无法同时满足两种安装入口。
- 在包管理器的生命周期脚本里下载：增加一次安装时下载，并依赖 Bun 默认限制的脚本执行。
- 使用独立发布仓库：对公开源码仓库没有实际收益，却会增加一个仓库、一项凭据和一个失败边界。
- 为同一个名称和版本提供不同的公共启动包：原型使用了不同的 Windows 和 Unix 压缩包，但 npm 不会为同一个包版本选择不同压缩包。正式流程使用同一个启动器，另发原生平台包。
- 覆盖同名命令：原型安装时，npm 在两个切换方向都拒绝覆盖。先卸载旧包可以避免强制覆盖。

## 后果与风险

完成一次性的 npm 配置后，维护者只需推送一个有效 tag，就能启动两条发布路径。版本 tag 无效、tag 不在默认分支历史中、npm secret 缺失、构建失败、校验失败或注册表失败时，发布会明确失败，不会静默跳过。

实现和本地包暂存可以在不发布的情况下验证，但目前尚未完成正式发布或公开安装验证。首个工作流成功后，用户才能安装公开版本。声明首发平台支持前，须在文档所列目标上验证公开 npm/Bun 命令和独立安装器。
