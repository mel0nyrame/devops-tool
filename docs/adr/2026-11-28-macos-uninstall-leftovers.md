# Separate uninstall leftover discovery from cleanup on macOS

[简体中文](2026-11-28-macos-uninstall-leftovers.zh.md)

Status: implemented

## Problem

Uninstalling a program can leave downloads, settings, user data, and references in other programs. A name match can point to an unrelated file. A reference in a shared configuration file does not make that file a cleanup target. The tool needs to help the user inspect these results without treating a search as authorization to remove anything.

## Decision

The maintainer selected these initial constraints:

- Name the command `lfd` (leftover data). Scope the first version to macOS. Cover Homebrew formulae and casks, directly downloaded standalone executables, drag-installed `.app` bundles, and `.pkg` installations. Search for artifacts even when the installation source is unknown. Do not promise to reconstruct the actions of arbitrary installer scripts.
- Bound discovery to a fixed, documented set of locations. In addition to the main-program checks, include the usual user-level macOS application data, preference, and cache directories under `~/Library`, along with `~/.config`, `~/.local/share`, `~/.local/state`, `~/.cache`, and the current user's private temporary directory. Search file contents only in listed reference sources, such as selected OpenCode configuration files and the Pi extensions directory. Do not search the entire home directory, browser data, or shell history. Report locations that could not be checked separately from locations where nothing matched.
- When the main program is found, label matching artifacts as [related items](../../GLOSSARY.md#uninstall-leftover-inspection). If the required checks complete without finding a main program, label the items as leftover candidates and state the limits of the check. If a required check could not be completed, show possible matches instead. Show the matching evidence and the artifact type when known.
- Keep discovery read-only. Provide a separate interactive cleanup action for items the user selects. Use the evidence split view: browse and select items on the left; show the focused item's full path, evidence, and known risk on the right.

The maintainer also selected these discovery and cleanup rules:

- Accept an application name or an exact identifier, such as a bundle identifier, a Homebrew package name, or a `.pkg` package identifier. Do not accept arbitrary paths as search input. A typed identifier narrows discovery but is not evidence of ownership by itself.
- Use local evidence such as `.app` metadata, Homebrew records, and registered `.pkg` receipts. Do not parse downloaded installer archives or run installer or uninstaller scripts. Continue discovery when records are missing; keep results with insufficient evidence view-only.
- When the evidence distinguishes multiple applications with the same name, ask the user to pick one. Show available application identifiers and installation locations. Do not combine the results or treat a name-only match as proof of ownership.
- Use `.pkg` receipts as evidence and as leads to paths within the scan scope. Check whether the paths still exist. A receipt alone does not establish current ownership or make an item eligible for cleanup. Do not clean up receipts.
- Check the standard application directories for `.app` bundles and `PATH` for standalone commands. Also check the relevant Homebrew locations when there is evidence of a Homebrew installation. If the program type is unknown, check both application directories and `PATH`.
- Make results view-only if the main program is found or a required main-program check cannot be completed. A failure to read an unrelated location, such as a cache directory, does not by itself block cleanup of other verified paths.
- Allow selection only for paths within the cleanup scope with evidence that they are dedicated to the application. For the implemented cleanup scope, only the standard container directory `~/Library/Containers/<bundle-id>` is eligible. Its local macOS `.com.apple.containermanagerd.metadata.plist` must be verifiable and its `MCMMetadataIdentifier` must exactly match the directory Bundle ID. Missing, unreadable, mismatched, or linked metadata provides no authorization. This evidence applies to that container alone, not to same-named caches, preferences, other application data, or shared containers. A query, filename, or receipt alone is insufficient. Display shared references without deleting or editing them.
- Use the actual macOS account to authorize the container and locate its private Trash. Environment variables cannot extend cleanup scope. For container selection, combine the application, command, Homebrew, and receipt main-program checks; do not let exact identifier matching or selecting an identity hide a found program or incomplete required check. Matches in all other locations remain view-only.
- Allow a verified app-specific directory to be selected as a whole. State that the move includes its descendants. If the user selects both the directory and a descendant, review and move only the directory. Do not descend through symbolic links or offer standalone links for cleanup. Moving an eligible directory must not touch the targets of links it contains.
- Start with no items selected. Enter opens a review of the selected paths. The review shows the full paths, flags known effects on retained user data, and initially focuses Cancel. The user must explicitly choose Move to Trash and confirm it. Known user data does not require a separate confirmation.
- Before moving each confirmed path, recheck main-program status, the account, and the identity, type, ownership, and location of the container, its metadata, and its ancestor directories without following links. Skip the item and report why if it was replaced, became a symbolic link, or cannot be verified. Do not apply the old confirmation to the new object.
- Move confirmed paths to the account's private Trash on the same volume without overwriting. If a move cannot be verified or fails, leave the original item in place and report the reason. Do not request elevated privileges or fall back to permanent deletion.

The [tool README](../../crates/lfd/README.md) lists the discovery roots, reference sources, and cleanup roots. The command name does not select a distribution package name.

## Alternatives

- A read-only report avoids all cleanup effects, but requires the user to handle each confirmed item separately. The maintainer chose to include selective cleanup.
- Cleaning every search match would reduce interaction, but a name match can identify an unrelated item or a shared reference. The selected separation between discovery and cleanup rules out this approach.
- The prototypes compared a path table with details below it, an evidence split view, and a layout centered on a cleanup queue. The maintainer selected the split view. It keeps the current item's evidence visible while the user browses the list. A dedicated queue is not part of the selected layout.

## Consequences and risks

A bounded search can miss artifacts outside its scope. A missing program result is not proof of uninstallation. Moving an application-owned path can still affect retained settings, sessions, and other user data. Cleanup must not infer that evidence of a link is proof that moving the item is safe. Rechecking a path reduces the risk of acting on a replaced object, but a path can still change between the check and the move.

Local prototype commit `c098828` on `prototype/app-leftovers` records the layout comparison. The prototypes use synthetic data and simulate cleanup. The browser and macOS pseudoterminal checks exercise interaction only; they do not verify discovery or filesystem changes. Verify cleanup with isolated test fixtures. Only run read-only discovery checks against existing applications or user data.

Native macOS arm64 smoke checks exercise real discovery and Trash moves with isolated files. Regression checks cover staged cask apps linked by Bundle ID, executables found through registered receipts, and incomplete receipt checks.

This decision does not change the accepted [CLI layout](2026-11-20-cli-source-layout.md) or [distribution design](2026-11-20-npm-cli-distribution.md).
