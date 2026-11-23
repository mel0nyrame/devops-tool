# rmem

[简体中文](README.zh.md) · [Project overview](../../README.md)

`rmem` is a read-only terminal monitor for local processes, RAM, and swap. It refreshes once per second and initially sorts processes by resident memory, highest first.

## Install

Install the single-tool npm package with:

```sh
npm install -g @melonyrame/rmem
```

See the [project installation instructions](../../README.md#install) for other installation methods.

## Usage

From the workspace root, in an interactive terminal:

```sh
cargo run --locked -p rmem
```

| Key | Action |
| --- | --- |
| `q` | Quit from the process list |
| `↑` / `↓`, `k` / `j` | Select previous or next process |
| `gg` / `G`, `Home` / `End` | Select the first or last process in the current list |
| `m` / `c` / `p` / `t` | Sort by memory, CPU usage, PID, or CPU time |
| `/` | Search by PID, user, name, or command |
| `Enter` | Apply a search or open the selected process's details |
| `Esc` | Close details or clear the search |
| `r` | Refresh now |

For `gg`, press lowercase `g` twice without pressing another key in between. There is no time limit. `gg` and `G` work only while browsing the process list. They use the current search results and sort order.

## Metrics

- `MEM` is resident process memory: RSS on Unix and working set on Windows. `MEM%` divides it by total physical RAM; `ΔMEM` is the change from the previous refresh.
- `CPU%` can exceed 100% on a multicore system. `TIME` is accumulated process CPU time, not elapsed time.
- `COMMAND` shows the command line when available, then falls back to the executable path or process name.
- Process visibility and reported values depend on the OS and permissions. A missing user or memory percentage is shown as `-`. A RAM or swap total reported as zero is shown as `n/a`. The backend does not always distinguish unavailable or stale process readings from measured values.
- On Windows, `SWAP (EST.)` estimates swap use and capacity from system commit and physical RAM. It does not measure page-file occupancy.
