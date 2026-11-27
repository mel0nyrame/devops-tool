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

## Initial search scope

| Checked location | What `lfd` checks |
| --- | --- |
| `/Applications`, `/Applications/Utilities`, `/System/Applications`, `/System/Applications/Utilities`, `~/Applications` | Immediate `.app` directories. `lfd` matches the directory name without `.app`, or reads `Contents/Info.plist` (up to 4 MiB) for `CFBundleDisplayName`, `CFBundleName`, and `CFBundleIdentifier`. Readable fields are used; missing metadata does not prevent a directory name match. It does not verify or launch the executable. |
| `~/Library/Application Support`, `~/Library/Preferences`, `~/Library/Caches` | Immediate entry names only, including preference filenames with a `.plist` suffix removed. `lfd` does not open user data files or descend into matching directories. |

Application-name comparisons ignore case. Bundle IDs from metadata match exactly, including case. If an exact metadata ID is found, it takes precedence over name matches. An artifact can match the exact Bundle ID from local metadata or the query by filename. Dotted queries shaped like Bundle IDs require an exact filename match. A typed identifier without independent evidence is only a filename match; it does not establish ownership.

Symbolic links are not followed. Matching user-data links can appear in the list with their targets uninspected. Missing search directories have no entries; read errors and failed metadata checks are reported separately.

## Registered `.pkg` receipts

`lfd` reads the registered receipt plists under `/private/var/db/receipts` (up to 4 MiB each). It matches an exact, case-sensitive `PackageIdentifier` or the recorded installer filename without `.pkg`, ignoring case for the latter. An installer filename that repeats the package ID does not make the ID case-insensitive. `lfd` uses macOS `/usr/bin/lsbom` to read the matching receipt's `.bom`, using `InstallPrefixPath` to determine the recorded locations. It does not open downloaded installers or run scripts. Without a matching receipt, the other searches still run.

Only currently existing paths within the search scope above are shown. For a recorded `.app` in those directories, `lfd` reads the current `Contents/Info.plist` without following symbolic links. A readable Bundle ID matching the package ID, or current metadata matching the recorded size and checksum, can corroborate the bundle. This does not verify its executable or prove current ownership. If the path now contains an app whose identity cannot be corroborated, the check is incomplete; press `i` for the path and reason. A missing recorded path says nothing about other locations. Receipts and paths matched only by a receipt are view-only.

This scope does not check standalone commands, Homebrew records, or shared configuration contents. It does not search the whole home directory. There are no cleanup locations: `lfd` does not select, move, edit, or delete any items.
