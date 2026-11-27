# AGENTS.md

## Repository guidance

- **Feature worktree:** Before implementing a feature, create a new Git worktree under the repository root’s `.worktree/` directory and implement and verify the feature there.
- **Portability:** Preserve command usage and core interactions. Before moving source, run `git status --short --untracked-files=all`; `git diff` omits untracked work.
- **Workspace:** Keep each tool an independent member under `crates/`. The root `Cargo.lock` governs the workspace, not leftover member lockfiles. Read the accepted [layout ADR](docs/adr/2026-11-20-cli-source-layout.md) before changing package boundaries.
- **Tool changes:** Read the affected crate's instructions ([`fsz`](crates/fsz/AGENTS.md), [`lfd`](crates/lfd/AGENTS.md), [`rmem`](crates/rmem/AGENTS.md)) for behavior and smoke checks. Each tool's README owns its user-facing contract.
- **Toolchain:** Use installed tools and ask before installing software, packages, runtimes, or Rust targets. If Windows Git Bash cannot find Cargo, check `$HOME/.cargo/bin` first.
- **Verification:** For Rust changes, run `cargo test --workspace --locked --offline` from the root, then the affected tool's smoke check. If the local cache or lockfile blocks the checks, report it and get approval before downloading dependencies or regenerating the lockfile online. Hosted CI may fetch locked dependencies. Report the OS and actual results. Count an OS as verified only after a native build and the applicable smoke checks pass there; a zero-test build, cross-build, or CI setup alone is not enough. Sync platform status in both root READMEs and both versions of the layout ADR when evidence changes.
- **Docs and decisions:** Edit English READMEs and ADRs first, then align their linked Chinese versions. Use the English-only [glossary](GLOSSARY.md) for user-facing terms. Read relevant [ADRs](docs/adr/) before reopening choices; record durable tradeoffs there and distinguish proposed, accepted, and implemented work.
- **Distribution:** Read the [npm distribution proposal](docs/adr/2026-11-20-npm-cli-distribution.md) before packaging. Packaging and publishing are separate from portability; require explicit authorization for each.

## Agent skills

### Design and delivery

Read the [design and delivery checkpoints](docs/agents/design-delivery.md) before a design interview or the first push to a new remote.

### Issue tracker

Use GitHub Issues for specs and tickets. See [issue tracker](docs/agents/issue-tracker.md).

### Triage labels

Read the [triage label mapping](docs/agents/triage-labels.md) before triaging issues.

### Domain docs

This is a single-context repository. See [domain docs](docs/agents/domain.md) before exploring or proposing changes.
