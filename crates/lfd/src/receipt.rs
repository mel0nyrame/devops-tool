use std::{
    collections::BTreeMap,
    fs::OpenOptions,
    io::{self, BufRead, BufReader},
    os::unix::fs::OpenOptionsExt,
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
};

use crate::{
    bundle::parse_metadata,
    discovery::{
        ArtifactKind, CheckIssue, CheckSource, Evidence, Item, UserDataDirectory, entries,
        same_name,
    },
    locations::{Locations, check_path},
    plist,
};

pub struct Receipt {
    pub path: PathBuf,
    pub identifier: String,
    pub exact: bool,
    prefix: Option<String>,
    matched_package_filename: Option<String>,
}

pub fn find(locations: &Locations, query: &str, issues: &mut Vec<CheckIssue>) -> Vec<Receipt> {
    let directory = locations.system_root.join("private/var/db/receipts");
    let source = CheckSource::PackageReceipts;
    let mut receipts = Vec::new();
    for entry in entries(&directory, source, issues) {
        let path = entry.path();
        if !path
            .extension()
            .is_some_and(|extension| extension == "plist")
        {
            continue;
        }
        let receipt = plist::read_dictionary(&path).and_then(|dictionary| {
            let identifier = plist::string(&dictionary, "PackageIdentifier")
                .filter(|identifier| {
                    path.file_stem().and_then(|stem| stem.to_str()) == Some(identifier)
                })
                .ok_or_else(|| {
                    io::Error::other("PackageIdentifier does not match receipt filename")
                })?;
            Ok((
                identifier,
                plist::string(&dictionary, "PackageFileName"),
                plist::string(&dictionary, "InstallPrefixPath"),
            ))
        });
        match receipt {
            Ok((identifier, package_filename, prefix)) => {
                let exact = identifier == query;
                let matched_package_filename = package_filename.filter(|filename| {
                    let filename = Path::new(filename);
                    filename
                        .extension()
                        .is_some_and(|extension| extension.eq_ignore_ascii_case("pkg"))
                        && filename
                            .file_stem()
                            .and_then(|stem| stem.to_str())
                            .is_some_and(|stem| {
                                same_name(stem, query) && !same_name(stem, &identifier)
                            })
                });
                if exact || matched_package_filename.is_some() {
                    receipts.push(Receipt {
                        path,
                        identifier,
                        exact,
                        prefix,
                        matched_package_filename,
                    });
                }
            }
            Err(error) => issues.push(CheckIssue {
                source,
                path,
                error,
            }),
        }
    }
    receipts
}

#[derive(Default)]
pub struct ReceiptInspection {
    pub items: Vec<Item>,
    pub application_found: bool,
    pub application_check_incomplete: bool,
}

impl Receipt {
    pub fn inspect(
        &self,
        locations: &Locations,
        data_dirs: &[UserDataDirectory],
        issues: &mut Vec<CheckIssue>,
    ) -> ReceiptInspection {
        let mut inspection = ReceiptInspection::default();
        inspection.items.push(Item {
            path: self.path.clone(),
            kind: ArtifactKind::PackageReceipt,
            evidence: Evidence::PackageReceipt {
                identifier: self.identifier.clone(),
                matched_package_filename: self.matched_package_filename.clone(),
            },
            symbolic_link: false,
        });
        let bom = self.path.with_extension("bom");
        let paths = match scope_paths(locations, data_dirs, self.prefix.as_deref(), &bom) {
            Ok(paths) => paths,
            Err(error) => {
                issues.push(CheckIssue {
                    source: CheckSource::PackageReceipts,
                    path: bom,
                    error,
                });
                return inspection;
            }
        };
        for RecordedPath {
            path,
            kind,
            bundle_metadata,
        } in paths
        {
            let is_bundle = matches!(kind, ArtifactKind::ApplicationBundle);
            let metadata = match check_path(&path) {
                Ok(metadata) => metadata,
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                Err(error) => {
                    inspection.application_check_incomplete |= is_bundle;
                    issues.push(CheckIssue {
                        source: CheckSource::PackageReceiptPaths,
                        path,
                        error,
                    });
                    continue;
                }
            };
            if is_bundle && !metadata.is_dir() {
                continue;
            }
            let verified_bundle_id = if is_bundle {
                match self.verify_bundle(&path, bundle_metadata) {
                    Ok(identifier) => {
                        inspection.application_found = true;
                        Some(identifier)
                    }
                    Err(error) => {
                        inspection.application_check_incomplete = true;
                        issues.push(CheckIssue {
                            source: CheckSource::PackageReceiptPaths,
                            path: path.clone(),
                            error,
                        });
                        None
                    }
                }
            } else {
                None
            };
            inspection.items.push(Item {
                path,
                kind,
                evidence: Evidence::PackageReceiptPath {
                    identifier: self.identifier.clone(),
                    receipt: self.path.clone(),
                    verified_bundle_id,
                },
                symbolic_link: false,
            });
        }
        inspection
    }

    fn verify_bundle(&self, path: &Path, recorded: Option<RecordedMetadata>) -> io::Result<String> {
        let bytes = plist::read_bytes(&path.join("Contents/Info.plist"))?;
        let metadata = parse_metadata(&bytes)?;
        let identifier = metadata.metadata.identifier.ok_or_else(|| {
            io::Error::other("current application has no readable CFBundleIdentifier")
        })?;
        if identifier == self.identifier || recorded.is_some_and(|record| record.matches(&bytes)) {
            return Ok(identifier);
        }
        Err(io::Error::other(format!(
            "current Bundle ID {identifier} could not be tied to the selected receipt"
        )))
    }
}

struct RecordedPath {
    path: PathBuf,
    kind: ArtifactKind,
    bundle_metadata: Option<RecordedMetadata>,
}

struct RecordedMetadata {
    checksum: u32,
    size: usize,
}

impl RecordedMetadata {
    fn matches(&self, bytes: &[u8]) -> bool {
        self.size == bytes.len() && self.checksum == bom_checksum(bytes)
    }
}

fn bom_checksum(bytes: &[u8]) -> u32 {
    // BOM regular-file checksums use POSIX cksum: CRC-32 with the length folded in.
    let mut crc = 0u32;
    let length_bytes = bytes.len().to_le_bytes();
    let length = length_bytes
        .iter()
        .rposition(|byte| *byte != 0)
        .map_or(0, |index| index + 1);
    for byte in bytes.iter().chain(&length_bytes[..length]) {
        crc ^= u32::from(*byte) << 24;
        for _ in 0..8 {
            let polynomial = if crc >> 31 == 1 { 0x04c11db7 } else { 0 };
            crc = (crc << 1) ^ polynomial;
        }
    }
    !crc
}

fn scope_paths(
    locations: &Locations,
    data_dirs: &[UserDataDirectory],
    prefix: Option<&str>,
    bom: &Path,
) -> io::Result<Vec<RecordedPath>> {
    let file = match OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW_ANY | libc::O_NONBLOCK)
        .open(bom)
    {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    if !file.metadata()?.is_file() {
        return Err(io::Error::other("receipt BOM is not a regular file"));
    }
    let prefix = prefix
        .map(Path::new)
        .filter(|prefix| {
            !prefix
                .components()
                .any(|component| component == Component::ParentDir)
        })
        .ok_or_else(|| io::Error::other("missing or invalid InstallPrefixPath"))?;
    let root = locations
        .system_root
        .join(prefix.strip_prefix("/").unwrap_or(prefix));
    let application_dirs = locations.application_dirs();

    // Give lsbom the already opened receipt; it cannot reopen a replacement path.
    let mut lsbom = Command::new("/usr/bin/lsbom")
        .args(["-p", "FMcs", "/dev/fd/0"])
        .stdin(file)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let mut paths = BTreeMap::new();
    let mut bundle_metadata = BTreeMap::new();
    let output = BufReader::new(lsbom.stdout.take().unwrap());
    let result = output.split(b'\n').try_for_each(|line| -> io::Result<()> {
        let line = line?;
        let Ok(line) = std::str::from_utf8(&line) else {
            return Ok(());
        };
        let mut fields = line.split('\t');
        let Some(relative) = fields
            .next()
            .and_then(|line| line.strip_prefix("\"./"))
            .and_then(|line| line.strip_suffix('"'))
            .filter(|line| !line.contains('"') && !line.chars().any(char::is_control))
            .map(Path::new)
            .filter(|path| {
                path.components()
                    .all(|component| matches!(component, Component::Normal(_)))
            })
        else {
            return Ok(());
        };
        let path = root.join(relative);
        if is_application_path(&path, &application_dirs) {
            paths.insert(path, ArtifactKind::ApplicationBundle);
        } else if let Some(directory) = data_dirs
            .iter()
            .find(|directory| Some(directory.path.as_path()) == path.parent())
        {
            paths.insert(path, directory.kind);
        } else if path.ends_with("Contents/Info.plist")
            && let Some(bundle) = path.ancestors().nth(2)
            && is_application_path(bundle, &application_dirs)
            && let (Some(mode), Some(checksum), Some(size), None) =
                (fields.next(), fields.next(), fields.next(), fields.next())
            && mode.starts_with('-')
            && let (Ok(checksum), Ok(size)) = (checksum.parse(), size.parse())
        {
            bundle_metadata.insert(bundle.to_path_buf(), RecordedMetadata { checksum, size });
        }
        Ok(())
    });
    if result.is_err() {
        let _ = lsbom.kill();
    }
    let status = lsbom.wait()?;
    result?;
    if !status.success() {
        return Err(io::Error::other(format!("lsbom failed: {status}")));
    }
    Ok(paths
        .into_iter()
        .map(|(path, kind)| RecordedPath {
            bundle_metadata: bundle_metadata.remove(&path),
            path,
            kind,
        })
        .collect())
}

fn is_application_path(path: &Path, application_dirs: &[PathBuf]) -> bool {
    path.parent()
        .is_some_and(|parent| application_dirs.iter().any(|directory| directory == parent))
        && path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("app"))
}
