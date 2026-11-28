use std::path::Path;

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    widgets::{Block, List, ListItem, ListState, Paragraph, Wrap},
};

use crate::discovery::{
    ArtifactKind, BundleMatch, CheckIssue, CheckSource, Evidence, Identity, Item, MainProgram,
    Report,
};

pub fn draw_search(frame: &mut Frame, input: &str, error: &str) {
    let input = visible(input);
    let text = format!(
        "Application name or Bundle ID or .pkg package ID:\n> {input}\n\n{error}\n\nEnter search / Esc quit"
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
        let (label, identifier) = identity_identifier(identity);
        ListItem::new(format!(
            "{label}: {}\n{}\n",
            visible(identifier),
            display_path(identity.path()),
        ))
    });
    frame.render_stateful_widget(list(rows, " Choose one application "), left, selection);
    if let Some(identity) = selection.selected().and_then(|index| identities.get(index)) {
        let (label, identifier) = identity_identifier(identity);
        let details = format!(
            "{label}: {}\n\n{}",
            visible(identifier),
            display_path(identity.path())
        );
        frame.render_widget(panel(details, " Installation evidence ", scroll), right);
    }
    frame.render_widget(Paragraph::new("↑/↓ browse · PgUp/PgDn details · Enter choose · q / Esc quit\nName-only artifacts are not assigned to any identity."), footer);
}

fn identity_identifier(identity: &Identity) -> (&str, &str) {
    match identity {
        Identity::Application(app) => ("Bundle ID", app.identifier().unwrap_or("unavailable")),
        Identity::PackageReceipt(receipt) => ("Package ID", &receipt.identifier),
    }
}

pub fn draw_report(frame: &mut Frame, report: &Report, selection: &mut ListState, scroll: u16) {
    let [header, main, footer] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .areas(frame.area());
    let [left, right] = columns(main, 32);
    let (status, title) = match report.main_program {
        MainProgram::Found => (
            "Main program found in the checked application directories.",
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
    let rows = report
        .items
        .iter()
        .map(|item| visible(&item.path.file_name().unwrap_or_default().to_string_lossy()));
    frame.render_stateful_widget(list(rows, title), left, selection);
    let text = selection
        .selected()
        .and_then(|index| report.items.get(index))
        .map_or_else(|| "No matching items.".into(), evidence);
    frame.render_widget(panel(text, " Evidence ", scroll), right);
    frame.render_widget(
        Paragraph::new("↑/↓ browse · PgUp/PgDn evidence · q / Esc quit · Read-only"),
        footer,
    );
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

fn evidence(item: &Item) -> String {
    let (kind, risk) = match item.kind {
        ArtifactKind::ApplicationBundle => {
            ("Application bundle", "may contain an installed application")
        }
        ArtifactKind::ApplicationData => ("Application data", "may contain retained user data"),
        ArtifactKind::Preferences => ("Preferences", "may contain retained settings"),
        ArtifactKind::Cache => ("Cache", "contents and ownership have not been verified"),
        ArtifactKind::PackageReceipt => ("Registered .pkg receipt", "historical evidence only"),
        ArtifactKind::Configuration => ("Configuration", "may contain retained settings"),
        ArtifactKind::State => ("State", "may contain retained sessions or user data"),
        ArtifactKind::Logs => ("Logs", "may contain retained activity records"),
        ArtifactKind::TemporaryData => (
            "Temporary data",
            "contents and ownership have not been verified",
        ),
        ArtifactKind::SharedConfiguration => (
            "Shared configuration",
            "may contain settings for another tool",
        ),
        ArtifactKind::ExtensionSource => ("Extension source", "may contain code for another tool"),
    };
    let mut text = format!(
        "{}\n\nType: {kind}\nRisk: {risk}\n\n",
        display_path(&item.path)
    );
    if item.symbolic_link {
        text.push_str("Symbolic link: target not inspected.\n");
    }
    match &item.evidence {
        Evidence::ApplicationBundle(app) => {
            let field = match app.matched_by {
                BundleMatch::DirectoryName => "bundle directory name",
                BundleMatch::DisplayName => "CFBundleDisplayName",
                BundleMatch::Name => "CFBundleName",
                BundleMatch::Identifier => "CFBundleIdentifier",
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
        }
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
            text.push_str("View only: does not establish current ownership or installation.");
        }
        Evidence::PackageReceiptPath {
            identifier,
            receipt,
            verified_bundle_id,
        } => {
            text.push_str(&format!(
                "Recorded by the registered .pkg receipt for {}.\nReceipt: {}\nView only: the path exists, but the receipt does not establish current ownership or installation.",
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
        Evidence::SharedReference { matched_term } => text.push_str(&format!(
            "Shared reference: matched {} in file contents.\nView only: a reference does not establish ownership of this file.",
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
        if ch.is_control() {
            escaped.extend(ch.escape_default());
        } else {
            escaped.push(ch);
        }
    }
    escaped
}
