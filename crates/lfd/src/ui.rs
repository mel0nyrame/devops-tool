use std::{collections::BTreeSet, path::Path};

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    widgets::{Block, List, ListItem, ListState, Paragraph, Wrap},
};

use crate::{
    cleanup::{CONTAINER_METADATA, MoveResult, Outcome},
    discovery::{
        ArtifactKind, BundleMatch, CheckIssue, CheckSource, Evidence, Identity, Item, MainProgram,
        Report,
    },
};

pub fn draw_search(frame: &mut Frame, input: &str, error: &str) {
    let input = visible(input);
    let text = format!(
        "Application name or Bundle ID, command name, Homebrew package name, or .pkg package ID:\n> {input}\n\n{error}\n\nEnter search / Esc quit"
    );
    frame.render_widget(panel(text, " lfd · read-only inspection ", 0), frame.area());
}

pub fn draw_choices(
    frame: &mut Frame,
    identities: &[Identity],
    selection: &mut ListState,
    scroll: u16,
) {
    let [main, footer] =
        Layout::vertical([Constraint::Min(1), Constraint::Length(2)]).areas(frame.area());
    let [left, right] = columns(main, 60);
    let rows = identities.iter().map(|identity| {
        ListItem::new(format!(
            "{}\n{}\n",
            identity_label(identity),
            display_path(identity.path()),
        ))
    });
    frame.render_stateful_widget(list(rows, " Choose one application "), left, selection);
    if let Some(identity) = selection.selected().and_then(|index| identities.get(index)) {
        let mut details = format!(
            "{}\n\n{}",
            identity_label(identity),
            display_path(identity.path())
        );
        if let Identity::Homebrew(package) = identity {
            if let Some(app) = package.single_application() {
                details.push_str(&format!(
                    "\n\nBundle ID: {}\n{}",
                    visible(app.identifier().unwrap_or("unavailable")),
                    display_path(&app.path)
                ));
            }
        }
        frame.render_widget(panel(details, " Installation ", scroll), right);
    }
    frame.render_widget(Paragraph::new("↑/↓ browse · PgUp/PgDn details · Enter choose · q / Esc quit\nName-only artifacts are not assigned to a selected identity."), footer);
}

fn identity_label(identity: &Identity) -> String {
    match identity {
        Identity::Application(app) => {
            format!(
                "Bundle ID: {}",
                visible(app.identifier().unwrap_or("unavailable"))
            )
        }
        Identity::PackageReceipt(receipt) => {
            format!("Package ID: {}", visible(&receipt.identifier))
        }
        Identity::Command(item) => format!(
            "Executable: {}",
            visible(&item.path.file_name().unwrap_or_default().to_string_lossy())
        ),
        Identity::Homebrew(package) => format!(
            "Homebrew {}: {}",
            package.kind.name(),
            visible(
                &package
                    .path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
            ),
        ),
    }
}

pub fn draw_report(
    frame: &mut Frame,
    report: &Report,
    selection: &mut ListState,
    marked: &BTreeSet<usize>,
    empty_preview: bool,
    scroll: u16,
) {
    let selection_status = match report
        .container_program
        .or_else(|| cfg!(feature = "test-fixtures").then_some(report.selection_program))
    {
        Some(MainProgram::Incomplete) => {
            "Selection disabled: required main program checks incomplete.\n"
        }
        _ => "",
    };
    let selection_line = if empty_preview {
        "Nothing selected for preview."
    } else {
        "Space toggle · Enter preview"
    };
    let [header, main, footer] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(1),
        Constraint::Length(if selection_status.is_empty() { 2 } else { 3 }),
    ])
    .areas(frame.area());
    let [left, right] = columns(main, 32);
    let (status, title) = match report.main_program {
        MainProgram::Found => (
            "Main program found in the checked locations.",
            " Related items ",
        ),
        MainProgram::NotFound => (
            "Main program not found in the checked locations.\nThis does not prove uninstallation.",
            " Leftover candidates ",
        ),
        MainProgram::Incomplete => ("Main program check incomplete.", " Possible matches "),
    };
    let mut text = String::from(status);
    if !report.issues.is_empty() {
        text.push_str(&format!(
            "\nUncheckable locations: {} (i to inspect)",
            report.issues.len()
        ));
    }
    frame.render_widget(Paragraph::new(text).wrap(Wrap { trim: false }), header);
    let rows = report.items.iter().enumerate().map(|(index, item)| {
        let name = visible(&item.path.file_name().unwrap_or_default().to_string_lossy());
        if item.selection_blocker(report).is_some() {
            name
        } else if marked.contains(&index) {
            format!("[x] {name}")
        } else {
            format!("[ ] {name}")
        }
    });
    frame.render_stateful_widget(list(rows, title), left, selection);
    let text = selection
        .selected()
        .and_then(|index| report.items.get(index))
        .map_or_else(
            || "No matching items.".into(),
            |item| evidence(item, report),
        );
    frame.render_widget(panel(text, " Evidence ", scroll), right);
    frame.render_widget(
        Paragraph::new(format!(
            "{selection_status}Selected: {} · {selection_line}\n↑/↓ browse · PgUp/PgDn evidence · q / Esc quit",
            marked.len()
        )),
        footer,
    );
}

pub fn draw_preview(frame: &mut Frame, items: &[&Item], move_to_trash: bool, scroll: u16) {
    let [body, footer] =
        Layout::vertical([Constraint::Min(1), Constraint::Length(2)]).areas(frame.area());
    let mut text = String::from("Only these paths will be moved if you confirm Move to Trash.\n\n");
    for item in items {
        text.push_str(&display_path(&item.path));
        text.push('\n');
        if item.file_type.is_dir() {
            text.push_str("Includes all descendants.\n");
        }
        text.push_str(&format!("Risk: {}.\n\n", item.kind.risk()));
    }
    frame.render_widget(panel(text, " Cleanup preview ", scroll), body);
    let choices = if move_to_trash {
        "Cancel     [ Move to Trash ]"
    } else {
        "[ Cancel ]     Move to Trash"
    };
    frame.render_widget(
        Paragraph::new(format!(
            "{choices}\n←/→ choose · Enter confirm · Esc return · PgUp/PgDn scroll"
        )),
        footer,
    );
}

pub fn draw_outcomes(frame: &mut Frame, outcomes: &[Outcome], scroll: u16) {
    let [body, footer] =
        Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(frame.area());
    let mut text = String::new();
    for outcome in outcomes {
        let status = match &outcome.result {
            MoveResult::Moved => "Moved to Trash".into(),
            MoveResult::Skipped(reason) => format!("Skipped: {}", visible(reason)),
            MoveResult::Failed(reason) => format!("Failed: {}", visible(reason)),
        };
        text.push_str(&format!("{status}\n{}\n\n", display_path(&outcome.path)));
    }
    frame.render_widget(panel(text, " Cleanup results ", scroll), body);
    frame.render_widget(Paragraph::new("PgUp/PgDn scroll · q / Esc quit"), footer);
}

pub fn draw_issues(
    frame: &mut Frame,
    issues: &[CheckIssue],
    selection: &mut ListState,
    scroll: u16,
) {
    let [main, footer] =
        Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(frame.area());
    let [left, right] = columns(main, 32);
    let rows = issues
        .iter()
        .map(|issue| visible(&issue.path.file_name().unwrap_or_default().to_string_lossy()));
    frame.render_stateful_widget(list(rows, " Uncheckable locations "), left, selection);
    if let Some(issue) = selection.selected().and_then(|index| issues.get(index)) {
        let source = match issue.source {
            CheckSource::Applications => "Application directory",
            CheckSource::BundleMetadata => "Bundle metadata",
            CheckSource::PathCommand => "PATH command check",
            CheckSource::Homebrew => "Homebrew location or record",
            CheckSource::UserData => "User data location",
            CheckSource::PackageReceipts => "Registered .pkg receipts",
            CheckSource::PackageReceiptPaths => "Path recorded by a registered .pkg receipt",
            CheckSource::ReferenceContent => "Shared reference source",
        };
        let reason = if issue.error.raw_os_error() == Some(libc::ELOOP) {
            format!("symbolic link not followed: {}", issue.error)
        } else {
            issue.error.to_string()
        };
        let text = format!(
            "{}\n\n{source}\n{}",
            display_path(&issue.path),
            visible(&reason)
        );
        frame.render_widget(panel(text, " Could not check ", scroll), right);
    }
    frame.render_widget(
        Paragraph::new("↑/↓ browse · PgUp/PgDn details · i results · q / Esc quit"),
        footer,
    );
}

fn columns(area: Rect, left: u16) -> [Rect; 2] {
    Layout::horizontal([
        Constraint::Percentage(left),
        Constraint::Percentage(100 - left),
    ])
    .areas(area)
}

fn list<'a>(rows: impl IntoIterator<Item = impl Into<ListItem<'a>>>, title: &'a str) -> List<'a> {
    List::new(rows)
        .block(Block::bordered().title(title))
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
        .highlight_symbol("> ")
}

fn panel(text: String, title: &str, scroll: u16) -> Paragraph<'_> {
    Paragraph::new(text)
        .wrap(Wrap { trim: false })
        .block(Block::bordered().title(title))
        .scroll((scroll, 0))
}

fn evidence(item: &Item, report: &Report) -> String {
    let kind = match item.kind {
        ArtifactKind::ApplicationBundle => "Application bundle",
        ArtifactKind::StandaloneCommand => "Executable command",
        ArtifactKind::HomebrewRecord => "Homebrew record",
        ArtifactKind::ApplicationData => "Application data",
        ArtifactKind::Preferences => "Preferences",
        ArtifactKind::Cache => "Cache",
        ArtifactKind::PackageReceipt => "Registered .pkg receipt",
        ArtifactKind::Configuration => "Configuration",
        ArtifactKind::State => "State",
        ArtifactKind::Logs => "Logs",
        ArtifactKind::TemporaryData => "Temporary data",
        ArtifactKind::SharedConfiguration => "Shared configuration",
        ArtifactKind::ExtensionSource => "Extension source",
    };
    let risk = item.kind.risk();
    let mut text = format!(
        "{}\n\nType: {kind}\nRisk: {risk}\n\n",
        display_path(&item.path)
    );
    if let Some(reason) = item.selection_blocker(report) {
        text.push_str(&format!("View only: {reason}.\n\n"));
    } else {
        text.push_str("Eligible for preview (Space to select or deselect).\n\n");
    }
    if item.file_type.is_symlink() {
        text.push_str("Symbolic link: target not inspected.\n");
    }
    match &item.evidence {
        Evidence::ApplicationBundle(app) => {
            let field = match &app.matched_by {
                BundleMatch::DirectoryName => "bundle directory name",
                BundleMatch::DisplayName => "CFBundleDisplayName",
                BundleMatch::Name => "CFBundleName",
                BundleMatch::Identifier => "CFBundleIdentifier",
                BundleMatch::HomebrewCask(_) => "Homebrew cask app path",
            };
            text.push_str(&format!("Matched on {field}."));
            if let Some(metadata) = &app.metadata {
                for (key, value) in [
                    ("CFBundleDisplayName", &metadata.display_name),
                    ("CFBundleName", &metadata.name),
                    ("CFBundleIdentifier", &metadata.identifier),
                ] {
                    if let Some(value) = value {
                        text.push_str(&format!("\n{key}: {}", visible(value)));
                    }
                }
                text.push_str(&format!(
                    "\nMetadata: {}",
                    display_path(&app.metadata_path())
                ));
            }
            if let BundleMatch::HomebrewCask(record) = &app.matched_by {
                text.push_str(&format!("\nLocal Homebrew installation record:\n{}\nHomebrew installation path: view-only.", display_path(record)));
            }
        }
        Evidence::PathCommand => text.push_str("Executable name matches the query in PATH."),
        Evidence::HomebrewRecord(kind) => text.push_str(&format!(
            "Homebrew {} installation record.\nHomebrew global location: view-only.",
            kind.name()
        )),
        Evidence::HomebrewProgram(record) => text.push_str(&format!(
            "Local Homebrew installation record:\n{}\nHomebrew global location: view-only.",
            display_path(record),
        )),
        Evidence::UnrecordedHomebrewCommand => text.push_str(
            "Executable name matches the query. No local Homebrew installation record;\napplication ownership not established.\nHomebrew global location: view-only.",
        ),
        Evidence::PackageReceipt {
            identifier,
            matched_package_filename,
        } => {
            text.push_str(&format!("Package ID: {}\n", visible(identifier)));
            if let Some(filename) = matched_package_filename {
                text.push_str(&format!(
                    "Matched registered installer filename: {}\n",
                    visible(filename)
                ));
            }
        }
        Evidence::PackageReceiptPath {
            identifier,
            receipt,
            verified_bundle_id,
        } => {
            text.push_str(&format!(
                "Recorded by the registered .pkg receipt for {}.\nReceipt: {}\nThe recorded path currently exists.",
                visible(identifier),
                display_path(receipt),
            ));
            if let Some(bundle_id) = verified_bundle_id {
                text.push_str(&format!(
                    "\nVerified current Bundle ID: {}",
                    visible(bundle_id)
                ));
            }
        }
        Evidence::FilenameMatch => {
            text.push_str("Filename match only; application ownership not established.")
        }
        Evidence::BundleIdentifierFilename { identifier, source } => text.push_str(&format!(
            "Filename matches the local CFBundleIdentifier: {}\nMetadata: {}",
            visible(identifier),
            display_path(source)
        )),
        Evidence::RegisteredBundleData {
            identifier,
            source,
            app_data_root,
            ..
        } => {
            text.push_str(&format!(
                "Standard app-specific data path matches the registered Bundle ID: {}\nRegistration: {}",
                visible(identifier),
                display_path(source)
            ));
            if app_data_root != &item.path {
                text.push_str(&format!(
                    "\nInside the app-specific path: {}",
                    display_path(app_data_root)
                ));
            }
        }
        Evidence::AppContainer { snapshot } => {
            match snapshot {
                Ok(_) => text.push_str("Container metadata matches the directory Bundle ID."),
                Err(error) => text.push_str(&format!(
                    "Container ownership could not be verified: {}",
                    visible(&error.to_string()),
                )),
            }
            let metadata_path = item.path.join(CONTAINER_METADATA);
            text.push_str(&format!("\nMetadata: {}", display_path(&metadata_path)));
        }
        Evidence::SharedReference { matched_term } => text.push_str(&format!(
            "Shared reference: matched {} in file contents.",
            visible(matched_term)
        )),
    }
    text
}

fn display_path(path: &Path) -> String {
    visible(&path.to_string_lossy())
}

fn visible(text: &str) -> String {
    let mut escaped = String::new();
    for ch in text.chars() {
        if ch.is_control() || ch == '\\' {
            escaped.extend(ch.escape_default());
        } else {
            escaped.push(ch);
        }
    }
    escaped
}
