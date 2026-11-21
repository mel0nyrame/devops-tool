# devops-tool

[简体中文](README.zh.md)

A collection of small, independent Rust command-line tools.

| Tool | Purpose |
| --- | --- |
| [`fsz`](crates/fsz/README.md) | Show the logical size of a file or the contents of a directory. |
| [`rmem`](crates/rmem/README.md) | Monitor local processes and memory in a read-only terminal UI. |

## Install

Install from a local checkout with Cargo and Rust 1.95 or newer. From the repository root, run either command, or both:

```sh
cargo install --locked --path crates/fsz
cargo install --locked --path crates/rmem
```

Add Cargo's bin directory to `PATH` if needed (by default, `$HOME/.cargo/bin` on Unix or `%USERPROFILE%\.cargo\bin` on Windows). Then run `fsz .` or start `rmem` in an interactive terminal.

To try the tools without installing them, run `cargo run --locked -p fsz -- .` or `cargo run --locked -p rmem` from the repository root.

### Install from a public release

These commands become available after the first successful publication. Supported targets are Windows x64, macOS arm64, and Linux x64 with glibc.

```sh
npm install -g @mel0nyrame/devops-tool
# Or install the same suite with Bun:
bun add -g @mel0nyrame/devops-tool
```

Use `@mel0nyrame/fsz` or `@mel0nyrame/rmem` for a single tool. Before switching between the suite and a single-tool package, uninstall the old package. The global launchers use Node. With Bun but no Node, use `bunx --bun -p @mel0nyrame/fsz fsz .` or `bunx --bun -p @mel0nyrame/rmem rmem`.

To install without Node, npm, or Bun, run the standalone installer in a Unix shell (Git Bash on Windows):

```sh
curl -fsSLo install.sh https://github.com/mel0nyrame/devops-tool/releases/latest/download/install.sh
sh install.sh
```

Pass `fsz` or `rmem` to install one tool, or `all vX.Y.Z` to pin a published version. The installer checks SHA-256 before copying the commands to `$HOME/.local/bin` (override with `INSTALL_DIR`). To uninstall, remove the installed executable files from that directory.

## Platform status

The [native CI workflow](.github/workflows/ci.yml) builds and tests both tools on Windows, macOS, and Linux. The `fsz` checks verify an 8 B scan that excludes a nested directory symlink and a 13 B scan when the symlink is passed explicitly. Windows also checks junction exclusion. The workflow runs the `rmem` interactive smoke check on macOS and Linux. The same interaction was checked locally on Windows x64 GNU in CMD via WinPTY: PID search, matching details, `Esc` back to the list, and `q` back to a working shell.

See the [glossary](GLOSSARY.md) for terminology and [design decisions](docs/adr/) for rationale.
