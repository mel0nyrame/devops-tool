use std::{
    fs::OpenOptions,
    io::{self, Read},
    os::unix::fs::OpenOptionsExt,
    path::Path,
};

use super::{ArtifactKind, CheckIssue, CheckSource, Evidence, Item, entries};
use crate::locations::{Locations, check_path};

pub(super) fn find(
    locations: &Locations,
    name_query: Option<&str>,
    bundle_id: Option<&str>,
    issues: &mut Vec<CheckIssue>,
) -> Vec<Item> {
    let (id_query, name_query) = name_query
        .filter(|query| super::looks_like_bundle_id(query))
        .map_or((None, name_query), |query| (Some(query), None));
    if bundle_id.or(id_query).or(name_query).is_none() {
        return Vec::new();
    }

    let mut sources = [".config/opencode/cli.json", ".config/opencode/tui.jsonc"]
        .into_iter()
        .map(|relative| {
            (
                locations.home.join(relative),
                ArtifactKind::SharedConfiguration,
            )
        })
        .collect::<Vec<_>>();
    let pi_extensions = locations.home.join(".pi/agent/extensions");
    for entry in entries(&pi_extensions, CheckSource::ReferenceContent, issues) {
        let path = entry.path();
        if !path
            .extension()
            .is_some_and(|suffix| suffix == "js" || suffix == "ts")
        {
            continue;
        }
        match entry.file_type() {
            Ok(kind) if kind.is_file() || kind.is_symlink() => {
                sources.push((path, ArtifactKind::ExtensionSource));
            }
            Ok(_) => {}
            Err(error) => issues.push(CheckIssue {
                source: CheckSource::ReferenceContent,
                path,
                error,
            }),
        }
    }

    let mut items = Vec::new();
    for (path, kind) in sources {
        match read_reference(&path) {
            Ok(contents) => {
                let matched_term = bundle_id
                    .into_iter()
                    .chain(id_query)
                    .find(|term| contains_term(&contents, term, true))
                    .or_else(|| {
                        name_query.filter(|term| {
                            contains_term(&contents.to_lowercase(), &term.to_lowercase(), false)
                        })
                    });
                if let Some(term) = matched_term {
                    items.push(Item {
                        path,
                        kind,
                        evidence: Evidence::SharedReference {
                            matched_term: term.to_owned(),
                        },
                        symbolic_link: false,
                    });
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => issues.push(CheckIssue {
                source: CheckSource::ReferenceContent,
                path,
                error,
            }),
        }
    }
    items
}

fn contains_term(contents: &str, term: &str, identifier: bool) -> bool {
    let word_char =
        |ch: char| ch.is_alphanumeric() || ch == '_' || (identifier && matches!(ch, '.' | '-'));
    contents.match_indices(term).any(|(start, _)| {
        contents[..start]
            .chars()
            .next_back()
            .is_none_or(|ch| !word_char(ch))
            && contents[start + term.len()..]
                .chars()
                .next()
                .is_none_or(|ch| !word_char(ch))
    })
}

fn read_reference(path: &Path) -> io::Result<String> {
    const MAX_REFERENCE_SIZE: u64 = 1024 * 1024;
    if !check_path(path)?.is_file() {
        return Err(io::Error::other("reference source is not a regular file"));
    }
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW_ANY | libc::O_NONBLOCK)
        .open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(io::Error::other("reference source is not a regular file"));
    }
    if metadata.len() > MAX_REFERENCE_SIZE {
        return Err(io::Error::other("reference source exceeds 1 MiB"));
    }
    let mut contents = String::new();
    file.take(MAX_REFERENCE_SIZE + 1)
        .read_to_string(&mut contents)?;
    if contents.len() as u64 > MAX_REFERENCE_SIZE {
        return Err(io::Error::other("reference source exceeds 1 MiB"));
    }
    Ok(contents)
}
