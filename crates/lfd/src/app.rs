use std::io;

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::{DefaultTerminal, widgets::ListState};

use crate::{
    discovery::{ApplicationCheck, Report},
    locations::Locations,
    ui,
};

enum View {
    Search,
    Choose(ApplicationCheck),
    Results(Report),
}

pub fn run(terminal: &mut DefaultTerminal, locations: &Locations) -> io::Result<()> {
    let mut view = View::Search;
    let mut input = String::new();
    let mut error = "";
    let mut selection = ListState::default().with_selected(Some(0));
    let mut issue_selection = ListState::default().with_selected(Some(0));
    let mut show_issues = false;
    let mut detail_scroll = 0_u16;
    loop {
        terminal.draw(|frame| match &view {
            View::Search => ui::draw_search(frame, &input, error),
            View::Choose(check) => {
                ui::draw_choices(frame, &check.matches, &mut selection, detail_scroll)
            }
            View::Results(report) if show_issues => {
                ui::draw_issues(frame, &report.issues, &mut issue_selection, detail_scroll)
            }
            View::Results(report) => ui::draw_report(frame, report, &mut selection, detail_scroll),
        })?;
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind == KeyEventKind::Release {
            continue;
        }
        if key.code == KeyCode::Esc
            || key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL)
            || key.code == KeyCode::Char('q') && !matches!(view, View::Search)
        {
            return Ok(());
        }

        match key.code {
            KeyCode::PageDown => detail_scroll = detail_scroll.saturating_add(5),
            KeyCode::PageUp => detail_scroll = detail_scroll.saturating_sub(5),
            KeyCode::Up | KeyCode::Down => detail_scroll = 0,
            _ => {}
        }
        match view {
            View::Search => match key.code {
                KeyCode::Char(ch)
                    if !key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                {
                    input.push(ch);
                    error = "";
                }
                KeyCode::Backspace => {
                    input.pop();
                    error = "";
                }
                KeyCode::Enter => {
                    let query = input.trim();
                    if query.is_empty() {
                        error = "Enter a non-empty application name or Bundle ID.";
                    } else if query.contains(['/', '\\']) || matches!(query, "." | ".." | "~") {
                        error = "Enter an application name or Bundle ID, not a path.";
                    } else {
                        let check = ApplicationCheck::run(locations, query);
                        view = if check.matches.len() > 1 {
                            View::Choose(check)
                        } else {
                            View::Results(check.report(locations, query, 0))
                        };
                    }
                }
                _ => {}
            },
            View::Choose(check) => {
                if key.code == KeyCode::Enter {
                    view = View::Results(check.report(
                        locations,
                        input.trim(),
                        selection.selected().unwrap_or(0),
                    ));
                    selection = ListState::default().with_selected(Some(0));
                    detail_scroll = 0;
                } else {
                    browse(&mut selection, key.code, check.matches.len());
                    view = View::Choose(check);
                }
            }
            View::Results(ref report) => {
                if key.code == KeyCode::Char('i') && !report.issues.is_empty() {
                    show_issues = !show_issues;
                    detail_scroll = 0;
                } else if show_issues {
                    browse(&mut issue_selection, key.code, report.issues.len());
                } else {
                    browse(&mut selection, key.code, report.items.len());
                }
            }
        }
    }
}

fn browse(selection: &mut ListState, key: KeyCode, len: usize) {
    if len == 0 {
        return;
    }
    let index = selection.selected().unwrap_or(0);
    match key {
        KeyCode::Up => selection.select(Some(index.saturating_sub(1))),
        KeyCode::Down => selection.select(Some((index + 1).min(len - 1))),
        _ => {}
    }
}
