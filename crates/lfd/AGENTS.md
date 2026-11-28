# AGENTS.md

## lfd guidance

- **Behavior:** Read the [tool README](README.md) and the [macOS uninstall leftover ADR](../../docs/adr/2026-11-28-macos-uninstall-leftovers.md). Use isolated files and synthetic evidence for PTY tests. Never clean up existing applications or user data.
- **Native smoke check:** From the workspace root on macOS, run:
  ```sh
  cargo build --locked --offline -p lfd
  python3 .github/scripts/lfd-smoke.py target/debug/lfd DefaultBuildTests
  cargo build --locked --offline -p lfd --features test-fixtures
  python3 .github/scripts/lfd-smoke.py target/debug/lfd
  ```
  The second build requires `LFD_TEST_ROOT` at runtime. The script supplies a real temporary directory with a `home` subdirectory, mapping both system and home search roots into the fixture. A missing or invalid fixture root must fail without falling back to real search roots. The normal build ignores `LFD_TEST_ROOT`.
- **Preview fixtures:** The isolated build reads `var/db/lfd-fixtures/<bundle-id>.plist` and `<bundle-id>.checks` under the fixture root to represent OS registration and supplemental main-program responses. It still scans the isolated application directories and evaluates the actual artifact path and type. These records are not available in the normal build.
