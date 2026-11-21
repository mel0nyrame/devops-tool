# AGENTS.md

## fsz guidance

- **Scan changes:** Preserve the [measurement contract](README.md#measurement). Build `ScanBoundary` from the resolved scan root. Use `symlink_metadata` for discovered entries so link and boundary checks happen before descent.
- **Smoke check:** From the workspace root, run `cargo run --locked --offline -p fsz -- "$path"` for each input. Put a 3-byte file at the scan root and a 5-byte file in a subdirectory. In that subdirectory, add a directory symlink to a directory outside the scan root containing a 13-byte file. The scan root must report `TOTAL  8 B`; passing the symlink itself must report `TOTAL  13 B`.
- **Windows smoke check:** Repeat the nested-link case with a junction and expect the same 8-byte total. Test the explicit path with a directory symlink, not a junction. Report any link case you cannot set up as blocked.
