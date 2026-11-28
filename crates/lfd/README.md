# lfd: Inspect macOS uninstall leftovers

[简体中文](README.zh.md)

`lfd` is a read-only terminal tool for checking whether an application's `.app` bundle is present and inspecting possible artifacts. Run it from the workspace root in an interactive macOS terminal:

```sh
cargo run --locked --offline -p lfd
```

Enter an application name, exact Bundle ID, or exact `.pkg` package ID, then press `Enter`. `lfd` does not take command-line arguments or accept paths as queries. If several application identities match, choose one with `↑`/`↓` and `Enter`; filename-only artifacts are not assigned to any identity.

Use `↑`/`↓` to browse the results on the left and read the full path, match evidence, type, and risk on the right. `PgUp`/`PgDn` scroll the evidence. Press `i` if locations could not be checked to inspect their paths and errors separately. Press `q` or `Esc` to quit from results; press `Esc` to quit from the search prompt.

- **Related items**: a matching app bundle was found in the checked application directories.
- **Leftover candidates**: no matching app bundle was found in those directories. This does not prove that the application was uninstalled or that its files are safe to remove.
- **Possible matches**: the required application check could not be completed. A problem checking a data location or reading a receipt alone does not change the application check status.

## Search scope: applications and entry names

| Checked location | What `lfd` checks |
| --- | --- |
| `/Applications`, `/Applications/Utilities`, `/System/Applications`, `/System/Applications/Utilities`, `~/Applications` | Immediate `.app` directories. `lfd` matches the directory name without `.app`, or reads `Contents/Info.plist` (up to 4 MiB) for `CFBundleDisplayName`, `CFBundleName`, and `CFBundleIdentifier`. Readable fields are used; missing metadata does not prevent a directory name match. It does not verify or launch the executable. |
| `~/Library/Application Support`, `~/Library/Containers`, `~/Library/Group Containers`, `~/Library/Caches`, `~/Library/Logs` | Immediate entry names only. |
| `~/Library/Preferences`, `~/Library/Saved Application State` | Immediate entry names only, with `.plist` and `.savedState` respectively removed before matching. |
| `~/.config`, `~/.local/share`, `~/.local/state`, `~/.cache` | Immediate entry names only. |
| The current user's macOS private temporary directory (`getconf DARWIN_USER_TEMP_DIR`) | Immediate entry names only. `lfd` does not use `$TMPDIR` or search the system-wide temporary directory. |

Application-name comparisons ignore case. Bundle IDs from metadata match exactly, including case. If an exact metadata ID is found, it takes precedence over name matches. An artifact can match the exact Bundle ID from local metadata or the query by filename. Dotted queries shaped like Bundle IDs require an exact filename match. A typed identifier alone does not establish ownership. A path-name hit without matching metadata remains a filename-only match.

Name lookup does not descend into user data directories. Matching user-data symbolic links can appear in the list, but their targets are not inspected. This version does not check standalone commands or Homebrew records. It does not search the whole home directory, browser data, or shell history.

## Registered `.pkg` receipts

`lfd` reads the registered receipt plists under `/private/var/db/receipts` (up to 4 MiB each). It matches an exact, case-sensitive `PackageIdentifier` or the recorded installer filename without `.pkg`, ignoring case for the latter. An installer filename that repeats the package ID does not make the ID case-insensitive. `lfd` uses macOS `/usr/bin/lsbom` to read the matching receipt's `.bom`, using `InstallPrefixPath` to determine the recorded locations. It does not open downloaded installers or run scripts. Without a matching receipt, the other searches still run.

Only currently existing recorded paths within the search scope above can appear in results. For a recorded `.app` path in an application directory, `lfd` checks the current bundle and reads its `Contents/Info.plist` without following symbolic links. A readable Bundle ID matching the package ID, or current metadata matching the recorded size and checksum, can corroborate the bundle. This does not verify its executable or prove current ownership. If a recorded `.app` path cannot be checked or its current bundle cannot be corroborated, `lfd` excludes it from the results and marks the main program check incomplete. Press `i` for the path and reason. The receipt alone, including a missing recorded path, does not establish whether the main program is present or absent. Receipts and receipt-matched paths are view-only.

## Search scope: shared reference contents

Apart from the app bundle metadata and receipt data listed above, `lfd` reads contents only from these sources:

| Source | Files checked |
| --- | --- |
| OpenCode | `~/.config/opencode/cli.json` and `~/.config/opencode/tui.jsonc` only. |
| Pi | Regular files with a `.js` or `.ts` extension directly under `~/.pi/agent/extensions`. No nested extension directories or other file types. |

`lfd` looks for literal, bounded terms in UTF-8 text; names ignore case and Bundle IDs match exactly. It does not parse or execute the files. An occurrence can be a comment or incidental text, not necessarily an active reference. The evidence pane shows the path and matched term, not the file contents. The file remains view-only because a reference does not establish ownership.

`lfd` does not follow symbolic links to reference files or through their parent directories. Missing sources have no matches. Unreadable files, files larger than 1 MiB, and other failed checks are reported separately, not as successful searches with no matches.

There are no cleanup locations. Finding a name or a shared reference does not authorize cleanup; `lfd` does not select, move, edit, or delete any items.
