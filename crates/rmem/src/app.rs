use std::{
    cmp::Ordering,
    io,
    time::{Duration, Instant},
};

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::DefaultTerminal;

use crate::{
    process::{ProcessCollector, ProcessInfo},
    ui,
};

const REFRESH_INTERVAL: Duration = Duration::from_secs(1);
const EVENT_POLL_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortBy {
    Memory,
    Cpu,
    Pid,
    Time,
}

impl SortBy {
    pub fn label(self) -> &'static str {
        match self {
            Self::Memory => "MEM ↓",
            Self::Cpu => "CPU% ↓",
            Self::Pid => "PID ↓",
            Self::Time => "TIME ↓",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    Search,
}

#[derive(Debug, PartialEq, Eq)]
enum KeyOutcome {
    NoRefresh,
    Refresh,
}

pub fn run(terminal: &mut DefaultTerminal) -> io::Result<()> {
    let mut collector = ProcessCollector::new();
    let mut app = App::default();
    let mut table_offset = 0;
    app.apply_snapshot(collector.refresh());
    let mut last_refresh = Instant::now();

    while !app.should_quit {
        if last_refresh.elapsed() >= REFRESH_INTERVAL {
            app.apply_snapshot(collector.refresh());
            last_refresh = Instant::now();
        }

        terminal.draw(|frame| {
            ui::draw(frame, &app, collector.memory_summary(), &mut table_offset);
        })?;

        if event::poll(EVENT_POLL_INTERVAL)? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press && app.handle_key(key) == KeyOutcome::Refresh {
                    app.apply_snapshot(collector.refresh());
                    last_refresh = Instant::now();
                }
            }
        }
    }

    Ok(())
}

pub struct App {
    processes: Vec<ProcessInfo>,
    selected_row: Option<usize>,
    pub sort_by: SortBy,
    pub input_mode: InputMode,
    pub search: String,
    show_details: bool,
    should_quit: bool,
}

impl Default for App {
    fn default() -> Self {
        Self {
            processes: Vec::new(),
            selected_row: None,
            sort_by: SortBy::Memory,
            input_mode: InputMode::Normal,
            search: String::new(),
            show_details: false,
            should_quit: false,
        }
    }
}

impl App {
    fn apply_snapshot(&mut self, processes: Vec<ProcessInfo>) {
        let selected_pid = self.selected_process().map(|process| process.pid);
        self.processes = processes;
        self.sort_processes();
        self.restore_selection(selected_pid);
    }

    pub fn visible_processes(&self) -> Vec<&ProcessInfo> {
        let query = self.search.trim().to_lowercase();

        self.processes
            .iter()
            .filter(|process| {
                query.is_empty()
                    || process.pid.to_string().contains(&query)
                    || process.user.to_lowercase().contains(&query)
                    || process.name.to_lowercase().contains(&query)
                    || process.command.to_lowercase().contains(&query)
            })
            .collect()
    }

    pub fn process_count(&self) -> usize {
        self.processes.len()
    }

    pub fn selected_row(&self) -> Option<usize> {
        self.selected_row
    }

    pub fn selected_process(&self) -> Option<&ProcessInfo> {
        self.visible_processes().get(self.selected_row?).copied()
    }

    pub fn details_process(&self) -> Option<&ProcessInfo> {
        if self.show_details {
            self.selected_process()
        } else {
            None
        }
    }

    fn handle_key(&mut self, key: KeyEvent) -> KeyOutcome {
        if self.show_details {
            match key.code {
                KeyCode::Enter | KeyCode::Esc | KeyCode::Char('q') => {
                    self.show_details = false;
                }
                _ => {}
            }
            return KeyOutcome::NoRefresh;
        }

        match self.input_mode {
            InputMode::Search => {
                self.handle_search_key(key);
                KeyOutcome::NoRefresh
            }
            InputMode::Normal => self.handle_normal_key(key),
        }
    }

    fn handle_normal_key(&mut self, key: KeyEvent) -> KeyOutcome {
        match key.code {
            KeyCode::Char('r') => return KeyOutcome::Refresh,
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('m') => self.set_sort(SortBy::Memory),
            KeyCode::Char('c') => self.set_sort(SortBy::Cpu),
            KeyCode::Char('p') => self.set_sort(SortBy::Pid),
            KeyCode::Char('t') => self.set_sort(SortBy::Time),
            KeyCode::Char('/') => self.input_mode = InputMode::Search,
            KeyCode::Down | KeyCode::Char('j') => self.select_next(),
            KeyCode::Up | KeyCode::Char('k') => self.select_previous(),
            KeyCode::Home => self.select_first(),
            KeyCode::End => self.select_last(),
            KeyCode::Enter => {
                if self.selected_process().is_some() {
                    self.show_details = true;
                }
            }
            KeyCode::Esc => {
                if !self.search.is_empty() {
                    self.search.clear();
                    self.clamp_selection();
                }
            }
            _ => {}
        }
        KeyOutcome::NoRefresh
    }

    fn handle_search_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.search.clear();
                self.input_mode = InputMode::Normal;
                self.clamp_selection();
            }
            KeyCode::Enter => {
                self.input_mode = InputMode::Normal;
                self.clamp_selection();
            }
            KeyCode::Backspace => {
                self.search.pop();
                self.clamp_selection();
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.search.clear();
                self.clamp_selection();
            }
            KeyCode::Char(ch) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.search.push(ch);
                self.clamp_selection();
            }
            _ => {}
        }
    }

    fn set_sort(&mut self, sort_by: SortBy) {
        let selected_pid = self.selected_process().map(|process| process.pid);
        self.sort_by = sort_by;
        self.sort_processes();
        self.restore_selection(selected_pid);
    }

    fn sort_processes(&mut self) {
        match self.sort_by {
            SortBy::Memory => self
                .processes
                .sort_unstable_by(|a, b| b.memory.cmp(&a.memory).then_with(|| b.pid.cmp(&a.pid))),
            SortBy::Cpu => self.processes.sort_unstable_by(|a, b| {
                b.cpu_percent
                    .partial_cmp(&a.cpu_percent)
                    .unwrap_or(Ordering::Equal)
                    .then_with(|| b.memory.cmp(&a.memory))
            }),
            SortBy::Pid => self.processes.sort_unstable_by(|a, b| b.pid.cmp(&a.pid)),
            SortBy::Time => self.processes.sort_unstable_by(|a, b| {
                b.cpu_time_ms
                    .cmp(&a.cpu_time_ms)
                    .then_with(|| b.memory.cmp(&a.memory))
            }),
        }
    }

    fn select_next(&mut self) {
        let len = self.visible_processes().len();
        self.selected_row = if len == 0 {
            None
        } else {
            Some(self.selected_row.map_or(0, |index| (index + 1) % len))
        };
    }

    fn select_previous(&mut self) {
        let len = self.visible_processes().len();
        self.selected_row = self
            .selected_row
            .and_then(|index| index.checked_sub(1))
            .or_else(|| len.checked_sub(1));
    }

    fn select_first(&mut self) {
        self.selected_row = (!self.visible_processes().is_empty()).then_some(0);
    }

    fn select_last(&mut self) {
        self.selected_row = self.visible_processes().len().checked_sub(1);
    }

    fn clamp_selection(&mut self) {
        self.restore_selection(None);
    }

    fn restore_selection(&mut self, pid: Option<u32>) {
        let visible = self.visible_processes();
        self.selected_row = pid
            .and_then(|pid| visible.iter().position(|process| process.pid == pid))
            .or_else(|| {
                visible
                    .len()
                    .checked_sub(1)
                    .map(|last| self.selected_row.unwrap_or(0).min(last))
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn process(pid: u32, memory: u64, command: &str) -> ProcessInfo {
        ProcessInfo {
            pid,
            user: "alice".into(),
            cpu_percent: 0.0,
            memory,
            memory_percent: None,
            memory_delta: 0,
            cpu_time_ms: 0,
            elapsed_seconds: 0,
            command: command.into(),
            name: "worker".into(),
            status: "Run".into(),
        }
    }

    fn press(app: &mut App, code: KeyCode) -> KeyOutcome {
        app.handle_key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    fn search(app: &mut App, query: &str) {
        press(app, KeyCode::Char('/'));
        for ch in query.chars() {
            press(app, KeyCode::Char(ch));
        }
        press(app, KeyCode::Enter);
    }

    fn visible_pids(app: &App) -> Vec<u32> {
        app.visible_processes()
            .iter()
            .map(|process| process.pid)
            .collect()
    }

    #[test]
    fn filtered_selection_follows_pid_through_sort_and_refresh_into_details() {
        let mut app = App::default();
        app.apply_snapshot(vec![
            process(11, 50, "serve api"),
            process(22, 30, "serve ui"),
            process(33, 70, "backup"),
        ]);

        search(&mut app, "SERVE");
        assert_eq!(visible_pids(&app), [11, 22]);
        press(&mut app, KeyCode::Down);
        assert_eq!(app.selected_process().unwrap().pid, 22);
        press(&mut app, KeyCode::Char('p'));
        assert_eq!(visible_pids(&app), [22, 11]);
        assert_eq!(app.selected_process().unwrap().pid, 22);

        assert_eq!(press(&mut app, KeyCode::Char('r')), KeyOutcome::Refresh);
        app.apply_snapshot(vec![
            process(44, 10, "serve cache"),
            process(22, 25, "serve ui updated"),
            process(11, 90, "serve api"),
        ]);
        assert_eq!(visible_pids(&app), [44, 22, 11]);
        assert_eq!(app.selected_process().unwrap().pid, 22);

        press(&mut app, KeyCode::Enter);
        let details = app.details_process().unwrap();
        assert_eq!(details.pid, 22);
        assert_eq!(details.command, "serve ui updated");
        press(&mut app, KeyCode::Esc);
        assert!(app.details_process().is_none());
        assert_eq!(app.selected_process().unwrap().pid, 22);
    }

    #[test]
    fn missing_pid_clamps_to_last_visible_row_and_empty_results_have_no_target() {
        let mut app = App::default();
        app.apply_snapshot(vec![
            process(11, 50, "serve api"),
            process(22, 30, "serve ui"),
            process(33, 10, "serve cache"),
        ]);
        press(&mut app, KeyCode::End);
        assert_eq!(app.selected_process().unwrap().pid, 33);

        app.apply_snapshot(vec![
            process(11, 50, "serve api"),
            process(22, 30, "serve ui"),
        ]);
        assert_eq!(visible_pids(&app), [11, 22]);
        assert_eq!(app.selected_process().unwrap().pid, 22);

        search(&mut app, "missing");
        assert!(visible_pids(&app).is_empty());
        press(&mut app, KeyCode::Up);
        press(&mut app, KeyCode::Enter);
        assert!(app.selected_process().is_none());
        assert!(app.details_process().is_none());

        press(&mut app, KeyCode::Esc);
        assert_eq!(visible_pids(&app), [11, 22]);
        assert_eq!(app.selected_process().unwrap().pid, 11);
        app.apply_snapshot(vec![]);
        assert!(app.selected_process().is_none());
    }
}
