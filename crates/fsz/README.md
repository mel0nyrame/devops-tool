# fsz

[简体中文](README.zh.md) · [Project overview](../../README.md)

`fsz` prints the size of a file or a size breakdown of a directory's immediate contents.

## Install

Use the [project installation instructions](../../README.md#install) to install from source or a public release. After a public version is available, install the single-tool npm package with:

```sh
npm install -g @mel0nyrame/fsz
```

## Usage

Run from the workspace root:

```sh
cargo run --locked -p fsz -- .
cargo run --locked -p fsz -- "path/to/file or directory"
```

The path defaults to the current directory. For a directory, the table lists its immediate entries; a subdirectory's size includes its scanned descendants.

## Measurement

- Sizes are logical bytes, not disk space allocated. Distinct hard-link entries are counted separately.
- An explicitly supplied symbolic link is resolved. Recursive scans skip symbolic links and special files. On macOS and Linux they stay on the starting device; on Windows they skip directory reparse points, including junctions and volume mount points.
- The scan is not a snapshot. If another process replaces a directory between inspection and descent, boundary checks may not apply to the replacement.
- A `~` before a size means the scan could measure only part of that entry. The total includes measured bytes from partial entries.
