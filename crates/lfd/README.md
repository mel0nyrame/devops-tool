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

## Initial search scope

| Checked location | What `lfd` checks |
| --- | --- |
| `/Applications`, `/Applications/Utilities`, `/System/Applications`, `/System/Applications/Utilities`, `~/Applications` | Immediate `.app` directories. `lfd` matches the directory name without `.app`, or reads `Contents/Info.plist` (up to 4 MiB) for `CFBundleDisplayName`, `CFBundleName`, and `CFBundleIdentifier`. Readable fields are used; missing metadata does not prevent a directory name match. It does not verify or launch the executable. |
| Directories on `PATH` | The entry with the queried command name. A command must be a regular file with at least one execute bit; `lfd` does not inspect or launch it. |
| `/opt/homebrew` and `/usr/local`: `Cellar/<formula>/<version>` and `Caskroom/<cask>` | Exact package name. For formulae, `lfd` reads `INSTALL_RECEIPT.json` and checks immediate `bin` and `sbin` executables in each version. Without a usable record, only a same-named executable is reported, as name evidence. For casks, `lfd` reads `.metadata/INSTALL_RECEIPT.json` and checks its `app` targets under the application directories (default `/Applications`), falling back to the staged bundle in that version's Caskroom directory, or its `binary` sources within that directory. Records are limited to 4 MiB each; other artifact types and paths outside those roots are not inspected. |
| `~/Library/Application Support`, `~/Library/Preferences`, `~/Library/Caches` | Immediate entry names only, including preference filenames with a `.plist` suffix removed. `lfd` does not open user data files or descend into matching directories. |

Application-name comparisons ignore case. Bundle IDs from metadata match exactly, including case. If an exact metadata ID is found, it takes precedence over name matches. An artifact can match the exact Bundle ID from local metadata or the query by filename. Dotted queries shaped like Bundle IDs require an exact filename match. A typed identifier without independent evidence is only a filename match; it does not establish ownership.

An exact Bundle ID from app metadata narrows the main-program check to applications. Otherwise `lfd` checks both applications and `PATH` for name matches, plus the standard Homebrew locations for the exact package name. A Homebrew record can identify the program to check, but does not by itself prove the program still exists. Missing records do not stop other checks. Homebrew global locations are view-only; `lfd` does not invoke Homebrew, access the network, or run scripts.

Symbolic links are not followed, including command links on `PATH`. Matching user-data links can appear in the list with their targets uninspected. Missing search directories have no entries; read errors and failed metadata checks are reported separately.

This initial scope does not check `.pkg` receipts or shared configuration contents. It does not search the whole home directory. There are no cleanup locations: `lfd` does not select, move, edit, or delete any items.
