# lfd: Inspect macOS uninstall leftovers

[简体中文](README.zh.md)

`lfd` is a read-only terminal tool for checking whether an application's main program is present and inspecting possible artifacts. Run it from the workspace root in an interactive macOS terminal:

```sh
cargo run --locked --offline -p lfd
```

Enter an application name, exact Bundle ID, command name, or exact Homebrew package name, then press `Enter`. `lfd` does not take command-line arguments or accept paths as queries. If distinct applications, commands, or Homebrew packages match, choose one with `↑`/`↓` and `Enter`; filename-only artifacts are not assigned to the chosen identity.

Use `↑`/`↓` to browse the results on the left and read the full path, match evidence, type, and risk on the right. `PgUp`/`PgDn` scroll the evidence. Press `i` if locations could not be checked to inspect their paths and errors separately. Press `q` or `Esc` to quit from results; press `Esc` to quit from the search prompt.

- **Related items**: a matching main program was found in the checked locations.
- **Leftover candidates**: no matching main program was found in the checked locations. This does not prove that the application was uninstalled or that its files are safe to remove.
- **Possible matches**: no matching main program was found, but a required main-program check could not be completed. A problem checking a user data location alone does not change this status.

## Search scope: main programs and entry names

| Checked location | What `lfd` checks |
| --- | --- |
| `/Applications`, `/Applications/Utilities`, `/System/Applications`, `/System/Applications/Utilities`, `~/Applications` | Immediate `.app` directories. `lfd` matches the directory name without `.app`, or reads `Contents/Info.plist` (up to 4 MiB) for `CFBundleDisplayName`, `CFBundleName`, and `CFBundleIdentifier`. Readable fields are used; missing metadata does not prevent a directory name match. It does not verify or launch the executable. |
| Directories on `PATH` | The entry with the queried command name. A command must be a regular file with at least one execute bit; `lfd` does not inspect or launch it. |
| `/opt/homebrew` and `/usr/local`: `Cellar/<formula>/<version>` and `Caskroom/<cask>` | Exact package name. For formulae, `lfd` reads `INSTALL_RECEIPT.json` and checks immediate `bin` and `sbin` executables in each version. Without a usable record, only a same-named executable is reported, as name evidence. For casks, `lfd` reads `.metadata/INSTALL_RECEIPT.json` and checks its `app` targets under the application directories (default `/Applications`), falling back to the staged bundle in that version's Caskroom directory, or its `binary` sources within that directory. Records are limited to 4 MiB each; other artifact types and paths outside those roots are not inspected. |
| `~/Library/Application Support`, `~/Library/Containers`, `~/Library/Group Containers`, `~/Library/Caches`, `~/Library/Logs` | Immediate entry names only. |
| `~/Library/Preferences`, `~/Library/Saved Application State` | Immediate entry names only, with `.plist` and `.savedState` respectively removed before matching. |
| `~/.config`, `~/.local/share`, `~/.local/state`, `~/.cache` | Immediate entry names only. |
| The current user's macOS private temporary directory (`getconf DARWIN_USER_TEMP_DIR`) | Immediate entry names only. `lfd` does not use `$TMPDIR` or search the system-wide temporary directory. |

Application-name comparisons ignore case. Bundle IDs from metadata match exactly, including case. If an exact metadata ID is found, it takes precedence over name matches. An artifact can match the exact Bundle ID from local metadata or the query by filename. Dotted queries shaped like Bundle IDs require an exact filename match. A typed identifier alone does not establish ownership. A path-name hit without matching metadata remains a filename-only match.

An exact Bundle ID from app metadata narrows the main-program check to applications. Otherwise `lfd` checks both applications and `PATH` for name matches, plus the standard Homebrew locations for the exact package name. A Homebrew record can identify the program to check, but does not by itself prove the program still exists. Missing records do not stop other checks. Homebrew global locations are view-only; `lfd` does not invoke Homebrew, access the network, or run scripts.

Name lookup does not descend into user data directories or follow symbolic links, including command links on `PATH`. Matching user-data links can appear in the list with their targets uninspected. Missing search directories have no entries; read errors and failed metadata checks are reported separately. This version does not check `.pkg` receipts or search the whole home directory, browser data, or shell history.

## Search scope: shared reference contents

Apart from the app bundle metadata and Homebrew records listed above, `lfd` reads contents only from these sources:

| Source | Files checked |
| --- | --- |
| OpenCode | `~/.config/opencode/cli.json` and `~/.config/opencode/tui.jsonc` only. |
| Pi | Regular files with a `.js` or `.ts` extension directly under `~/.pi/agent/extensions`. No nested extension directories or other file types. |

`lfd` looks for literal, bounded terms in UTF-8 text; names ignore case and Bundle IDs match exactly. It does not parse or execute the files. An occurrence can be a comment or incidental text, not necessarily an active reference. The evidence pane shows the path and matched term, not the file contents. The file remains view-only because a reference does not establish ownership.

`lfd` does not follow symbolic links to reference files or through their parent directories. Missing sources have no matches. Unreadable files, files larger than 1 MiB, and other failed checks are reported separately, not as successful searches with no matches.

There are no cleanup locations. Finding a name or a shared reference does not authorize cleanup; `lfd` does not select, move, edit, or delete any items.
