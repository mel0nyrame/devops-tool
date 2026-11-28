# 将独立 CLI 源码归入统一目录

[English](2026-11-20-cli-source-layout.md)

状态: accepted

## 问题

仓库包含两个独立 Rust 工具：目录大小查看器 `fsz` 和只读进程监视器 `rmem`。需要统一组织源码和三端验证，同时保留两条命令和各自的主要交互。仓库名为 `devops-tool`。当前工作范围是仓库整理和原生 Windows、macOS、Linux 适配，不包括发布安装包。

## 决策

项目维护者选定根 Cargo workspace，将两个独立 crate 放入 `crates/fsz/` 和 `crates/rmem/`。根目录负责 Cargo 锁文件、项目说明及全仓验证；各工具保有自己的 manifest、源码和工具文档。未来的 Rust 工具可在 `crates/` 下加入为独立成员。两个 crate 共用根锁文件和根目录的构建产物忽略规则。

已确认保留现有命令用法和核心交互。三端支持以各操作系统原生构建和核心功能冒烟检查为准；未实测的系统不计为已验证。目录名不依赖仓库名，[分发方案](2026-11-20-npm-cli-distribution.zh.md)也不要求在移植阶段创建 npm 目录。

## 备选方案

- 根目录直接放两个 crate：路径最短，但工具目录与文档、CI 等项目级内容并列；项目维护者不偏好这种布局。
- 使用 `tools/` 分组：更直接表达用户看到的是工具，也适合将来容纳非 Rust 工具。当前两个成员都是 Rust crate，`crates/` 更明确表达 Cargo 成员的职责。这两个分组均不改变构建和发布能力。
- 使用单个 crate 的 `src/bin/`：文件树可以更短，但会把两个独立工具的依赖和包元数据合在一起。`rmem` 已有独立的采集、交互和界面模块，不必为了集中目录牺牲包边界。

[Biome 的 Cargo workspace](https://github.com/biomejs/biome/blob/main/Cargo.toml)使用 `crates/`；[uutils/coreutils 的 workspace](https://github.com/uutils/coreutils/blob/main/Cargo.toml)将工具放在 `src/uu/` 下。两者说明分组是源码组织选择，不要求复用它们的发布或聚合方式。

## 后果与风险

分组后根目录更清晰，仍能按包独立构建；代价是工具路径增加一层，迁移时需更新相对路径和构建说明。两个工具在 Windows、macOS 和 Linux 上均有原生构建和核心冒烟验证结果。CI 在三端验证 `fsz` 的符号链接行为，并在 macOS 和 Linux 验证 `rmem` 的终端交互；Windows 的 `rmem` 交互检查在本机 CMD 中通过 WinPTY 完成。详见[平台状态](../../README.zh.md#平台状态)。这项工作不依赖 npm 分发方案。

仅支持 macOS 的 `lfd` 按同一布局放在 `crates/lfd/`。只读应用检查及隔离测试中的清理预览已通过 macOS arm64 原生构建和伪终端冒烟检查。这不改变 `fsz` 和 `rmem` 的平台或分发状态。
