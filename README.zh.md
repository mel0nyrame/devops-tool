# devops-tool

[English](README.md)

一组小巧、独立的 Rust 命令行工具。

| 工具 | 用途 |
| --- | --- |
| [`fsz`](crates/fsz/README.zh.md) | 查看文件的逻辑大小，或查看目录下各项的逻辑大小。 |
| [`rmem`](crates/rmem/README.zh.md) | 在终端界面中只读监视本机进程和内存。 |

## 安装

目前可用 Cargo 从本地检出的源码安装。准备 Cargo 和 Rust 1.95 或更新版本，在仓库根目录按需运行一条或两条命令：

```sh
cargo install --locked --path crates/fsz
cargo install --locked --path crates/rmem
```

如有需要，将 Cargo 的二进制目录加到 `PATH`。默认目录在 Unix 上为 `$HOME/.cargo/bin`，在 Windows 上为 `%USERPROFILE%\.cargo\bin`。然后运行 `fsz .`，或在交互式终端中启动 `rmem`。

如果只想试用、不安装，可以在仓库根目录运行 `cargo run --locked -p fsz -- .` 或 `cargo run --locked -p rmem`。

### 从公开版本安装

以下命令在首个版本成功发布后可用。目标平台为 Windows x64、macOS arm64 和 Linux x64（glibc）。

```sh
npm install -g @mel0nyrame/devops-tool
# 或使用 Bun 安装同一个套件包：
bun add -g @mel0nyrame/devops-tool
```

只装一个工具时，使用 `@mel0nyrame/fsz` 或 `@mel0nyrame/rmem`。在套件包与单工具包之间切换时，先卸载旧包。全局命令入口使用 Node；如果只有 Bun，可以用 `bunx --bun -p @mel0nyrame/fsz fsz .` 或 `bunx --bun -p @mel0nyrame/rmem rmem`。

不想使用 Node、npm 或 Bun 时，在 Unix shell（Windows 上为 Git Bash）中运行：

```sh
curl -fsSLo install.sh https://github.com/mel0nyrame/devops-tool/releases/latest/download/install.sh
sh install.sh
```

传入 `fsz` 或 `rmem` 可只装一个工具；传入 `all vX.Y.Z` 可指定已发布的版本。脚本先校验 SHA-256，再把命令安装到 `$HOME/.local/bin`，可通过 `INSTALL_DIR` 更改目录。卸载时删除该目录中对应的可执行文件。

## 平台状态

[原生 CI 工作流](.github/workflows/ci.yml)在 Windows、macOS 和 Linux 上构建并测试两个工具。`fsz` 验证扫描时跳过嵌套目录符号链接（8 B），以及显式传入该链接时扫描目标（13 B）；Windows 还验证跳过目录联接。工作流在 macOS 和 Linux 上运行 `rmem` 交互冒烟检查。同一交互已在 Windows x64 GNU 的 CMD 中通过 WinPTY 完成本地检查：按 PID 搜索、打开对应详情、按 `Esc` 返回列表、按 `q` 退出后正常执行 shell 命令。

项目术语见[术语表](GLOSSARY.md)，设计理由见[决策记录](docs/adr/)。
