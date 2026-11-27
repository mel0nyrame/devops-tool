# lfd: Inspect macOS uninstall leftovers

[简体中文](README.zh.md)

`lfd` is a read-only terminal tool for checking whether an application's `.app` bundle is present and inspecting possible artifacts. Run it from the workspace root in an interactive macOS terminal:

```sh
cargo run --locked --offline -p lfd
```

Enter an application name or exact Bundle ID, then press `Enter`. `lfd` does not take command-line arguments or accept paths as queries. If several app bundles match a name, choose one with `↑`/`↓` and `Enter`; filename-only artifacts are not assigned to either identity.

Use `↑`/`↓` to browse the results on the left and read the full path, match evidence, type, and risk on the right. `PgUp`/`PgDn` scroll the evidence. Press `i` if locations could not be checked to inspect their paths and errors separately. Press `q` or `Esc` to quit from results; press `Esc` to quit from the search prompt.

- **Related items**: a matching app bundle was found in the checked application directories.
- **Leftover candidates**: no matching app bundle was found in those directories. This does not prove that the application was uninstalled or that its files are safe to remove.
- **Possible matches**: no matching app bundle was found, but the application check could not be completed. A problem checking a data location alone does not change the application check status.

## Initial search scope

| Checked location | What `lfd` checks |
| --- | --- |
| `/Applications`, `/Applications/Utilities`, `/System/Applications`, `/System/Applications/Utilities`, `~/Applications` | Immediate `.app` directories. `lfd` matches the directory name without `.app`, or reads `Contents/Info.plist` (up to 4 MiB) for `CFBundleDisplayName`, `CFBundleName`, and `CFBundleIdentifier`. Readable fields are used; missing metadata does not prevent a directory name match. It does not verify or launch the executable. |
| `~/Library/Application Support`, `~/Library/Preferences`, `~/Library/Caches` | Immediate entry names only, including preference filenames with a `.plist` suffix removed. `lfd` does not open user data files or descend into matching directories. |

Application-name comparisons ignore case. Bundle IDs from metadata match exactly, including case. If an exact metadata ID is found, it takes precedence over name matches. An artifact can match the exact Bundle ID from local metadata or the query by filename. Dotted queries shaped like Bundle IDs require an exact filename match. A typed identifier without independent evidence is only a filename match; it does not establish ownership.

Symbolic links are not followed. Matching user-data links can appear in the list with their targets uninspected. Missing search directories have no entries; read errors and failed metadata checks are reported separately.

This initial scope does not check standalone commands, Homebrew records, `.pkg` receipts, or shared configuration contents. It does not search the whole home directory. There are no cleanup locations: `lfd` does not select, move, edit, or delete any items.
