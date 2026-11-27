use std::{collections::BTreeSet, path::Path};

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    widgets::{Block, List, ListItem, ListState, Paragraph, Wrap},
};

use crate::discovery::{
    Application, ArtifactKind, BundleMatch, CheckIssue, CheckSource, Evidence, Item, MainProgram,
    Report,
};

pub fn draw_search(frame: &mut Frame, input: &str, error: &str) {
    let input = visible(input);
    let text =
        format!("Application name or Bundle ID:\n> {input}\n\n{error}\n\nEnter search / Esc quit");
    frame.render_widget(panel(text, " lfd · read-only inspection ", 0), frame.area());
}

pub fn draw_choices(
    frame: &mut Frame,
    apps: &[Application],
    selection: &mut ListState,
    scroll: u16,
) {
    let [main, footer] =
        Layout::vertical([Constraint::Min(1), Constraint::Length(2)]).areas(frame.area());
    let [left, right] = columns(main, 60);
    let rows = apps.iter().map(|app| {
        ListItem::new(format!(
            "{}\n{}\n",
            visible(app.identifier().unwrap_or("Bundle ID unavailable")),
            display_path(&app.path),
        ))
    });
    frame.render_stateful_widget(list(rows, " Choose one application "), left, selection);
    if let Some(app) = selection.selected().and_then(|index| apps.get(index)) {
        let details = format!(
            "Bundle ID: {}\n\n{}",
            visible(app.identifier().unwrap_or("unavailable")),
            display_path(&app.path)
        );
        frame.render_widget(panel(details, " Installation ", scroll), right);
    }
    frame.render_widget(Paragraph::new("↑/↓ browse · PgUp/PgDn details · Enter choose · q / Esc quit\nName-only artifacts are not assigned to either identity."), footer);
}

pub fn draw_report(
    frame: &mut Frame,
    report: &Report,
    selection: &mut ListState,
    marked: &BTreeSet<usize>,
    empty_preview: bool,
    scroll: u16,
) {
    let selection_status = match report.selection_program {
        MainProgram::Incomplete => "Selection disabled: required main program checks incomplete.\n",
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
    let rows = report.items.iter().enumerate().map(|(index, item)| {
        let name = visible(&item.path.file_name().unwrap_or_default().to_string_lossy());
        if item.selection_blocker(&report.selection_program).is_some() {
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
            |item| evidence(item, &report.selection_program),
        );
    frame.render_widget(panel(text, " Evidence ", scroll), right);
    frame.render_widget(
        Paragraph::new(format!(
            "{selection_status}Selected: {} · {selection_line} · Read-only\n↑/↓ browse · PgUp/PgDn evidence · q / Esc quit",
            marked.len()
        )),
        footer,
    );
}

pub fn draw_preview(frame: &mut Frame, items: &[&Item], scroll: u16) {
    let [body, footer] =
        Layout::vertical([Constraint::Min(1), Constraint::Length(2)]).areas(frame.area());
    let mut text = String::from("Preview only: no files will be moved or deleted.\n\n");
    for item in items {
        text.push_str(&display_path(&item.path));
        text.push('\n');
        if item.file_type.is_dir() {
            text.push_str("Includes all descendants.\n");
        }
        text.push_str(&format!("Risk: {}.\n\n", item.kind.risk()));
    }
    frame.render_widget(panel(text, " Cleanup preview ", scroll), body);
    frame.render_widget(
        Paragraph::new("[ Cancel ] · Enter / Esc return · PgUp/PgDn scroll · q quit"),
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

fn evidence(item: &Item, program: &MainProgram) -> String {
    let kind = match item.kind {
        ArtifactKind::ApplicationBundle => "Application bundle",
        ArtifactKind::ApplicationData => "Application data",
        ArtifactKind::Preferences => "Preferences",
        ArtifactKind::Cache => "Cache",
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
    if let Some(reason) = item.selection_blocker(program) {
        text.push_str(&format!("View only: {reason}.\n\n"));
    } else {
        text.push_str("Eligible for preview (Space to select or deselect).\n\n");
    }
    if item.file_type.is_symlink() {
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
        if ch.is_control() {
            escaped.extend(ch.escape_default());
        } else {
            escaped.push(ch);
        }
    }
    escaped
}
