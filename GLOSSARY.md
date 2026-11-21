# Local System Tools

`fsz` measures files and directories. `rmem` provides a read-only view of local processes and their resource usage.

## Language

### fsz

**Logical size**:
The length of a file's contents in bytes, or the sum of the logical sizes of the file entries counted in a directory scan.
_Avoid_: Allocated size, disk space used

**Scan root**:
The directory where measurement begins. For an explicitly selected directory symlink, this is the target directory.

**Scan boundary**:
The limit of a directory measurement at nested symbolic links, Windows directory junctions, and filesystems other than the one containing the scan root.

### rmem

**Resident process memory**:
The part of a process's memory currently resident in physical RAM, reported as RSS or as the Windows working set. It can include shared memory.
_Avoid_: Private memory, virtual memory size

**Unavailable metric**:
A metric for which there is not enough information to report a meaningful value, distinct from a measured zero.

### Distribution

**Suite package**:
A published package that installs a group of tool commands together.
_Avoid_: Repository, workspace

**Single-tool package**:
A published package that installs one tool command independently.
