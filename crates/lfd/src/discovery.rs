mod references;

use std::{
    collections::HashSet,
    fs, io,
    path::{Path, PathBuf},
};

use crate::{
    bundle::{BundleMetadata, read_metadata},
    locations::{Locations, check_path},
    receipt::{self, Receipt, ReceiptInspection},
};

#[derive(Clone, Copy)]
pub enum ArtifactKind {
    ApplicationBundle,
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
}

pub enum Evidence {
    ApplicationBundle(Application),
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
    SharedReference {
        matched_term: String,
    },
}

pub struct Item {
    pub path: PathBuf,
    pub kind: ArtifactKind,
    pub evidence: Evidence,
    pub symbolic_link: bool,
}

#[derive(Clone, Copy)]
pub enum CheckSource {
    Applications,
    BundleMetadata,
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

pub struct Report {
    pub main_program: MainProgram,
    pub items: Vec<Item>,
    pub issues: Vec<CheckIssue>,
}

pub struct Application {
    pub path: PathBuf,
    pub metadata: Option<BundleMetadata>,
    pub matched_by: BundleMatch,
}

impl Application {
    pub fn metadata_path(&self) -> PathBuf {
        self.path.join("Contents/Info.plist")
    }

    pub fn identifier(&self) -> Option<&str> {
        self.metadata.as_ref()?.identifier.as_deref()
    }
}

pub enum Identity {
    Application(Application),
    PackageReceipt(Receipt),
}

impl Identity {
    fn matches_exact_identifier(&self) -> bool {
        match self {
            Self::Application(app) => matches!(app.matched_by, BundleMatch::Identifier),
            Self::PackageReceipt(receipt) => receipt.exact,
        }
    }

    pub fn path(&self) -> &Path {
        match self {
            Self::Application(app) => &app.path,
            Self::PackageReceipt(receipt) => &receipt.path,
        }
    }
}

pub struct Search {
    pub matches: Vec<Identity>,
    application_issues: Vec<CheckIssue>,
    receipt_issues: Vec<CheckIssue>,
}

impl Search {
    pub fn run(locations: &Locations, query: &str) -> Self {
        let mut application_issues = Vec::new();
        let mut receipt_issues = Vec::new();
        let mut matches = find_applications(locations, query, &mut application_issues)
            .into_iter()
            .map(Identity::Application)
            .chain(
                receipt::find(locations, query, &mut receipt_issues)
                    .into_iter()
                    .map(Identity::PackageReceipt),
            )
            .collect::<Vec<_>>();
        if matches.iter().any(Identity::matches_exact_identifier) {
            matches.retain(Identity::matches_exact_identifier);
        }
        Self {
            matches,
            application_issues,
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
        let mut issues = self.application_issues;
        let application_check_incomplete = !issues.is_empty();
        issues.extend(self.receipt_issues);
        let data_dirs = user_data_dirs(locations, &mut issues);
        let (receipt, app) = match identity {
            Some(Identity::Application(app)) => (ReceiptInspection::default(), Some(app)),
            Some(Identity::PackageReceipt(receipt)) => {
                (receipt.inspect(locations, &data_dirs, &mut issues), None)
            }
            None => (ReceiptInspection::default(), None),
        };
        let main_program = receipt.application_check.unwrap_or_else(|| {
            if app.is_some() {
                MainProgram::Found
            } else if application_check_incomplete {
                MainProgram::Incomplete
            } else {
                MainProgram::NotFound
            }
        });
        let mut items = receipt.items;
        let mut artifacts = find_artifacts(&data_dirs, name_query, app.as_ref(), &mut issues);
        let recorded_paths: HashSet<_> = items.iter().map(|item| item.path.as_path()).collect();
        artifacts.retain(|item| !recorded_paths.contains(item.path.as_path()));
        items.extend(artifacts);
        items.extend(references::find(
            locations,
            name_query,
            app.as_ref().and_then(Application::identifier),
            &mut issues,
        ));
        if let Some(app) = app {
            items.insert(
                0,
                Item {
                    path: app.path.clone(),
                    kind: ArtifactKind::ApplicationBundle,
                    evidence: Evidence::ApplicationBundle(app),
                    symbolic_link: false,
                },
            );
        }
        Report {
            main_program,
            items,
            issues,
        }
    }
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
            match entry.file_type() {
                Ok(kind) if kind.is_dir() => {}
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
            }
            let metadata_path = path.join("Contents/Info.plist");
            let metadata = match read_metadata(&metadata_path) {
                Ok(parsed) => {
                    if !parsed.invalid_keys.is_empty() {
                        issues.push(CheckIssue {
                            source: CheckSource::BundleMetadata,
                            path: metadata_path,
                            error: io::Error::other(format!(
                                "Expected strings in {}",
                                parsed.invalid_keys.join(", ")
                            )),
                        });
                    }
                    Some(parsed.metadata)
                }
                Err(error) => {
                    issues.push(CheckIssue {
                        source: CheckSource::BundleMetadata,
                        path: metadata_path,
                        error,
                    });
                    None
                }
            };
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
                });
            }
        }
    }
    apps
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
    name_query: Option<&str>,
    app: Option<&Application>,
    issues: &mut Vec<CheckIssue>,
) -> Vec<Item> {
    let mut items = Vec::new();
    for UserDataDirectory { path, kind, suffix } in data_dirs {
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
                Ok(file_type) => items.push(Item {
                    path: entry.path(),
                    kind: *kind,
                    evidence,
                    symbolic_link: file_type.is_symlink(),
                }),
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
