use std::{
    fs, io,
    path::{Path, PathBuf},
};

use crate::{
    bundle::{BundleMetadata, read_metadata},
    locations::{Locations, check_path},
};

#[derive(Clone, Copy)]
pub enum ArtifactKind {
    ApplicationBundle,
    ApplicationData,
    Preferences,
    Cache,
}

pub enum BundleMatch {
    DirectoryName,
    DisplayName,
    Name,
    Identifier,
}

pub enum Evidence {
    ApplicationBundle(Application),
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
}

pub struct Item {
    pub path: PathBuf,
    pub kind: ArtifactKind,
    pub evidence: Evidence,
    pub file_type: fs::FileType,
    in_cleanup_scope: bool,
}

impl Item {
    pub fn selection_blocker(&self, program: &MainProgram) -> Option<&'static str> {
        if self.file_type.is_symlink() {
            return Some("symbolic links cannot be selected");
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
            Self::ApplicationData => "may contain retained user data",
            Self::Preferences => "may contain retained settings",
            Self::Cache => "may contain cached data the application would need to recreate",
        }
    }
}

#[derive(Clone, Copy)]
pub enum CheckSource {
    Applications,
    BundleMetadata,
    UserData,
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
    pub selection_program: MainProgram,
    pub items: Vec<Item>,
    pub issues: Vec<CheckIssue>,
}

pub struct Application {
    pub path: PathBuf,
    pub metadata: Option<BundleMetadata>,
    pub matched_by: BundleMatch,
    file_type: fs::FileType,
}

impl Application {
    pub fn metadata_path(&self) -> PathBuf {
        self.path.join("Contents/Info.plist")
    }

    pub fn identifier(&self) -> Option<&str> {
        self.metadata.as_ref()?.identifier.as_deref()
    }
}

pub struct ApplicationCheck {
    pub matches: Vec<Application>,
    issues: Vec<CheckIssue>,
}

impl ApplicationCheck {
    pub fn run(locations: &Locations, query: &str) -> Self {
        let mut issues = Vec::new();
        let mut matches = find_applications(locations, query, &mut issues);
        if matches
            .iter()
            .any(|app| matches!(app.matched_by, BundleMatch::Identifier))
        {
            matches.retain(|app| matches!(app.matched_by, BundleMatch::Identifier));
        }
        Self { matches, issues }
    }

    pub fn report(self, locations: &Locations, query: &str, selected: usize) -> Report {
        let ambiguous = self.matches.len() > 1;
        let mut issues = self.issues;
        let app = self.matches.into_iter().nth(selected);
        let main_program = if app.is_some() {
            MainProgram::Found
        } else if issues.is_empty() {
            MainProgram::NotFound
        } else {
            MainProgram::Incomplete
        };
        let name_query = (!ambiguous
            && !app
                .as_ref()
                .is_some_and(|app| matches!(app.matched_by, BundleMatch::Identifier)))
        .then_some(query);
        let (registration, supplemental) = selection_evidence(locations, query);
        let selection_program = match (&main_program, supplemental) {
            (MainProgram::Found, _) | (_, MainProgram::Found) => MainProgram::Found,
            (MainProgram::NotFound, MainProgram::NotFound) => MainProgram::NotFound,
            _ => MainProgram::Incomplete,
        };
        let mut items = find_artifacts(
            locations,
            name_query,
            app.as_ref(),
            registration.as_deref(),
            &mut issues,
        );
        if let Some(app) = app {
            items.insert(
                0,
                Item {
                    path: app.path.clone(),
                    kind: ArtifactKind::ApplicationBundle,
                    file_type: app.file_type,
                    in_cleanup_scope: false,
                    evidence: Evidence::ApplicationBundle(app),
                },
            );
        }
        Report {
            main_program,
            selection_program,
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
                    file_type,
                });
            }
        }
    }
    apps
}

fn find_artifacts(
    locations: &Locations,
    name_query: Option<&str>,
    app: Option<&Application>,
    registration: Option<&Path>,
    issues: &mut Vec<CheckIssue>,
) -> Vec<Item> {
    let mut items = Vec::new();
    for (relative, kind) in [
        ("Library/Application Support", ArtifactKind::ApplicationData),
        ("Library/Preferences", ArtifactKind::Preferences),
        ("Library/Caches", ArtifactKind::Cache),
    ] {
        for entry in entries(
            &locations.home.join(relative),
            CheckSource::UserData,
            issues,
        ) {
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            let stem = if matches!(kind, ArtifactKind::Preferences) {
                name.strip_suffix(".plist").unwrap_or(name)
            } else {
                name
            };
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
                    let in_cleanup_scope = match kind {
                        ArtifactKind::ApplicationData | ArtifactKind::Cache => file_type.is_dir(),
                        ArtifactKind::Preferences => name != stem && file_type.is_file(),
                        ArtifactKind::ApplicationBundle => false,
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
                        kind,
                        evidence,
                        file_type,
                        in_cleanup_scope,
                    });
                    if let Some(source) = source
                        && matches!(kind, ArtifactKind::ApplicationData)
                    {
                        for child in entries(&path, CheckSource::UserData, issues) {
                            match child.file_type() {
                                Ok(file_type) => items.push(Item {
                                    path: child.path(),
                                    kind,
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
    // Discovery currently checks only application directories; that is not enough for selection.
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

fn same_name(left: &str, right: &str) -> bool {
    left.to_lowercase() == right.to_lowercase()
}

fn entries(path: &Path, source: CheckSource, issues: &mut Vec<CheckIssue>) -> Vec<fs::DirEntry> {
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
