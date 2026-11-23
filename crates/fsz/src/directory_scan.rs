use std::fs;
use std::io;
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
#[cfg(windows)]
use std::os::windows::fs::MetadataExt;
use std::path::Path;

#[derive(Debug, Clone)]
pub(crate) struct EntryInfo {
    pub(crate) name: String,
    pub(crate) kind: EntryKind,
}

#[derive(Debug, Clone)]
pub(crate) enum EntryKind {
    Exact {
        size: u64,
        is_dir: bool,
    },

    Partial {
        size: u64,
        is_dir: bool,
    },

    PermissionDenied {
        is_dir: bool,
    },
}

impl EntryInfo {
    pub(crate) fn measured_size(&self) -> Option<u64> {
        match self.kind {
            EntryKind::Exact { size, .. }
            | EntryKind::Partial { size, .. } => Some(size),

            EntryKind::PermissionDenied { .. } => None,
        }
    }

    pub(crate) fn is_dir(&self) -> bool {
        match self.kind {
            EntryKind::Exact { is_dir, .. }
            | EntryKind::Partial { is_dir, .. }
            | EntryKind::PermissionDenied { is_dir } => is_dir,
        }
    }
}

#[derive(Debug)]
enum MeasureResult {
    Exact(u64),
    Partial(u64),
    MountedFs,
    PermissionDenied,
    Symlink,
    Special,
}

struct ScanBoundary {
    #[cfg(unix)]
    device: u64,
}

impl ScanBoundary {
    fn new(_root: &fs::Metadata) -> Self {
        Self {
            #[cfg(unix)]
            device: _root.dev(),
        }
    }

    fn excludes(&self, metadata: &fs::Metadata) -> bool {
        #[cfg(unix)]
        {
            metadata.dev() != self.device
        }
        #[cfg(windows)]
        {
            // Skip junctions, volume mount points, and other directory reparse points.
            const DIRECTORY: u32 = 0x10;
            const REPARSE_POINT: u32 = 0x400;
            let mask = DIRECTORY | REPARSE_POINT;
            metadata.file_attributes() & mask == mask
        }
    }
}

pub(crate) fn scan_directory(
    path: &Path,
    visit: &mut impl FnMut(),
) -> io::Result<Vec<EntryInfo>> {
    let boundary = ScanBoundary::new(&fs::metadata(path)?);

    let mut entries = Vec::new();

    for result in fs::read_dir(path)? {
        let entry = match result {
            Ok(entry) => entry,

            Err(err)
                if err.kind() == io::ErrorKind::PermissionDenied =>
            {
                continue;
            }

            Err(err) => {
                return Err(err);
            }
        };

        visit();
        let entry_path = entry.path();

        let name = entry
            .file_name()
            .to_string_lossy()
            .into_owned();

        // This probe supplies the row's type; measure_path probes again for measurement.
        let metadata = match fs::symlink_metadata(&entry_path) {
            Ok(metadata) => metadata,

            Err(err)
                if err.kind() == io::ErrorKind::PermissionDenied =>
            {
                entries.push(EntryInfo {
                    name,
                    kind: EntryKind::PermissionDenied {
                        is_dir: false,
                    },
                });

                continue;
            }

            Err(_) => {
                continue;
            }
        };

        let is_dir = metadata.is_dir();

        match measure_path(&entry_path, &boundary, visit) {
            Ok(MeasureResult::Exact(size)) => {
                entries.push(EntryInfo {
                    name,
                    kind: EntryKind::Exact {
                        size,
                        is_dir,
                    },
                });
            }

            Ok(MeasureResult::Partial(size)) => {
                entries.push(EntryInfo {
                    name,
                    kind: EntryKind::Partial {
                        size,
                        is_dir,
                    },
                });
            }

            Ok(MeasureResult::PermissionDenied) => {
                entries.push(EntryInfo {
                    name,
                    kind: EntryKind::PermissionDenied {
                        is_dir,
                    },
                });
            }

            /*
             * Don't show these in the normal output.
             *
             * This keeps `/` clean and avoids bogus values from
             * virtual filesystems such as /proc.
             */
            Ok(
                MeasureResult::MountedFs
                | MeasureResult::Symlink
                | MeasureResult::Special,
            ) => {}

            Err(err)
                if err.kind() == io::ErrorKind::PermissionDenied =>
            {
                entries.push(EntryInfo {
                    name,
                    kind: EntryKind::PermissionDenied {
                        is_dir,
                    },
                });
            }

            Err(_) => {
                // Ignore unreadable/transient special entries.
            }
        }
    }

    Ok(entries)
}

fn measure_path(
    path: &Path,
    boundary: &ScanBoundary,
    visit: &mut impl FnMut(),
) -> io::Result<MeasureResult> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,

        Err(err)
            if err.kind() == io::ErrorKind::PermissionDenied =>
        {
            return Ok(
                MeasureResult::PermissionDenied
            );
        }

        Err(err) => {
            return Err(err);
        }
    };

    let file_type = metadata.file_type();

    /*
     * Never follow symlinks during recursive scans.
     *
     * This also avoids cycles such as:
     *
     * a -> b
     * b -> a
     */
    if file_type.is_symlink() {
        return Ok(MeasureResult::Symlink);
    }

    if boundary.excludes(&metadata) {
        return Ok(MeasureResult::MountedFs);
    }

    if metadata.is_file() {
        return Ok(
            MeasureResult::Exact(
                metadata.len()
            )
        );
    }

    if metadata.is_dir() {
        return directory_size(path, boundary, visit);
    }

    Ok(MeasureResult::Special)
}

fn directory_size(
    path: &Path,
    boundary: &ScanBoundary,
    visit: &mut impl FnMut(),
) -> io::Result<MeasureResult> {
    let read_dir = match fs::read_dir(path) {
        Ok(read_dir) => read_dir,

        Err(err)
            if err.kind() == io::ErrorKind::PermissionDenied =>
        {
            return Ok(
                MeasureResult::PermissionDenied
            );
        }

        Err(err) => {
            return Err(err);
        }
    };

    let mut total = 0_u64;
    let mut incomplete = false;

    for result in read_dir {
        let entry = match result {
            Ok(entry) => entry,

            Err(err)
                if err.kind() == io::ErrorKind::PermissionDenied =>
            {
                incomplete = true;
                continue;
            }

            Err(_) => {
                incomplete = true;
                continue;
            }
        };

        visit();
        let child_path = entry.path();

        match measure_path(&child_path, boundary, visit) {
            Ok(MeasureResult::Exact(size)) => {
                total =
                    total.saturating_add(size);
            }

            Ok(MeasureResult::Partial(size)) => {
                total =
                    total.saturating_add(size);

                incomplete = true;
            }

            /*
             * Mounted filesystems, symlinks and special
             * filesystem objects intentionally contribute
             * nothing to the size.
             */
            Ok(
                MeasureResult::MountedFs
                | MeasureResult::Symlink
                | MeasureResult::Special,
            ) => {}

            Ok(MeasureResult::PermissionDenied) => {
                incomplete = true;
            }

            Err(err)
                if err.kind() == io::ErrorKind::PermissionDenied =>
            {
                incomplete = true;
            }

            Err(_) => {
                incomplete = true;
            }
        }
    }

    if incomplete {
        Ok(MeasureResult::Partial(total))
    } else {
        Ok(MeasureResult::Exact(total))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visits_each_discovered_entry_once() -> io::Result<()> {
        struct Fixture(std::path::PathBuf);

        impl Drop for Fixture {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }

        let path = std::env::temp_dir().join(format!("fsz-visits-{}", std::process::id()));
        fs::create_dir(&path)?;
        let fixture = Fixture(path);
        let root = fixture.0.join("root");
        fs::create_dir_all(root.join("sub"))?;
        fs::write(root.join("first"), [0; 3])?;
        fs::write(root.join("sub/second"), [0; 5])?;

        #[cfg(unix)]
        {
            let outside = fixture.0.join("outside");
            fs::create_dir(&outside)?;
            fs::write(outside.join("extra"), [0; 13])?;
            std::os::unix::fs::symlink(&outside, root.join("sub/link"))?;
        }

        let mut visits = 0;
        let entries = scan_directory(&root, &mut || visits += 1)?;
        assert_eq!(visits, if cfg!(unix) { 4 } else { 3 });
        assert_eq!(entries.len(), 2);
        let total: u64 = entries.iter().filter_map(EntryInfo::measured_size).sum();
        assert_eq!(total, 8);
        Ok(())
    }
}
