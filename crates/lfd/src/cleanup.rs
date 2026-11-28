use std::{
    ffi::{CStr, CString, OsStr},
    fs::{File, Metadata, OpenOptions},
    io::{self, Read},
    mem::MaybeUninit,
    os::{
        fd::{AsRawFd, FromRawFd},
        macos::fs::MetadataExt as _,
        unix::{
            ffi::OsStrExt,
            fs::{MetadataExt, OpenOptionsExt},
        },
    },
    path::{Component, Path, PathBuf},
};

use crate::{
    discovery::{Evidence, Item, MainProgram, check_container_program},
    locations::Locations,
    plist,
};

pub const CONTAINERS: &str = "Library/Containers";
pub const CONTAINER_METADATA: &str = ".com.apple.containermanagerd.metadata.plist";

#[derive(PartialEq, Eq)]
struct Identity {
    device: u64,
    inode: u64,
    birth: (i64, i64),
    kind: u32,
}

impl Identity {
    fn of(metadata: &Metadata) -> Self {
        Self {
            device: metadata.dev(),
            inode: metadata.ino(),
            birth: (metadata.st_birthtime(), metadata.st_birthtime_nsec()),
            kind: metadata.mode() & u32::from(libc::S_IFMT),
        }
    }
}

pub struct Snapshot {
    relative: PathBuf,
    directories: Vec<Identity>,
    item: Identity,
    container_metadata: Option<Identity>,
}

impl Snapshot {
    pub fn inspect(home: &Path, path: &Path, metadata: &Metadata) -> io::Result<Self> {
        let relative = path.strip_prefix(home).map_err(io::Error::other)?;
        let account = Account::current()?;
        let home = open_home(home)?;
        let (parent, directories) = open_parent(&home, relative, account)?;
        let snapshot = Self {
            relative: relative.to_owned(),
            directories,
            item: Identity::of(metadata),
            container_metadata: None,
        };
        snapshot.check_item(&parent, &file_name(relative)?, account)?;
        Ok(snapshot)
    }

    pub fn inspect_container(
        locations: &Locations,
        path: &Path,
        metadata: &Metadata,
    ) -> io::Result<Self> {
        let account = Account::current()?;
        let home = cleanup_home(locations, account)?;
        let mut snapshot = Self::inspect(&home, path, metadata)?;
        let (parent, name) = snapshot.recheck(&open_home(&home)?, account)?;
        snapshot.container_metadata =
            Some(snapshot.read_container_metadata(&parent, &name, account)?);
        Ok(snapshot)
    }

    fn read_container_metadata(
        &self,
        parent: &File,
        name: &CStr,
        account: Account,
    ) -> io::Result<Identity> {
        if self.relative.parent() != Some(Path::new(CONTAINERS)) {
            return Err(io::Error::other("not a standard application container"));
        }
        let identifier = name.to_str().map_err(io::Error::other)?;
        let container = open_entry(parent, name, libc::O_DIRECTORY)?;
        let container_metadata = account.check_directory(&container)?;
        if self.item != Identity::of(&container_metadata) {
            return Err(io::Error::other("container identity changed"));
        }
        let evidence = CString::new(CONTAINER_METADATA).expect("constant metadata filename");
        let file = open_entry(&container, &evidence, 0)?;
        let metadata = file.metadata()?;
        if !metadata.is_file()
            || metadata.uid() != account.uid
            || metadata.mode() & 0o022 != 0
            || metadata.nlink() != 1
            || metadata.dev() != container_metadata.dev()
        {
            return Err(io::Error::other(
                "container metadata is not a verified user-owned regular file",
            ));
        }
        const MAX_SIZE: u64 = 4 * 1024 * 1024;
        let mut bytes = Vec::new();
        file.take(MAX_SIZE + 1).read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX_SIZE {
            return Err(io::Error::other("container metadata exceeds 4 MiB"));
        }
        let dictionary = plist::parse_dictionary(&bytes)?;
        if plist::string(&dictionary, "MCMMetadataIdentifier").as_deref() != Some(identifier) {
            return Err(io::Error::other(
                "metadata Bundle ID does not match the container directory",
            ));
        }
        Ok(Identity::of(&metadata))
    }

    fn check_item(&self, parent: &File, name: &CStr, account: Account) -> io::Result<()> {
        let metadata = entry_metadata(parent, name)?;
        let kind = metadata.st_mode & libc::S_IFMT;
        if kind == libc::S_IFLNK {
            return Err(io::Error::other("symbolic link not followed"));
        }
        let identity = Identity {
            device: metadata.st_dev as u64,
            inode: metadata.st_ino,
            birth: (metadata.st_birthtime, metadata.st_birthtime_nsec),
            kind: u32::from(kind),
        };
        if !matches!(kind, libc::S_IFREG | libc::S_IFDIR) || self.item != identity {
            return Err(io::Error::other("object identity or type changed"));
        }
        if metadata.st_uid != account.uid || identity.device != parent.metadata()?.dev() {
            return Err(io::Error::other(
                "object is outside the current user's cleanup scope",
            ));
        }
        Ok(())
    }

    fn recheck(&self, home: &File, account: Account) -> io::Result<(File, CString)> {
        let (parent, directories) = open_parent(home, &self.relative, account)?;
        if self.directories != directories {
            return Err(io::Error::other("cleanup directory identity changed"));
        }
        let name = file_name(&self.relative)?;
        self.check_item(&parent, &name, account)?;
        if let Some(expected) = &self.container_metadata
            && self.read_container_metadata(&parent, &name, account)? != *expected
        {
            return Err(io::Error::other("container metadata identity changed"));
        }
        Ok((parent, name))
    }
}

#[derive(Clone, Copy)]
struct Account {
    uid: libc::uid_t,
}

impl Account {
    fn current() -> io::Result<Self> {
        // SAFETY: these functions simply read the process credentials.
        let (uid, euid, gid, egid) = unsafe {
            (
                libc::getuid(),
                libc::geteuid(),
                libc::getgid(),
                libc::getegid(),
            )
        };
        if uid == 0 || uid != euid || gid != egid {
            return Err(io::Error::other(
                "cleanup requires an unprivileged current user",
            ));
        }
        Ok(Self { uid })
    }

    fn check_directory(self, directory: &File) -> io::Result<Metadata> {
        let metadata = directory.metadata()?;
        if metadata.uid() != self.uid || metadata.mode() & 0o022 != 0 {
            return Err(io::Error::other(
                "cleanup directory is not controlled by the current user",
            ));
        }
        Ok(metadata)
    }
}

fn open_home(home: &Path) -> io::Result<File> {
    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW_ANY)
        .open(home)
}

fn open_parent(
    home: &File,
    relative: &Path,
    account: Account,
) -> io::Result<(File, Vec<Identity>)> {
    let home_metadata = account.check_directory(home)?;
    let mut identities = vec![Identity::of(&home_metadata)];
    let mut directory = None;
    for component in relative
        .parent()
        .ok_or_else(|| io::Error::other("missing cleanup parent"))?
        .components()
    {
        let Component::Normal(name) = component else {
            return Err(io::Error::other("invalid cleanup path component"));
        };
        let child = open_entry(
            directory.as_ref().unwrap_or(home),
            &entry_name(name)?,
            libc::O_DIRECTORY,
        )?;
        let metadata = account.check_directory(&child)?;
        if metadata.dev() != home_metadata.dev() {
            return Err(io::Error::other("cleanup directory is on another volume"));
        }
        identities.push(Identity::of(&metadata));
        directory = Some(child);
    }
    let parent =
        directory.ok_or_else(|| io::Error::other("missing cleanup directory below home"))?;
    Ok((parent, identities))
}

fn file_name(path: &Path) -> io::Result<CString> {
    entry_name(
        path.file_name()
            .ok_or_else(|| io::Error::other("missing cleanup filename"))?,
    )
}

fn entry_name(name: &OsStr) -> io::Result<CString> {
    CString::new(name.as_bytes()).map_err(io::Error::other)
}

fn open_entry(parent: &File, name: &CStr, flags: libc::c_int) -> io::Result<File> {
    // SAFETY: the directory FD and terminated name remain valid through openat.
    let fd = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name.as_ptr(),
            flags | libc::O_CLOEXEC | libc::O_NOFOLLOW_ANY | libc::O_NONBLOCK,
        )
    };
    if fd == -1 {
        let error = io::Error::last_os_error();
        return Err(if error.raw_os_error() == Some(libc::ELOOP) {
            io::Error::other("symbolic link not followed")
        } else {
            error
        });
    }
    // SAFETY: successful openat returned a new owned FD.
    Ok(unsafe { File::from_raw_fd(fd) })
}

fn entry_metadata(parent: &File, name: &CStr) -> io::Result<libc::stat> {
    let mut metadata = MaybeUninit::uninit();
    // SAFETY: the directory FD and terminated name are valid, and metadata is a writable stat.
    let result = unsafe {
        libc::fstatat(
            parent.as_raw_fd(),
            name.as_ptr(),
            metadata.as_mut_ptr(),
            libc::AT_SYMLINK_NOFOLLOW,
        )
    };
    if result == -1 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: fstatat initialized metadata on success.
    Ok(unsafe { metadata.assume_init() })
}

pub struct Outcome {
    pub path: PathBuf,
    pub result: MoveResult,
}

pub enum MoveResult {
    Moved,
    Skipped(String),
    Failed(String),
}

pub fn move_reviewed(locations: &Locations, items: &[&Item]) -> Vec<Outcome> {
    let account =
        Account::current().and_then(|account| Ok((account, cleanup_home(locations, account)?)));
    items
        .iter()
        .map(|item| Outcome {
            path: item.path.clone(),
            result: match &account {
                Ok((account, home)) => move_one(locations, item, *account, home),
                Err(error) => {
                    MoveResult::Skipped(format!("could not verify cleanup scope: {error}"))
                }
            },
        })
        .collect()
}

#[cfg(feature = "test-fixtures")]
fn cleanup_home(locations: &Locations, _: Account) -> io::Result<PathBuf> {
    Ok(locations.home.clone())
}

#[cfg(not(feature = "test-fixtures"))]
fn cleanup_home(locations: &Locations, account: Account) -> io::Result<PathBuf> {
    let mut record = MaybeUninit::uninit();
    let mut entry = std::ptr::null_mut();
    let mut fields = [0u8; 16 * 1024];
    // SAFETY: the record, result pointer and buffer are writable for the duration of getpwuid_r.
    let result = unsafe {
        libc::getpwuid_r(
            account.uid,
            record.as_mut_ptr(),
            fields.as_mut_ptr().cast(),
            fields.len(),
            &mut entry,
        )
    };
    if result != 0 {
        return Err(io::Error::from_raw_os_error(result));
    }
    if entry.is_null() {
        return Err(io::Error::other("no macOS account for the current user"));
    }
    // SAFETY: a non-null result points to the record initialized by getpwuid_r.
    let entry = unsafe { record.assume_init() };
    if entry.pw_uid != account.uid || entry.pw_dir.is_null() {
        return Err(io::Error::other("macOS account has no home directory"));
    }
    // SAFETY: pw_dir points to a terminated field in the still-live fields buffer.
    let home = Path::new(OsStr::from_bytes(
        unsafe { CStr::from_ptr(entry.pw_dir) }.to_bytes(),
    ));
    if !home.is_absolute()
        || home.parent().is_none()
        || home.components().any(|part| part == Component::ParentDir)
        || home != locations.home
    {
        return Err(io::Error::other(
            "discovery home differs from the macOS account home",
        ));
    }
    Ok(home.to_owned())
}

fn open_trash(home: &File, account: Account) -> io::Result<File> {
    let trash = open_entry(home, c".Trash", libc::O_DIRECTORY)?;
    let metadata = account.check_directory(&trash)?;
    if metadata.mode() & 0o077 != 0 {
        return Err(io::Error::other("Trash is not private to the current user"));
    }
    if metadata.dev() != home.metadata()?.dev() {
        return Err(io::Error::other("Trash is on another volume"));
    }
    Ok(trash)
}

fn move_one(locations: &Locations, item: &Item, account: Account, home: &Path) -> MoveResult {
    let snapshot = match &item.evidence {
        Evidence::AppContainer {
            snapshot: Ok(snapshot),
        } if snapshot.container_metadata.is_some() => {
            let Some(identifier) = snapshot.relative.file_name().and_then(OsStr::to_str) else {
                return MoveResult::Skipped("container identifier could not be verified".into());
            };
            match check_container_program(locations, identifier, &mut Vec::new()) {
                MainProgram::Found => return MoveResult::Skipped("main program found".into()),
                MainProgram::Incomplete => {
                    return MoveResult::Skipped("required main program checks incomplete".into());
                }
                MainProgram::NotFound => snapshot,
            }
        }
        #[cfg(feature = "test-fixtures")]
        Evidence::RegisteredBundleData {
            snapshot: Ok(snapshot),
            ..
        } => snapshot,
        _ => return MoveResult::Skipped("cleanup eligibility could not be verified".into()),
    };
    let source = open_home(home).and_then(|home| {
        let (parent, name) = snapshot.recheck(&home, account)?;
        Ok((home, parent, name))
    });
    let (home, parent, name) = match source {
        Ok(source) => source,
        Err(error) => return MoveResult::Skipped(format!("could not recheck source: {error}")),
    };
    let trash = match open_trash(&home, account) {
        Ok(trash) => trash,
        Err(error) => return MoveResult::Failed(format!("could not verify Trash: {error}")),
    };
    if let Err(error) = snapshot.check_item(&parent, &name, account) {
        return MoveResult::Skipped(format!("could not recheck source: {error}"));
    }
    // SAFETY: both directory FDs and the terminated filename remain valid for the call.
    // RENAME_EXCL cannot replace an existing Trash item and does not fall back to copying.
    let result = unsafe {
        libc::renameatx_np(
            parent.as_raw_fd(),
            name.as_ptr(),
            trash.as_raw_fd(),
            name.as_ptr(),
            libc::RENAME_EXCL,
        )
    };
    if result == 0 {
        MoveResult::Moved
    } else {
        MoveResult::Failed(io::Error::last_os_error().to_string())
    }
}
