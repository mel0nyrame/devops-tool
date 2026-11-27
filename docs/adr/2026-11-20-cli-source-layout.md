# Group independent CLI crates under one directory

[简体中文](2026-11-20-cli-source-layout.zh.md)

Status: accepted

## Problem

The repository contains two independent Rust tools: `fsz` measures file and directory sizes, and `rmem` monitors local processes without modifying them. The project needs a common source layout and native Windows, macOS, and Linux verification while preserving both commands and their core interactions. The repository is named `devops-tool`. Package publishing is outside the portability work.

## Decision

The maintainer selected a root Cargo workspace with independent members at `crates/fsz/` and `crates/rmem/`. The workspace owns the shared lockfile, project overview, and workspace-wide validation; each crate owns its manifest, source, and tool documentation. Future Rust tools can join as independent members. Both crates use the root lockfile and root build-output ignore rules.

Preserve existing command usage and core interactions. Verify each platform with a native build and a core smoke check. The layout does not depend on the repository name, and the [distribution proposal](2026-11-20-npm-cli-distribution.md) does not require creating npm packaging during the port.

## Alternatives

- Keep both crates at the root: shortest paths, but tool folders sit alongside project-level documentation and CI. The maintainer preferred to group them.
- Group them under `tools/`: emphasizes user-facing tools and could accommodate non-Rust tools. `crates/` better describes a directory that contains only Cargo members. Either grouping allows the same builds and distribution.
- Use `src/bin/` in one crate: shortens the tree but combines package metadata and dependencies for independent commands. The separate boundaries also preserve `rmem`'s collection, interaction, and presentation modules within its own crate.

[Biome](https://github.com/biomejs/biome/blob/main/Cargo.toml) groups its Rust crates under `crates/`; [uutils/coreutils](https://github.com/uutils/coreutils/blob/main/Cargo.toml) groups utilities under `src/uu/`. Grouping is a source-layout choice, independent of how these projects aggregate or distribute binaries.

## Consequences and risks

The root stays focused on project-wide files and each tool remains independently buildable. Tool paths gain one level, so references and build instructions must follow the move. Windows, macOS, and Linux have native build and core smoke evidence for both tools. CI checks `fsz` symlink behavior on all three platforms and `rmem` terminal interaction on macOS and Linux; Windows `rmem` was checked locally in CMD via WinPTY. The [platform status](../../README.md#platform-status) links to the evidence. The npm design does not block this work.

The macOS-only `lfd` follows the same layout at `crates/lfd/`. Its read-only app inspection and isolated cleanup preview were verified with native macOS arm64 builds and PTY smoke checks. This does not change the platform or distribution status of `fsz` and `rmem`.
