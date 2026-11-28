use std::{
    fs, io,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

use crate::{
    bundle::{BundleMetadata, read_metadata},
    homebrew::{self, HomebrewKind, HomebrewPackage},
    locations::{Locations, check_path},
};

#[derive(Clone, Copy)]
pub enum ArtifactKind {
    ApplicationBundle,
    StandaloneCommand,
    HomebrewRecord,
    ApplicationData,
    Preferences,
    Cache,
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
    FilenameMatch,
    BundleIdentifierFilename { identifier: String, source: PathBuf },
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
    PathCommand,
    Homebrew,
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

    pub fn into_item(self) -> Item {
        Item {
            path: self.path.clone(),
            kind: ArtifactKind::ApplicationBundle,
            evidence: Evidence::ApplicationBundle(self),
            symbolic_link: false,
        }
    }
}

pub enum ProgramIdentity {
    Application(Application),
    Command(Item),
    Homebrew(HomebrewPackage),
}

impl ProgramIdentity {
    pub fn path(&self) -> &Path {
        match self {
            Self::Application(app) => &app.path,
            Self::Command(item) => &item.path,
            Self::Homebrew(package) => &package.path,
        }
    }
}

pub struct ProgramCheck {
    pub matches: Vec<ProgramIdentity>,
    issues: Vec<CheckIssue>,
}

impl ProgramCheck {
    pub fn run(locations: &Locations, query: &str) -> Self {
        let mut issues = Vec::new();
        let mut apps = find_applications(locations, query, &mut issues);
        let exact_bundle_id = apps
            .iter()
            .any(|app| matches!(app.matched_by, BundleMatch::Identifier));
        if exact_bundle_id {
            apps.retain(|app| matches!(app.matched_by, BundleMatch::Identifier));
        }
        let mut brew = homebrew::HomebrewMatches::default();
        let mut commands = Vec::new();
        if !exact_bundle_id {
            brew = homebrew::find_packages(locations, query, &mut issues);
            commands = find_path_commands(locations, query, &mut issues);
        }
        let belongs_to_package = |path: &Path| {
            brew.packages
                .iter()
                .any(|package| package.items.iter().any(|item| item.path == path))
        };
        let mut matches: Vec<_> = apps
            .into_iter()
            .filter(|app| !belongs_to_package(&app.path))
            .map(ProgramIdentity::Application)
            .collect();
        commands.retain(|command| {
            !belongs_to_package(&command.path)
                && !brew
                    .unrecorded_commands
                    .iter()
                    .any(|other| command.path == other.path)
        });
        matches.extend(
            commands
                .into_iter()
                .chain(brew.unrecorded_commands)
                .map(ProgramIdentity::Command),
        );
        matches.extend(brew.packages.into_iter().map(ProgramIdentity::Homebrew));
        Self { matches, issues }
    }

    pub fn report(self, locations: &Locations, query: &str, selected: usize) -> Report {
        let ambiguous = self.matches.len() > 1;
        let mut issues = self.issues;
        let identity = self.matches.into_iter().nth(selected);
        let main_program = match &identity {
            Some(ProgramIdentity::Homebrew(package)) => MainProgram::from_check(
                package.program_found,
                package.check_incomplete
                    || (package.checks_path
                        && issues
                            .iter()
                            .any(|issue| matches!(issue.source, CheckSource::PathCommand))),
            ),
            Some(_) => MainProgram::Found,
            None => MainProgram::from_check(false, !issues.is_empty()),
        };
        let app = match &identity {
            Some(ProgramIdentity::Application(app)) => Some(app),
            Some(ProgramIdentity::Homebrew(package)) => package.single_application(),
            _ => None,
        };
        let name_query = (!ambiguous
            && !app.is_some_and(|app| matches!(app.matched_by, BundleMatch::Identifier)))
        .then_some(query);
        let mut items = find_artifacts(locations, name_query, app, &mut issues);
        let program_items = match identity {
            Some(ProgramIdentity::Application(app)) => vec![app.into_item()],
            Some(ProgramIdentity::Command(command)) => vec![command],
            Some(ProgramIdentity::Homebrew(package)) => package.items,
            None => Vec::new(),
        };
        items.splice(..0, program_items);
        Report {
            main_program,
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
        if checked_metadata(&path, CheckSource::PathCommand, issues)
            .is_some_and(|metadata| is_executable(&metadata))
            && !commands.iter().any(|item| item.path == path)
        {
            commands.push(Item {
                path,
                kind: ArtifactKind::StandaloneCommand,
                evidence: Evidence::PathCommand,
                symbolic_link: false,
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

fn find_artifacts(
    locations: &Locations,
    name_query: Option<&str>,
    app: Option<&Application>,
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
                Ok(file_type) => items.push(Item {
                    path: entry.path(),
                    kind,
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

fn same_name(left: &str, right: &str) -> bool {
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
