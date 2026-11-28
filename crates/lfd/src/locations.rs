use std::{
    fs::{self, OpenOptions},
    io::{self, Read},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
};

pub struct Locations {
    pub home: PathBuf,
    pub system_root: PathBuf,
}

impl Locations {
    pub fn application_dirs(&self) -> [PathBuf; 5] {
        [
            self.system_root.join("Applications"),
            self.system_root.join("Applications/Utilities"),
            self.system_root.join("System/Applications"),
            self.system_root.join("System/Applications/Utilities"),
            self.home.join("Applications"),
        ]
    }

    pub fn homebrew_prefixes(&self) -> [PathBuf; 2] {
        [
            self.system_root.join("opt/homebrew"),
            self.system_root.join("usr/local"),
        ]
    }

    pub fn path_dirs(&self) -> Vec<PathBuf> {
        let Some(path) = std::env::var_os("PATH") else {
            return Vec::new();
        };
        let directories = std::env::split_paths(&path);
        #[cfg(feature = "test-fixtures")]
        let directories = directories.filter(|path| {
            path.starts_with(&self.system_root)
                && !path
                    .components()
                    .any(|component| component == std::path::Component::ParentDir)
        });
        directories.collect()
    }

    pub fn current() -> io::Result<Self> {
        #[cfg(feature = "test-fixtures")]
        {
            let root = std::env::var_os("LFD_TEST_ROOT")
                .map(PathBuf::from)
                .ok_or_else(|| io::Error::other("test-fixtures requires LFD_TEST_ROOT"))?;
            if !root.is_absolute()
                || root == Path::new("/")
                || root
                    .components()
                    .any(|component| component == std::path::Component::ParentDir)
            {
                return Err(io::Error::other(
                    "LFD_TEST_ROOT must be an absolute fixture directory, not the filesystem root",
                ));
            }
            let home = root.join("home");
            for path in [&root, &home] {
                let metadata = check_path(path).map_err(|error| {
                    io::Error::new(
                        error.kind(),
                        format!(
                            "LFD_TEST_ROOT fixture directory {}: {error}",
                            path.display()
                        ),
                    )
                })?;
                if !metadata.is_dir() {
                    return Err(io::Error::other(format!(
                        "LFD_TEST_ROOT fixture directory is not a directory: {}",
                        path.display()
                    )));
                }
            }
            Ok(Self {
                home,
                system_root: root,
            })
        }
        #[cfg(not(feature = "test-fixtures"))]
        {
            let home = std::env::home_dir()
                .filter(|path| path.is_absolute())
                .ok_or_else(|| {
                    io::Error::other("could not determine an absolute home directory")
                })?;
            Ok(Self {
                home,
                system_root: PathBuf::from("/"),
            })
        }
    }
}

pub fn read_metadata_file(path: &Path, label: &str) -> io::Result<Vec<u8>> {
    const MAX_METADATA_SIZE: u64 = 4 * 1024 * 1024;
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW_ANY | libc::O_NONBLOCK)
        .open(path)?;
    if !file.metadata()?.is_file() {
        return Err(io::Error::other(format!("{label} is not a regular file")));
    }
    let mut bytes = Vec::new();
    file.take(MAX_METADATA_SIZE + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_METADATA_SIZE {
        return Err(io::Error::other(format!("{label} exceeds 4 MiB")));
    }
    Ok(bytes)
}

pub fn check_path(path: &Path) -> io::Result<fs::Metadata> {
    let mut components = path.ancestors().collect::<Vec<_>>();
    let mut metadata = fs::symlink_metadata(components.pop().unwrap_or(path))?;
    for component in components.into_iter().rev() {
        metadata = fs::symlink_metadata(component)?;
        if metadata.is_symlink() {
            return Err(io::Error::other(format!(
                "symbolic link not followed: {}",
                component.display()
            )));
        }
    }
    Ok(metadata)
}
