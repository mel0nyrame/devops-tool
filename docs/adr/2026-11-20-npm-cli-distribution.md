# Distribute the CLI suite and individual tools

[简体中文](2026-11-20-npm-cli-distribution.zh.md)

Status: accepted

## Problem

Users need to install both `fsz` and `rmem`, or either tool alone, without a Rust toolchain. The public repository also needs one release action that publishes matching GitHub Release assets and npm packages without manual rebuilding or uploading.

## Decision

The maintainer selected a tag-driven release in the public source repository. [The release workflow](../../.github/workflows/release.yml) accepts a stable `vX.Y.Z` tag only when its commit is in the repository's default-branch history. Pushing that tag starts the workflow automatically. It builds native binaries for Linux x64 (glibc), macOS arm64, and Windows x64, creates the GitHub Release in this repository, and publishes npm version `X.Y.Z` from the same binaries. Cargo package versions remain independent.

The Release job uses the workflow's built-in `GITHUB_TOKEN` with job-scoped `contents: write` permission. npm publication uses the `NPM_TOKEN` Actions repository secret. No separate release repository or release token is required.

The public suite package `@mel0nyrame/devops-tool` registers both commands. `@mel0nyrame/fsz` and `@mel0nyrame/rmem` each register one command. They share `@mel0nyrame/devops-tool-linux-x64`, `@mel0nyrame/devops-tool-darwin-arm64`, and `@mel0nyrame/devops-tool-win32-x64` through optional dependencies. Launch uses an installed binary, with no install-time scripts or compilation. Global launchers use Node; Bun alone can execute a tool through `bunx --bun`. Direct global launch on Unix with Bun alone remains unsupported by this launcher. Uninstall the old package before switching between the suite and a single-tool package; do not force overwrite.

Retries reuse a Release only when its `SOURCE_COMMIT` matches the tagged source commit and its checksums pass. The npm step skips package versions that already exist. A published npm version is immutable; a changed release requires a new `vX.Y.Z` tag.

### Maintainer setup and release procedure

Before the first release, create an Actions repository secret named `NPM_TOKEN`. npm must accept it for unattended publication, and it must be allowed to create or update all six public `@mel0nyrame` packages listed above. Keep the release workflow on the default branch and enable GitHub Actions for the repository.

Tag a commit that is already in the default branch, then push the tag:

```sh
git switch <default-branch>
git pull --ff-only
git tag -a v1.2.3 -m "v1.2.3"
git push origin v1.2.3
```

The tag push is the release trigger; no manual workflow dispatch is required. If publication fails after creating some outputs, rerun the failed GitHub Actions workflow. Do not move or recreate a tag after any Release asset or npm package for that version has been published.

The [standalone installer](../../packaging/install.sh) downloads and checks binaries from this repository's Releases. It installs them in `$HOME/.local/bin` or `INSTALL_DIR` without a JavaScript runtime. On Windows it runs in Git Bash. Remove the files to uninstall.

## Alternatives

- Suite only or individual packages only: neither provides both installation choices.
- Download from a package-manager lifecycle script: adds an install-time network step and depends on script execution, which Bun restricts by default.
- Use a separate release repository: adds another repository, credential, and failure boundary without providing a benefit for a public source repository.
- Ship different public launchers under one name and version: the prototype used distinct Windows and Unix archives, but npm does not select different archives for one package version. The production path uses one launcher and keeps native payloads separate.
- Overwrite shared command entries: npm rejected both switch directions during prototype installation; uninstall-first avoids forcing a collision.

## Consequences and risks

After the one-time npm setup, pushing one valid tag is the only maintainer action required to start both publication paths. Invalid version tags, tags outside the default-branch history, a missing npm secret, build failures, checksum failures, or registry failures stop publication and leave the workflow failed rather than silently skipping the release.

The implementation and local package staging are testable without publishing, but no production release or public installation has been verified. Installability follows only after the first successful workflow run. Verify public npm/Bun commands and the standalone installer on the documented targets before claiming first-release support.
