use std::{collections::BTreeSet, io};

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::{DefaultTerminal, widgets::ListState};

use crate::{
    cleanup::{self, Outcome},
    discovery::{Item, Report, Search},
    locations::Locations,
    ui,
};

enum View {
    Search,
    Choose(Search),
    Results(Report, ResultsMode),
}

impl View {
    fn results(report: Report) -> Self {
        Self::Results(
            report,
            ResultsMode::Browse {
                empty_preview: false,
            },
        )
    }
}

enum ResultsMode {
    Browse { empty_preview: bool },
    Issues,
    Review(ReviewAction),
    Outcomes(Vec<Outcome>),
}

enum ReviewAction {
    Cancel,
    MoveToTrash,
}

pub fn run(terminal: &mut DefaultTerminal, locations: &Locations) -> io::Result<()> {
    let mut view = View::Search;
    let mut input = String::new();
    let mut error = "";
    let mut selection = ListState::default().with_selected(Some(0));
    let mut issue_selection = ListState::default().with_selected(Some(0));
    let mut marked = BTreeSet::<usize>::new();
    let mut detail_scroll = 0_u16;
    loop {
        terminal.draw(|frame| match &view {
            View::Search => ui::draw_search(frame, &input, error),
            View::Choose(check) => {
                ui::draw_choices(frame, &check.matches, &mut selection, detail_scroll)
            }
            View::Results(report, mode) => match mode {
                ResultsMode::Browse { empty_preview } => ui::draw_report(
                    frame,
                    report,
                    &mut selection,
                    &marked,
                    *empty_preview,
                    detail_scroll,
                ),
                ResultsMode::Issues => {
                    ui::draw_issues(frame, &report.issues, &mut issue_selection, detail_scroll)
                }
                ResultsMode::Review(action) => ui::draw_preview(
                    frame,
                    &preview_items(report, &marked),
                    matches!(action, ReviewAction::MoveToTrash),
                    detail_scroll,
                ),
                ResultsMode::Outcomes(outcomes) => {
                    ui::draw_outcomes(frame, outcomes, detail_scroll)
                }
            },
        })?;
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind == KeyEventKind::Release {
            continue;
        }
        if (key.code == KeyCode::Esc && !matches!(view, View::Results(_, ResultsMode::Review(_))))
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
                        error = "Enter a non-empty application name or Bundle ID, command name, or package name or ID.";
                    } else if query.contains(['/', '\\']) || matches!(query, "." | ".." | "~") {
                        error = "Enter a name or exact identifier, not a path.";
                    } else {
                        let check = Search::run(locations, query);
                        view = if check.matches.len() > 1 {
                            View::Choose(check)
                        } else {
                            View::results(check.report(locations, query, 0))
                        };
                    }
                }
                _ => {}
            },
            View::Choose(check) => {
                if key.code == KeyCode::Enter {
                    view = View::results(check.report(
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
            View::Results(ref report, ref mut mode) => match mode {
                ResultsMode::Browse { empty_preview } => {
                    *empty_preview = false;
                    match key.code {
                        KeyCode::Char('i') if !report.issues.is_empty() => {
                            *mode = ResultsMode::Issues;
                            detail_scroll = 0;
                        }
                        KeyCode::Char(' ') => {
                            if let Some(index) = selection.selected().filter(|&index| {
                                report
                                    .items
                                    .get(index)
                                    .is_some_and(|item| item.selection_blocker(report).is_none())
                            }) && !marked.remove(&index)
                            {
                                marked.insert(index);
                            }
                        }
                        KeyCode::Enter if !marked.is_empty() => {
                            *mode = ResultsMode::Review(ReviewAction::Cancel);
                            detail_scroll = 0;
                        }
                        KeyCode::Enter => *empty_preview = true,
                        _ => browse(&mut selection, key.code, report.items.len()),
                    }
                }
                ResultsMode::Issues => {
                    if key.code == KeyCode::Char('i') {
                        *mode = ResultsMode::Browse {
                            empty_preview: false,
                        };
                        detail_scroll = 0;
                    } else {
                        browse(&mut issue_selection, key.code, report.issues.len());
                    }
                }
                ResultsMode::Review(action) => match key.code {
                    KeyCode::Esc => {
                        *mode = ResultsMode::Browse {
                            empty_preview: false,
                        };
                        detail_scroll = 0;
                    }
                    KeyCode::Left => *action = ReviewAction::Cancel,
                    KeyCode::Right => *action = ReviewAction::MoveToTrash,
                    KeyCode::Enter if key.kind == KeyEventKind::Press => {
                        *mode = match action {
                            ReviewAction::Cancel => ResultsMode::Browse {
                                empty_preview: false,
                            },
                            ReviewAction::MoveToTrash => ResultsMode::Outcomes(
                                cleanup::move_reviewed(locations, &preview_items(report, &marked)),
                            ),
                        };
                        detail_scroll = 0;
                    }
                    _ => {}
                },
                ResultsMode::Outcomes(_) => {}
            },
        }
    }
}

fn preview_items<'a>(report: &'a Report, marked: &BTreeSet<usize>) -> Vec<&'a Item> {
    let selected = marked
        .iter()
        .map(|&index| &report.items[index])
        .filter(|item| item.selection_blocker(report).is_none());
    let directories: BTreeSet<_> = selected
        .clone()
        .filter(|item| item.file_type.is_dir())
        .map(|item| item.path.as_path())
        .collect();
    selected
        .filter(|item| {
            !item
                .path
                .ancestors()
                .skip(1)
                .any(|parent| directories.contains(parent))
        })
        .collect()
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
