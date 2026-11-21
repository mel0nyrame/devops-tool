# AGENTS.md

## rmem guidance

- **Metrics changes:** Read [metrics](README.md#metrics). Collect and derive process metrics in `src/process.rs`; render them in `src/ui.rs`. Preserve the distinction between unavailable values and measured zeros when the backend exposes it. Check both the process table and the details view for display changes.
- **Interaction changes:** Keep monitoring read-only. Handle input and selection in `src/app.rs`. Preserve selection by PID after refresh or sorting while that PID remains visible.
- **Smoke check:** Run `cargo run --locked --offline -p rmem` from the workspace root in an interactive terminal. Search for a known running PID with `/` and `Enter`. Select the matching row and press `Enter` again; verify that the details show the same PID. Press `Esc` to return to the list, then `q`; verify that the shell prompt returns and terminal input echoes normally.
- **Windows smoke automation:** From the workspace root, run `cargo build --locked --offline -p rmem`, set `WINPTY_DLL` to the existing `winpty.dll` path, then run [the WinPTY probe](../../.github/scripts/rmem-windows-smoke.py) with native Windows Python: `python .github/scripts/rmem-windows-smoke.py target/debug/rmem.exe`. A Git Bash installation with `/usr/bin/winpty.dll` can supply the path via `export WINPTY_DLL=$(cygpath -w /usr/bin/winpty.dll)`. If prerequisites are missing, report the automated check as blocked or run the manual smoke check.
