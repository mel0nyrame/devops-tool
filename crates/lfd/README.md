# lfd: Inspect macOS uninstall leftovers

[简体中文](README.zh.md)

`lfd` is a terminal tool for checking whether an application's main program is present and inspecting possible artifacts. Discovery is read-only; confirmed cleanup can move an eligible app container to Trash. Run it from the workspace root in an interactive macOS terminal:

```sh
cargo run --locked --offline -p lfd
```

Enter an application name, exact Bundle ID, command name, exact Homebrew package name, or exact `.pkg` package ID, then press `Enter`. `lfd` does not take command-line arguments or accept paths as queries. If distinct identities match, choose one with `↑`/`↓` and `Enter`; filename-only artifacts are not assigned to the chosen identity.

Use `↑`/`↓` to browse the results on the left and read the full path, match evidence, type, and risk on the right. `PgUp`/`PgDn` scroll the evidence. Press `i` if locations could not be checked to inspect their paths and errors separately. Press `q` or `Esc` to quit from results; press `Esc` to quit from the search prompt.

- **Related items**: a matching main program was found in the checked locations.
- **Leftover candidates**: no matching main program was found in the checked locations. This does not prove that the application was uninstalled or that its files are safe to remove.
- **Possible matches**: no matching main program was found, but a required main-program check could not be completed. A problem checking an unrelated user data location does not make the main-program check incomplete.

## Search scope: main programs and entry names

| Checked location | What `lfd` checks |
| --- | --- |
| `/Applications`, `/Applications/Utilities`, `/System/Applications`, `/System/Applications/Utilities`, `~/Applications` | Immediate `.app` directories. `lfd` matches the directory name without `.app`, or reads `Contents/Info.plist` (up to 4 MiB) for `CFBundleDisplayName`, `CFBundleName`, and `CFBundleIdentifier`. Readable fields are used; missing metadata does not prevent a directory name match. It does not verify or launch the executable. |
| Directories on `PATH` | The entry with the queried command name. A command must be a regular file with at least one execute bit; `lfd` does not inspect or launch it. |
| `/opt/homebrew` and `/usr/local`: `Cellar/<formula>/<version>` and `Caskroom/<cask>` | Exact package name. For formulae, `lfd` reads `INSTALL_RECEIPT.json` and checks immediate `bin` and `sbin` executables in each version. Without a usable record, only a same-named executable is reported, as name evidence. For casks, `lfd` reads `.metadata/INSTALL_RECEIPT.json` and checks its `app` targets under the application directories (default `/Applications`), falling back to the staged bundle in that version's Caskroom directory, or its `binary` sources within that directory. Records are limited to 4 MiB each; other artifact types and paths outside those roots are not inspected. |
| `~/Library/Application Support`, `~/Library/Group Containers`, `~/Library/Caches`, `~/Library/Logs` | Immediate entry names only. |
| `~/Library/Containers` | Immediate entry names. For a matching app container directory, `lfd` checks the specific macOS metadata file described under [Cleanup scope](#cleanup-scope). |
| `~/Library/Preferences`, `~/Library/Saved Application State` | Immediate entry names only, with `.plist` and `.savedState` respectively removed before matching. |
| `~/.config`, `~/.local/share`, `~/.local/state`, `~/.cache` | Immediate entry names only. |
| The current user's macOS private temporary directory (`getconf DARWIN_USER_TEMP_DIR`) | Immediate entry names only. `lfd` does not use `$TMPDIR` or search the system-wide temporary directory. |

Application-name comparisons ignore case. Bundle IDs from metadata match exactly, including case. If an exact metadata ID is found, it takes precedence over name matches. An artifact can match the exact Bundle ID from local metadata or the query by filename. Dotted queries shaped like Bundle IDs require an exact filename match. A typed identifier alone does not establish ownership. A path-name hit without matching metadata remains a filename-only match.

An exact Bundle ID from app metadata narrows the main-program check to applications. Without an exact Bundle ID or registered `.pkg` package ID match, `lfd` checks both applications and `PATH` for name matches, plus the standard Homebrew locations for the exact package name. A Homebrew record can identify the program to check, but does not by itself prove the program still exists. Missing records do not stop other checks. These are discovery rules; cleanup uses the combined checks below. Homebrew global locations are view-only; `lfd` does not invoke Homebrew, access the network, or run scripts.

Name lookup does not descend into user data directories or follow symbolic links, including command links on `PATH`. Matching user-data links can appear in the list with their targets uninspected. Missing search directories have no entries; read errors and failed metadata checks are reported separately. `lfd` does not search the whole home directory, browser data, or shell history.

## Registered `.pkg` receipts

`lfd` reads the registered receipt plists under `/private/var/db/receipts` (up to 4 MiB each). It matches an exact, case-sensitive `PackageIdentifier` or the recorded installer filename without `.pkg`, ignoring case for the latter. An installer filename that repeats the package ID does not make the ID case-insensitive. `lfd` uses macOS `/usr/bin/lsbom` to read the matching receipt's `.bom`, using `InstallPrefixPath` to determine the recorded locations. It does not open downloaded installers or run scripts. Without a matching receipt, the other searches still run.

Only currently existing recorded paths within the search scope above can appear in results. For a recorded `.app` path in an application directory, `lfd` checks the current bundle and reads its `Contents/Info.plist` without following symbolic links. A readable Bundle ID matching the package ID, or current metadata matching the recorded size and checksum, can corroborate the bundle. This does not verify its executable or prove current ownership. If a recorded `.app` path cannot be checked or its current bundle cannot be corroborated, `lfd` excludes it from the results and marks the main program check incomplete. Press `i` for the path and reason. The receipt alone, including a missing recorded path, does not establish whether the main program is present or absent. Receipts and receipt-matched paths are view-only.

## Search scope: shared reference contents

Apart from the listed app bundle and app container metadata, Homebrew records, and `.pkg` receipt data, `lfd` reads contents only from these reference sources:

| Source | Files checked |
| --- | --- |
| OpenCode | `~/.config/opencode/cli.json` and `~/.config/opencode/tui.jsonc` only. |
| Pi | Regular files with a `.js` or `.ts` extension directly under `~/.pi/agent/extensions`. No nested extension directories or other file types. |

`lfd` looks for literal, bounded terms in UTF-8 text; names ignore case and Bundle IDs match exactly. It does not parse or execute the files. An occurrence can be a comment or incidental text, not necessarily an active reference. The evidence pane shows the path and matched term, not the file contents. The file remains view-only because a reference does not establish ownership.

`lfd` does not follow symbolic links to reference files or through their parent directories. Missing sources have no matches. Unreadable files, files larger than 1 MiB, and other failed checks are reported separately, not as successful searches with no matches.

## Cleanup scope

The normal build can move only `~/Library/Containers/<bundle-id>`, a whole application container directory in the current macOS account's home. Its `.com.apple.containermanagerd.metadata.plist` must be a readable, verifiable regular file (up to 4 MiB), with `MCMMetadataIdentifier` exactly matching the directory's Bundle ID, including case. `lfd` reads this one metadata file for ownership, not other container contents. Missing, unreadable, linked, mismatched, or otherwise unverifiable metadata or paths remain view-only. This evidence applies only to that container. Same-named caches, preferences, other application data, and shared `~/Library/Group Containers` remain view-only, as do shared references, receipts, global paths, and standalone symbolic links. A query, filename, or old receipt never authorizes cleanup.

To select a container, `lfd` must complete the main-program checks without finding a program. It combines standard app directory checks, exact command checks on `PATH` and in `/usr/bin`, `/bin`, `/usr/sbin`, `/sbin`, `/usr/local/bin`, and `/opt/homebrew/bin`, matching Homebrew installations, and registered `.pkg` receipt leads to app bundles in the application directories. An identity choice or exact receipt match does not cancel those checks. A missing record for an encountered Homebrew installation, an unreadable receipt, or an unverifiable recorded app can make the checks incomplete. An existing main program or an incomplete check prevents selection.

Nothing is selected by default. Use `Space` to toggle an eligible item and `Enter` to review the selected paths. The review lists full paths, notes that directories include all descendants, and flags known effects such as moving retained user data. A selected parent is listed instead of any selected descendant. In displayed paths, a newline appears as `\n`, and a literal backslash as `\\`. Press `Enter` on the default **Cancel**, or `Esc`, to return without moving anything. To move the reviewed paths, choose **Move to Trash** with `→` and press `Enter`. The results show which paths moved and why others were skipped or failed.

Before a move, `lfd` checks main-program status again and rechecks the account, ancestor directories, and the source and metadata identities, types, ownership, and scope without following links. It uses the actual macOS account, not `HOME`, `PATH`, `TMPDIR`, or a fixture override, to authorize paths and locate the account's private `~/.Trash`. The Trash must be accessible and on the same volume. A conflict or failed move leaves the original in place; `lfd` does not overwrite, copy across volumes, request elevated privileges, or fall back to permanent deletion. Rechecking reduces but does not eliminate the chance that a path changes between the check and the move.

## Isolated cleanup verification

The `test-fixtures` build exercises the same container metadata and move checks under an isolated home and its private `.Trash`. Legacy preview fixtures also exercise parent/child deduplication and individual file moves using synthetic registration: `~/Library/Application Support/<bundle-id>` and its immediate regular files or directories, `~/Library/Preferences/<bundle-id>.plist`, and `~/Library/Caches/<bundle-id>`. These fixture-only paths and synthetic records never authorize cleanup in the normal build. Cleanup smoke tests do not move existing applications or user data.
