mod references;

use std::{
    collections::HashSet,
    fs, io,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

use crate::{
    bundle::{BundleMetadata, read_metadata},
    homebrew::{self, HomebrewKind, HomebrewPackage},
    locations::{Locations, check_path},
    receipt::{self, Receipt, ReceiptInspection},
};

#[derive(Clone, Copy)]
pub enum ArtifactKind {
    ApplicationBundle,
    StandaloneCommand,
    HomebrewRecord,
    ApplicationData,
    Preferences,
    Cache,
    PackageReceipt,
    Configuration,
    State,
    Logs,
    TemporaryData,
    SharedConfiguration,
    ExtensionSource,
}

pub enum BundleMatch {
    DirectoryName,
    DisplayName,
    Name,
    Identifier,
    HomebrewCask(PathBuf),
}

pub enum Evidence {
    ApplicationBundle(Application),
    PathCommand,
    HomebrewRecord(HomebrewKind),
    HomebrewProgram(PathBuf),
    UnrecordedHomebrewCommand,
    PackageReceipt {
        identifier: String,
        matched_package_filename: Option<String>,
    },
    PackageReceiptPath {
        identifier: String,
        receipt: PathBuf,
        verified_bundle_id: Option<String>,
    },
    FilenameMatch,
    BundleIdentifierFilename {
        identifier: String,
        source: PathBuf,
    },
    RegisteredBundleData {
        identifier: String,
        source: PathBuf,
        app_data_root: PathBuf,
    },
    SharedReference {
        matched_term: String,
    },
}

pub struct Item {
    pub path: PathBuf,
    pub kind: ArtifactKind,
    pub evidence: Evidence,
    pub file_type: fs::FileType,
    pub(crate) in_cleanup_scope: bool,
}

impl Item {
    pub fn selection_blocker(&self, program: &MainProgram) -> Option<&'static str> {
        if self.file_type.is_symlink() {
            return Some("symbolic links cannot be selected");
        }
        match self.evidence {
            Evidence::SharedReference { .. } => {
                return Some("a reference does not establish ownership of this file");
            }
            Evidence::PackageReceipt { .. } | Evidence::PackageReceiptPath { .. } => {
                return Some("a receipt does not establish current ownership or installation");
            }
            _ => {}
        }
        if !self.in_cleanup_scope {
            return Some("outside the cleanup scope");
        }
        match (&self.evidence, program) {
            (Evidence::RegisteredBundleData { .. }, MainProgram::NotFound) => None,
            (
                Evidence::RegisteredBundleData { .. } | Evidence::BundleIdentifierFilename { .. },
                MainProgram::Found,
            ) => Some("main program found"),
            (Evidence::RegisteredBundleData { .. }, MainProgram::Incomplete) => {
                Some("required main program checks incomplete")
            }
            _ => Some("application ownership not established"),
        }
    }
}

impl ArtifactKind {
    pub fn risk(self) -> &'static str {
        match self {
            Self::ApplicationBundle => "may contain an installed application",
            Self::StandaloneCommand => "not launched or inspected",
            Self::HomebrewRecord => "installation evidence, not proof of a current main program",
            Self::ApplicationData => "may contain retained user data",
            Self::Preferences => "may contain retained settings",
            Self::Cache => "may contain cached data the application would need to recreate",
            Self::PackageReceipt => "historical evidence only",
            Self::Configuration => "may contain retained settings",
            Self::State => "may contain retained sessions or user data",
            Self::Logs => "may contain retained activity records",
            Self::TemporaryData => "contents and ownership have not been verified",
            Self::SharedConfiguration => "may contain settings for another tool",
            Self::ExtensionSource => "may contain code for another tool",
        }
    }
}

#[derive(Clone, Copy)]
pub enum CheckSource {
    Applications,
    BundleMetadata,
    PathCommand,
    Homebrew,
    UserData,
    PackageReceipts,
    PackageReceiptPaths,
    ReferenceContent,
}

pub struct CheckIssue {
    pub source: CheckSource,
    pub path: PathBuf,
    pub error: io::Error,
}

pub enum MainProgram {
    Found,
    NotFound,
    Incomplete,
}

impl MainProgram {
    pub fn from_check(found: bool, incomplete: bool) -> Self {
        if found {
            Self::Found
        } else if incomplete {
            Self::Incomplete
        } else {
            Self::NotFound
        }
    }
}

pub struct Report {
    pub main_program: MainProgram,
    pub selection_program: MainProgram,
    pub items: Vec<Item>,
    pub issues: Vec<CheckIssue>,
}

pub struct Application {
    pub path: PathBuf,
    pub metadata: Option<BundleMetadata>,
    pub matched_by: BundleMatch,
    pub(crate) file_type: fs::FileType,
}

impl Application {
    pub fn metadata_path(&self) -> PathBuf {
        self.path.join("Contents/Info.plist")
    }

    pub fn identifier(&self) -> Option<&str> {
        self.metadata.as_ref()?.identifier.as_deref()
    }

    pub fn into_item(self) -> Item {
        Item {
            path: self.path.clone(),
            kind: ArtifactKind::ApplicationBundle,
            file_type: self.file_type,
            in_cleanup_scope: false,
            evidence: Evidence::ApplicationBundle(self),
        }
    }
}

pub enum Identity {
    Application(Application),
    Command(Item),
    Homebrew(HomebrewPackage),
    PackageReceipt(Receipt),
}

impl Identity {
    fn matches_exact_identifier(&self) -> bool {
        match self {
            Self::Application(app) => matches!(app.matched_by, BundleMatch::Identifier),
            Self::PackageReceipt(receipt) => receipt.exact,
            _ => false,
        }
    }

    pub fn path(&self) -> &Path {
        match self {
            Self::Application(app) => &app.path,
            Self::Command(item) => &item.path,
            Self::Homebrew(package) => &package.path,
            Self::PackageReceipt(receipt) => &receipt.path,
        }
    }
}

pub struct Search {
    pub matches: Vec<Identity>,
    program_issues: Vec<CheckIssue>,
    receipt_issues: Vec<CheckIssue>,
}

impl Search {
    pub fn run(locations: &Locations, query: &str) -> Self {
        let mut program_issues = Vec::new();
        let mut receipt_issues = Vec::new();
        let mut matches: Vec<_> = find_applications(locations, query, &mut program_issues)
            .into_iter()
            .map(Identity::Application)
            .chain(
                receipt::find(locations, query, &mut receipt_issues)
                    .into_iter()
                    .map(Identity::PackageReceipt),
            )
            .collect();
        if matches.iter().any(Identity::matches_exact_identifier) {
            matches.retain(Identity::matches_exact_identifier);
        } else {
            let brew = homebrew::find_packages(locations, query, &mut program_issues);
            let commands = find_path_commands(locations, query, &mut program_issues);
            let belongs_to_package = |path: &Path| {
                brew.packages
                    .iter()
                    .any(|package| package.items.iter().any(|item| item.path == path))
            };
            matches.retain(|identity| !belongs_to_package(identity.path()));
            matches.extend(
                commands
                    .into_iter()
                    .filter(|command| {
                        !belongs_to_package(&command.path)
                            && !brew
                                .unrecorded_commands
                                .iter()
                                .any(|other| command.path == other.path)
                    })
                    .map(Identity::Command),
            );
            matches.extend(brew.unrecorded_commands.into_iter().map(Identity::Command));
            matches.extend(brew.packages.into_iter().map(Identity::Homebrew));
        }
        Self {
            matches,
            program_issues,
            receipt_issues,
        }
    }

    pub fn report(self, locations: &Locations, query: &str, selected: usize) -> Report {
        let ambiguous = self.matches.len() > 1;
        let identity = self.matches.into_iter().nth(selected);
        let name_query = (!ambiguous
            && !identity
                .as_ref()
                .is_some_and(Identity::matches_exact_identifier))
        .then_some(query);
        let mut issues = self.program_issues;
        let main_program_check_incomplete = !issues.is_empty();
        let path_check_incomplete = issues
            .iter()
            .any(|issue| matches!(issue.source, CheckSource::PathCommand));
        issues.extend(self.receipt_issues);
        let data_dirs = user_data_dirs(locations, &mut issues);
        let mut receipt = ReceiptInspection::default();
        let main_program = match &identity {
            Some(Identity::Homebrew(package)) => MainProgram::from_check(
                package.program_found,
                package.check_incomplete || (package.checks_path && path_check_incomplete),
            ),
            Some(Identity::PackageReceipt(package)) => {
                receipt = package.inspect(locations, &data_dirs, &mut issues);
                receipt.application_check.unwrap_or_else(|| {
                    MainProgram::from_check(false, main_program_check_incomplete)
                })
            }
            Some(_) => MainProgram::Found,
            None => MainProgram::from_check(false, main_program_check_incomplete),
        };
        let app = match &identity {
            Some(Identity::Application(app)) => Some(app),
            Some(Identity::Homebrew(package)) => package.single_application(),
            _ => None,
        };
        let (registration, supplemental) = selection_evidence(locations, query);
        let selection_program = match (&main_program, supplemental) {
            (MainProgram::Found, _) | (_, MainProgram::Found) => MainProgram::Found,
            (MainProgram::NotFound, MainProgram::NotFound) => MainProgram::NotFound,
            _ => MainProgram::Incomplete,
        };
        let mut items = receipt.items;
        let mut artifacts = find_artifacts(
            &data_dirs,
            &locations.home,
            name_query,
            app,
            registration.as_deref(),
            &mut issues,
        );
        let recorded_paths: HashSet<_> = items.iter().map(|item| item.path.as_path()).collect();
        artifacts.retain(|item| !recorded_paths.contains(item.path.as_path()));
        items.extend(artifacts);
        items.extend(references::find(
            locations,
            name_query,
            app.and_then(Application::identifier),
            &mut issues,
        ));
        let program_items = match identity {
            Some(Identity::Application(app)) => vec![app.into_item()],
            Some(Identity::Command(command)) => vec![command],
            Some(Identity::Homebrew(package)) => package.items,
            Some(Identity::PackageReceipt(_)) | None => Vec::new(),
        };
        items.splice(..0, program_items);
        Report {
            main_program,
            selection_program,
            items,
            issues,
        }
    }
}

fn find_path_commands(
    locations: &Locations,
    query: &str,
    issues: &mut Vec<CheckIssue>,
) -> Vec<Item> {
    let mut commands: Vec<Item> = Vec::new();
    for directory in locations.path_dirs() {
        let path = if directory.is_absolute() {
            directory.join(query)
        } else {
            match std::env::current_dir() {
                Ok(current) => current.join(directory).join(query),
                Err(error) => {
                    issues.push(CheckIssue {
                        source: CheckSource::PathCommand,
                        path: directory,
                        error,
                    });
                    continue;
                }
            }
        };
        if let Some(metadata) = checked_metadata(&path, CheckSource::PathCommand, issues)
            && is_executable(&metadata)
            && !commands.iter().any(|item| item.path == path)
        {
            commands.push(Item {
                path,
                kind: ArtifactKind::StandaloneCommand,
                evidence: Evidence::PathCommand,
                file_type: metadata.file_type(),
                in_cleanup_scope: false,
            });
        }
    }
    commands
}

pub fn checked_metadata(
    path: &Path,
    source: CheckSource,
    issues: &mut Vec<CheckIssue>,
) -> Option<fs::Metadata> {
    match check_path(path) {
        Ok(metadata) => Some(metadata),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => {
            issues.push(CheckIssue {
                source,
                path: path.to_path_buf(),
                error,
            });
            None
        }
    }
}

pub fn is_executable(metadata: &fs::Metadata) -> bool {
    metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
}

fn find_applications(
    locations: &Locations,
    query: &str,
    issues: &mut Vec<CheckIssue>,
) -> Vec<Application> {
    let mut apps = Vec::new();
    for directory in locations.application_dirs() {
        for entry in entries(&directory, CheckSource::Applications, issues) {
            let path = entry.path();
            let Some(stem) = path.file_stem().and_then(|name| name.to_str()) else {
                continue;
            };
            if !path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("app"))
            {
                continue;
            }
            let file_type = match entry.file_type() {
                Ok(kind) if kind.is_dir() => kind,
                Ok(kind) if kind.is_symlink() => {
                    issues.push(CheckIssue {
                        source: CheckSource::Applications,
                        path,
                        error: io::Error::other("symbolic app bundle not followed"),
                    });
                    continue;
                }
                Ok(_) => continue,
                Err(error) => {
                    issues.push(CheckIssue {
                        source: CheckSource::Applications,
                        path,
                        error,
                    });
                    continue;
                }
            };
            let metadata = bundle_metadata(&path, issues);
            let names = [
                (
                    metadata
                        .as_ref()
                        .and_then(|value| value.display_name.as_deref()),
                    BundleMatch::DisplayName,
                ),
                (
                    metadata.as_ref().and_then(|value| value.name.as_deref()),
                    BundleMatch::Name,
                ),
                (Some(stem), BundleMatch::DirectoryName),
            ];
            let matched_by = if metadata
                .as_ref()
                .and_then(|value| value.identifier.as_deref())
                == Some(query)
            {
                Some(BundleMatch::Identifier)
            } else {
                names.into_iter().find_map(|(name, matched_by)| {
                    name.filter(|name| same_name(name, query))
                        .map(|_| matched_by)
                })
            };
            if let Some(matched_by) = matched_by {
                apps.push(Application {
                    path,
                    metadata,
                    matched_by,
                    file_type,
                });
            }
        }
    }
    apps
}

pub fn bundle_metadata(path: &Path, issues: &mut Vec<CheckIssue>) -> Option<BundleMetadata> {
    let metadata_path = path.join("Contents/Info.plist");
    let error = match read_metadata(&metadata_path) {
        Ok(parsed) if parsed.invalid_keys.is_empty() => return Some(parsed.metadata),
        Ok(parsed) => {
            issues.push(CheckIssue {
                source: CheckSource::BundleMetadata,
                path: metadata_path,
                error: io::Error::other(format!(
                    "Expected strings in {}",
                    parsed.invalid_keys.join(", ")
                )),
            });
            return Some(parsed.metadata);
        }
        Err(error) => error,
    };
    issues.push(CheckIssue {
        source: CheckSource::BundleMetadata,
        path: metadata_path,
        error,
    });
    None
}

pub struct UserDataDirectory {
    pub path: PathBuf,
    pub kind: ArtifactKind,
    suffix: Option<&'static str>,
}

fn user_data_dirs(locations: &Locations, issues: &mut Vec<CheckIssue>) -> Vec<UserDataDirectory> {
    let home_dirs = [
        (
            "Library/Application Support",
            ArtifactKind::ApplicationData,
            None,
        ),
        (
            "Library/Preferences",
            ArtifactKind::Preferences,
            Some(".plist"),
        ),
        ("Library/Caches", ArtifactKind::Cache, None),
        ("Library/Containers", ArtifactKind::ApplicationData, None),
        (
            "Library/Group Containers",
            ArtifactKind::ApplicationData,
            None,
        ),
        ("Library/Logs", ArtifactKind::Logs, None),
        (
            "Library/Saved Application State",
            ArtifactKind::State,
            Some(".savedState"),
        ),
        (".config", ArtifactKind::Configuration, None),
        (".local/share", ArtifactKind::ApplicationData, None),
        (".local/state", ArtifactKind::State, None),
        (".cache", ArtifactKind::Cache, None),
    ]
    .into_iter()
    .map(|(relative, kind, suffix)| (locations.home.join(relative), kind, suffix));
    let temp_dir = match locations.user_temp_dir() {
        Ok(path) => Some((path, ArtifactKind::TemporaryData, None)),
        Err(error) => {
            issues.push(CheckIssue {
                source: CheckSource::UserData,
                path: PathBuf::from("DARWIN_USER_TEMP_DIR"),
                error,
            });
            None
        }
    };
    home_dirs
        .chain(temp_dir)
        .map(|(path, kind, suffix)| UserDataDirectory { path, kind, suffix })
        .collect()
}

fn find_artifacts(
    data_dirs: &[UserDataDirectory],
    home: &Path,
    name_query: Option<&str>,
    app: Option<&Application>,
    registration: Option<&Path>,
    issues: &mut Vec<CheckIssue>,
) -> Vec<Item> {
    let mut items = Vec::new();
    for UserDataDirectory { path, kind, suffix } in data_dirs {
        let relative_root = path.strip_prefix(home).ok().and_then(Path::to_str);
        for entry in entries(path, CheckSource::UserData, issues) {
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            let stem = suffix
                .and_then(|suffix| name.strip_suffix(suffix))
                .unwrap_or(name);
            let evidence = if let Some(app) = app.filter(|app| app.identifier() == Some(stem)) {
                Evidence::BundleIdentifierFilename {
                    identifier: stem.to_owned(),
                    source: app.metadata_path(),
                }
            } else if name_query.is_some_and(|query| {
                if looks_like_bundle_id(query) {
                    stem == query
                } else {
                    same_name(stem, query)
                }
            }) {
                Evidence::FilenameMatch
            } else {
                continue;
            };
            match entry.file_type() {
                Ok(file_type) => {
                    let in_cleanup_scope = match relative_root {
                        Some("Library/Application Support" | "Library/Caches") => {
                            file_type.is_dir()
                        }
                        Some("Library/Preferences") => name != stem && file_type.is_file(),
                        _ => false,
                    };
                    let source =
                        registration.filter(|_| in_cleanup_scope && name_query == Some(stem));
                    let path = entry.path();
                    let evidence =
                        source.map_or(evidence, |source| Evidence::RegisteredBundleData {
                            identifier: stem.to_owned(),
                            source: source.to_owned(),
                            app_data_root: path.clone(),
                        });
                    items.push(Item {
                        path: path.clone(),
                        kind: *kind,
                        evidence,
                        file_type,
                        in_cleanup_scope,
                    });
                    if let Some(source) = source
                        && relative_root == Some("Library/Application Support")
                    {
                        for child in entries(&path, CheckSource::UserData, issues) {
                            match child.file_type() {
                                Ok(file_type) => items.push(Item {
                                    path: child.path(),
                                    kind: *kind,
                                    evidence: Evidence::RegisteredBundleData {
                                        identifier: stem.to_owned(),
                                        source: source.to_owned(),
                                        app_data_root: path.clone(),
                                    },
                                    file_type,
                                    in_cleanup_scope: file_type.is_file() || file_type.is_dir(),
                                }),
                                Err(error) => issues.push(CheckIssue {
                                    source: CheckSource::UserData,
                                    path: child.path(),
                                    error,
                                }),
                            }
                        }
                    }
                }
                Err(error) => issues.push(CheckIssue {
                    source: CheckSource::UserData,
                    path: entry.path(),
                    error,
                }),
            }
        }
    }
    items
}

#[cfg(not(feature = "test-fixtures"))]
fn selection_evidence(_: &Locations, _: &str) -> (Option<PathBuf>, MainProgram) {
    // Supplemental main-program checks and independent registration are not yet available.
    (None, MainProgram::Incomplete)
}

#[cfg(feature = "test-fixtures")]
fn selection_evidence(locations: &Locations, query: &str) -> (Option<PathBuf>, MainProgram) {
    use std::{io::Read, os::unix::fs::OpenOptionsExt};

    if !looks_like_bundle_id(query) {
        return (None, MainProgram::Incomplete);
    }
    let records = locations.system_root.join("var/db/lfd-fixtures");
    let source = records.join(format!("{query}.plist"));
    let registration = read_metadata(&source)
        .ok()
        .filter(|parsed| {
            parsed.invalid_keys.is_empty() && parsed.metadata.identifier.as_deref() == Some(query)
        })
        .map(|_| source);

    // Reproduce OS-boundary outcomes only. The real application-directory scan still runs.
    let mut response = String::new();
    let checks = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW_ANY | libc::O_NONBLOCK)
        .open(records.join(format!("{query}.checks")))
        .and_then(|file| {
            if !file.metadata()?.is_file() {
                return Err(io::Error::other("not a regular fixture response"));
            }
            file.take(256).read_to_string(&mut response)
        });
    let supplemental = if checks.is_ok()
        && response == "PATH: not found\nHomebrew: not found\nPackage receipts: not found\n"
    {
        MainProgram::NotFound
    } else {
        MainProgram::Incomplete
    };
    (registration, supplemental)
}

fn looks_like_bundle_id(query: &str) -> bool {
    query.contains('.')
        && query.split('.').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == b'-')
        })
}

pub fn same_name(left: &str, right: &str) -> bool {
    left.to_lowercase() == right.to_lowercase()
}

pub fn entries(
    path: &Path,
    source: CheckSource,
    issues: &mut Vec<CheckIssue>,
) -> Vec<fs::DirEntry> {
    let entries = match check_path(path).and_then(|_| fs::read_dir(path)) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Vec::new(),
        Err(error) => {
            issues.push(CheckIssue {
                source,
                path: path.to_path_buf(),
                error,
            });
            return Vec::new();
        }
    };
    let mut readable = Vec::new();
    for entry in entries {
        match entry {
            Ok(entry) => readable.push(entry),
            Err(error) => issues.push(CheckIssue {
                source,
                path: path.to_path_buf(),
                error,
            }),
        }
    }
    readable.sort_by_key(fs::DirEntry::file_name);
    readable
}
