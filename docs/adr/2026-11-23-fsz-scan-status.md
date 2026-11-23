# Report `fsz` scan activity without a percentage

[简体中文](2026-11-23-fsz-scan-status.zh.md)

Status: implemented

## Problem

`fsz` prints the directory breakdown after traversal. A long scan gives users no indication that it is still advancing. The scanner discovers nested entries during traversal, so it does not know the total entry count in advance.

## Decision

The maintainer selected a single-pass scan status that reports the cumulative number of visited entries, without a percentage or an estimated time remaining. The status does not change the [measurement contract](../../crates/fsz/README.md#measurement).

The maintainer also selected a temporary status line on standard error, shown only when standard error is a terminal. Clear the line when the scan returns, including on a scan error. Keep the final breakdown on standard output.

Count each directory entry as the scan discovers it, including entries later skipped or found unreadable. Do not count the scan root or enumeration errors that yield no entry. The count describes entries visited, not entries successfully measured.

Show the first status update when the scan discovers an entry at least 250 milliseconds after scanning starts. Refresh only when the scan discovers another entry and at least 250 milliseconds have passed since the previous update. The scanner and status renderer implement this design without adding a dependency. This topic is independent of the workspace layout and distribution decisions.

## Alternatives

- Count entries in a preliminary traversal: adds directory work before measurement. The count can also become stale if the directory changes between traversals.
- Use scan-root children as a percentage denominator: weights a file and a directory with many descendants equally, so the percentage can misrepresent the work remaining.

## Consequences and risks

The count can show that traversal is advancing, but it cannot show the fraction complete or predict when measurement will finish. An individual slow filesystem operation can leave the count unchanged until traversal resumes. The status renderer overwrites a single terminal line. If a terminal wraps the status, clearing may leave text on screen.

On macOS arm64, the locked offline workspace tests and `fsz` symlink smoke checks passed. A pseudo-terminal check of the built CLI confirmed that a long scan shows and clears the status, a fast scan stays quiet, and redirecting standard error suppresses the status without changing standard output. Native Linux and Windows terminal behavior for this feature has not been verified.
