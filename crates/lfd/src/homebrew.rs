use std::{
    fs, io,
    path::{Component, Path, PathBuf},
};

use objc2::{
    rc::{Retained, autoreleasepool},
    runtime::AnyObject,
};
use objc2_foundation::{
    NSArray, NSData, NSDictionary, NSJSONReadingOptions, NSJSONSerialization, NSString,
};

use crate::{
    discovery::{
        Application, ArtifactKind, BundleMatch, CheckIssue, CheckSource, Evidence, Item,
        bundle_metadata, checked_metadata, entries, is_executable,
    },
    locations::{Locations, read_metadata_file},
};

const SOURCE: CheckSource = CheckSource::Homebrew;
const INSTALL_RECEIPT: &str = "INSTALL_RECEIPT.json";

#[derive(Clone, Copy)]
pub enum HomebrewKind {
    Formula,
    Cask,
}

impl HomebrewKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Formula => "formula",
            Self::Cask => "cask",
        }
    }
}

pub struct HomebrewPackage {
    pub kind: HomebrewKind,
    pub path: PathBuf,
    pub items: Vec<Item>,
    pub program_found: bool,
    pub check_incomplete: bool,
    pub checks_path: bool,
}

impl HomebrewPackage {
    pub fn single_application(&self) -> Option<&Application> {
        let mut apps = self.items.iter().filter_map(|item| match &item.evidence {
            Evidence::ApplicationBundle(app) => Some(app),
            _ => None,
        });
        let app = apps.next()?;
        apps.next().is_none().then_some(app)
    }
}

#[derive(Default)]
struct PackageScan {
    programs: Vec<Item>,
    records: Vec<Item>,
    unrecorded_commands: Vec<Item>,
    checks_path: bool,
}

#[derive(Default)]
pub struct HomebrewMatches {
    pub packages: Vec<HomebrewPackage>,
    pub unrecorded_commands: Vec<Item>,
}

pub fn find_packages(
    locations: &Locations,
    query: &str,
    issues: &mut Vec<CheckIssue>,
) -> HomebrewMatches {
    let mut matches = HomebrewMatches::default();
    for prefix in locations.homebrew_prefixes() {
        for (room, kind) in [
            ("Cellar", HomebrewKind::Formula),
            ("Caskroom", HomebrewKind::Cask),
        ] {
            for entry in entries(&prefix.join(room), SOURCE, issues) {
                if entry.file_name() != query {
                    continue;
                }
                let before = issues.len();
                let Some(path) = directory(entry, issues) else {
                    continue;
                };
                let mut scan = match kind {
                    HomebrewKind::Formula => find_formula(&path, query, issues),
                    HomebrewKind::Cask => find_cask(locations, &path, issues),
                };
                matches
                    .unrecorded_commands
                    .append(&mut scan.unrecorded_commands);
                if !scan.records.is_empty() {
                    let program_found = !scan.programs.is_empty();
                    scan.programs.append(&mut scan.records);
                    matches.packages.push(HomebrewPackage {
                        kind,
                        path,
                        items: scan.programs,
                        program_found,
                        check_incomplete: issues.len() != before,
                        checks_path: scan.checks_path,
                    });
                }
            }
        }
    }
    matches
}

fn find_formula(path: &Path, query: &str, issues: &mut Vec<CheckIssue>) -> PackageScan {
    let mut scan = PackageScan {
        checks_path: true,
        ..Default::default()
    };
    for entry in entries(path, SOURCE, issues) {
        let Some(version) = directory(entry, issues) else {
            continue;
        };
        let receipt = version.join(INSTALL_RECEIPT);
        let record_type = installation_record(&receipt, issues).map(|(_, _, file_type)| file_type);
        for subdirectory in ["bin", "sbin"] {
            for entry in entries(&version.join(subdirectory), SOURCE, issues) {
                if record_type.is_some() {
                    if let Some(item) = command_item(entry.path(), Some(&receipt), issues) {
                        scan.programs.push(item);
                    }
                } else if entry.file_name() == query
                    && let Some(item) = command_item(entry.path(), None, issues)
                {
                    scan.unrecorded_commands.push(item);
                }
            }
        }
        if let Some(file_type) = record_type {
            scan.records
                .push(record_item(receipt, HomebrewKind::Formula, file_type));
        }
    }
    scan
}

fn find_cask(locations: &Locations, cask: &Path, issues: &mut Vec<CheckIssue>) -> PackageScan {
    let mut scan = PackageScan::default();
    let receipt = cask.join(".metadata").join(INSTALL_RECEIPT);
    let Some((record, source, file_type)) = installation_record(&receipt, issues) else {
        return scan;
    };
    let directories = locations.application_dirs();
    match cask_artifacts(&record, &source) {
        Ok((version, artifacts)) => {
            let stage = cask.join(version);
            for artifact in artifacts {
                match artifact {
                    CaskArtifact::App { source, target } => {
                        let target = target.unwrap_or_else(|| {
                            PathBuf::from(source.file_name().unwrap_or_default())
                        });
                        let installed = match installed_app_path(&target, &directories) {
                            Some(path) => check_cask_app(path, &receipt, &mut scan, issues),
                            None => {
                                record_issue(
                                    &receipt,
                                    "Cask app target is outside the checked application directories",
                                    issues,
                                );
                                false
                            }
                        };
                        if !installed {
                            check_cask_app(stage.join(source), &receipt, &mut scan, issues);
                        }
                    }
                    CaskArtifact::Binary(source) => {
                        scan.checks_path = true;
                        let relative = if source.is_absolute() {
                            source.strip_prefix(&stage).ok()
                        } else {
                            Some(source.as_path())
                        };
                        if let Some(relative) = relative.filter(|path| is_relative_path(path)) {
                            let path = stage.join(relative);
                            if !scan.programs.iter().any(|item| item.path == path)
                                && let Some(item) = command_item(path, Some(&receipt), issues)
                            {
                                scan.programs.push(item);
                            }
                        } else {
                            record_issue(
                                &receipt,
                                "Cask binary source is outside the checked Caskroom version",
                                issues,
                            );
                        }
                    }
                }
            }
        }
        Err(error) => issues.push(CheckIssue {
            source: SOURCE,
            path: receipt.clone(),
            error,
        }),
    }
    scan.records
        .push(record_item(receipt, HomebrewKind::Cask, file_type));
    scan
}

fn installed_app_path(target: &Path, directories: &[PathBuf; 5]) -> Option<PathBuf> {
    if !is_application(target) {
        return None;
    }
    if is_name(target) {
        Some(directories[0].join(target))
    } else {
        target
            .parent()
            .filter(|parent| directories.iter().any(|directory| directory == parent))?;
        Some(target.to_path_buf())
    }
}

fn check_cask_app(
    path: PathBuf,
    receipt: &Path,
    scan: &mut PackageScan,
    issues: &mut Vec<CheckIssue>,
) -> bool {
    if scan.programs.iter().any(|item| item.path == path) {
        return true;
    }
    let Some(file_type) = checked_metadata(&path, SOURCE, issues)
        .map(|metadata| metadata.file_type())
        .filter(|file_type| file_type.is_dir())
    else {
        return false;
    };
    let metadata = bundle_metadata(&path, issues);
    scan.programs.push(
        Application {
            path,
            metadata,
            matched_by: BundleMatch::HomebrewCask(receipt.to_path_buf()),
            file_type,
        }
        .into_item(),
    );
    true
}

enum CaskArtifact {
    App {
        source: PathBuf,
        target: Option<PathBuf>,
    },
    Binary(PathBuf),
}

fn cask_artifacts(
    record: &NSDictionary,
    source: &NSDictionary,
) -> io::Result<(PathBuf, Vec<CaskArtifact>)> {
    autoreleasepool(|_| {
        let version = field(source, "version")
            .and_then(string_path)
            .filter(|path| is_name(path))
            .ok_or_else(|| io::Error::other("Missing or invalid Homebrew cask version"))?;
        let artifacts = field(record, "uninstall_artifacts")
            .and_then(|value| value.downcast::<NSArray>().ok())
            .ok_or_else(|| io::Error::other("Missing Homebrew cask artifacts"))?;
        let mut programs = Vec::new();
        for artifact in artifacts.to_vec() {
            let artifact = artifact
                .downcast::<NSDictionary>()
                .map_err(|_| io::Error::other("Invalid Homebrew cask artifact"))?;
            if let Some(app) = field(&artifact, "app") {
                let app = app
                    .downcast::<NSArray>()
                    .map_err(|_| io::Error::other("Invalid Homebrew cask app artifact"))?;
                let source = app
                    .firstObject()
                    .and_then(string_path)
                    .filter(|path| is_relative_path(path) && is_application(path))
                    .ok_or_else(|| io::Error::other("Invalid Homebrew cask app path"))?;
                let target = if app.len() > 1 {
                    let options = app
                        .objectAtIndex(1)
                        .downcast::<NSDictionary>()
                        .map_err(|_| io::Error::other("Invalid Homebrew cask app options"))?;
                    field(&options, "target")
                        .map(|target| {
                            string_path(target)
                                .ok_or_else(|| io::Error::other("Invalid Homebrew cask app target"))
                        })
                        .transpose()?
                } else {
                    None
                };
                programs.push(CaskArtifact::App { source, target });
            }
            if let Some(binary) = field(&artifact, "binary") {
                let source = binary
                    .downcast::<NSArray>()
                    .ok()
                    .and_then(|array| array.firstObject())
                    .and_then(string_path)
                    .ok_or_else(|| io::Error::other("Invalid Homebrew cask binary path"))?;
                programs.push(CaskArtifact::Binary(source));
            }
        }
        if programs.is_empty() {
            return Err(io::Error::other(
                "No supported app or binary in the Homebrew cask record",
            ));
        }
        Ok((version, programs))
    })
}

fn field(object: &NSDictionary, key: &str) -> Option<Retained<AnyObject>> {
    object.objectForKey(&NSString::from_str(key))
}

fn string_path(value: Retained<AnyObject>) -> Option<PathBuf> {
    value
        .downcast::<NSString>()
        .ok()
        .map(|value| PathBuf::from(value.to_string()))
}

fn is_name(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|name| name == path.as_os_str())
}

fn is_relative_path(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && path
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
}

fn is_application(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("app"))
}

fn directory(entry: fs::DirEntry, issues: &mut Vec<CheckIssue>) -> Option<PathBuf> {
    let path = entry.path();
    checked_metadata(&path, SOURCE, issues)?
        .is_dir()
        .then_some(path)
}

fn installation_record(
    path: &Path,
    issues: &mut Vec<CheckIssue>,
) -> Option<(Retained<NSDictionary>, Retained<NSDictionary>, fs::FileType)> {
    match read_installation_record(path) {
        Ok(record) => Some(record),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => {
            issues.push(CheckIssue {
                source: SOURCE,
                path: path.to_path_buf(),
                error,
            });
            None
        }
    }
}

fn read_installation_record(
    path: &Path,
) -> io::Result<(Retained<NSDictionary>, Retained<NSDictionary>, fs::FileType)> {
    let (bytes, file_type) = read_metadata_file(path, "Homebrew installation record")?;
    autoreleasepool(|_| {
        let value = NSJSONSerialization::JSONObjectWithData_options_error(
            &NSData::with_bytes(&bytes),
            NSJSONReadingOptions::empty(),
        )
        .map_err(|error| io::Error::other(error.localizedDescription().to_string()))?;
        let record = value
            .downcast::<NSDictionary>()
            .map_err(|_| io::Error::other("Expected a Homebrew installation record dictionary"))?;
        let source = field(&record, "source")
            .and_then(|value| value.downcast::<NSDictionary>().ok())
            .ok_or_else(|| io::Error::other("Missing Homebrew installation record source"))?;
        Ok((record, source, file_type))
    })
}

fn record_issue(path: &Path, reason: &str, issues: &mut Vec<CheckIssue>) {
    issues.push(CheckIssue {
        source: SOURCE,
        path: path.to_path_buf(),
        error: io::Error::other(reason),
    });
}

fn command_item(
    path: PathBuf,
    receipt: Option<&Path>,
    issues: &mut Vec<CheckIssue>,
) -> Option<Item> {
    let metadata = checked_metadata(&path, SOURCE, issues)?;
    is_executable(&metadata).then(|| Item {
        path,
        kind: ArtifactKind::StandaloneCommand,
        evidence: receipt.map_or(Evidence::UnrecordedHomebrewCommand, |record| {
            Evidence::HomebrewProgram(record.to_path_buf())
        }),
        file_type: metadata.file_type(),
        in_cleanup_scope: false,
    })
}

fn record_item(path: PathBuf, kind: HomebrewKind, file_type: fs::FileType) -> Item {
    Item {
        path,
        kind: ArtifactKind::HomebrewRecord,
        evidence: Evidence::HomebrewRecord(kind),
        file_type,
        in_cleanup_scope: false,
    }
}
