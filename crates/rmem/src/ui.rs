use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Cell, Clear, Gauge, Paragraph, Row, Table, TableState, Wrap,
    },
};

use crate::{
    app::{App, InputMode},
    format,
    process::MemorySummary,
};

pub fn draw(frame: &mut Frame, app: &App, memory: MemorySummary, table_offset: &mut usize) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),
            Constraint::Min(8),
            Constraint::Length(2),
        ])
        .split(area);

    draw_memory_gauges(frame, memory, chunks[0]);
    draw_process_table(frame, app, chunks[1], table_offset);
    draw_footer(frame, app, chunks[2]);
    draw_details(frame, app);
}

fn draw_memory_gauges(frame: &mut Frame, memory: MemorySummary, area: Rect) {
    let MemorySummary {
        total_memory,
        used_memory,
        total_swap,
        used_swap,
    } = memory;
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    let memory_ratio = ratio(used_memory, total_memory);

    let memory_label = if total_memory == 0 {
        "n/a".into()
    } else {
        format!(
            "{} / {}  {:>5.1}%",
            format::bytes(used_memory),
            format::bytes(total_memory),
            memory_ratio * 100.0
        )
    };

    let memory = Gauge::default()
        .block(panel(" RAM "))
        .gauge_style(
            Style::default()
                .fg(usage_color(memory_ratio * 100.0))
                .add_modifier(Modifier::BOLD),
        )
        .ratio(memory_ratio)
        .label(memory_label);

    frame.render_widget(memory, columns[0]);

    let swap_ratio = ratio(used_swap, total_swap);
    let swap_label = if total_swap == 0 {
        "n/a".to_string()
    } else {
        format!(
            "{} / {}  {:>5.1}%",
            format::bytes(used_swap),
            format::bytes(total_swap),
            swap_ratio * 100.0
        )
    };

    #[cfg(windows)]
    let swap_title = " SWAP (EST.) ";
    #[cfg(not(windows))]
    let swap_title = " SWAP ";
    let swap = Gauge::default()
        .block(panel(swap_title))
        .gauge_style(
            Style::default()
                .fg(usage_color(swap_ratio * 100.0))
                .add_modifier(Modifier::BOLD),
        )
        .ratio(swap_ratio)
        .label(swap_label);

    frame.render_widget(swap, columns[1]);
}

fn draw_process_table(frame: &mut Frame, app: &App, area: Rect, offset: &mut usize) {
    let visible = app.visible_processes();

    let header = Row::new([
        Cell::from("PID"),
        Cell::from("USER"),
        Cell::from("CPU%"),
        Cell::from("MEM"),
        Cell::from("MEM%"),
        Cell::from("ΔMEM"),
        Cell::from("TIME"),
        Cell::from("COMMAND"),
    ])
    .style(
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )
    .bottom_margin(1);

    let rows = visible.iter().map(|process| {
        Row::new([
            Cell::from(process.pid.to_string()),
            Cell::from(process.user.clone()),
            Cell::from(format!("{:.1}", process.cpu_percent))
                .style(Style::default().fg(cpu_color(process.cpu_percent))),
            Cell::from(format::bytes(process.memory)),
            match process.memory_percent {
                Some(percent) => Cell::from(format!("{percent:.2}"))
                    .style(Style::default().fg(usage_color(percent))),
                None => Cell::from("-"),
            },
            Cell::from(format::signed_bytes(process.memory_delta))
                .style(Style::default().fg(delta_color(process.memory_delta))),
            Cell::from(format::cpu_time(process.cpu_time_ms)),
            Cell::from(process.command.clone()),
        ])
    });

    let title = format!(
        " Processes {} / {}  ·  sorted by {} ",
        visible.len(),
        app.process_count(),
        app.sort_by.label()
    );

    let table = Table::new(
        rows,
        [
            Constraint::Length(8),
            Constraint::Length(12),
            Constraint::Length(8),
            Constraint::Length(12),
            Constraint::Length(8),
            Constraint::Length(12),
            Constraint::Length(11),
            Constraint::Min(20),
        ],
    )
    .header(header)
    .column_spacing(1)
    .block(panel(&title))
    .row_highlight_style(
        Style::default()
            .bg(Color::DarkGray)
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    );

    let mut state = TableState::new().with_offset(*offset);
    state.select(app.selected_row());
    frame.render_stateful_widget(table, area, &mut state);
    *offset = state.offset();
}

fn draw_footer(frame: &mut Frame, app: &App, area: Rect) {
    let mode_line = if app.input_mode == InputMode::Search {
        Line::from(vec![
            Span::styled(" SEARCH ", Style::default().fg(Color::Black).bg(Color::Cyan)),
            Span::raw(" "),
            Span::styled(&app.search, Style::default().fg(Color::White)),
            Span::styled("▌", Style::default().fg(Color::Cyan)),
            Span::styled("   Enter keep · Esc clear", Style::default().fg(Color::DarkGray)),
        ])
    } else if app.search.is_empty() {
        Line::from(vec![
            key("q"), text(" quit   "),
            key("↑↓/jk"), text(" select   "),
            key("gg/G"), text(" first/last   "),
            key("m"), text(" mem   "),
            key("c"), text(" cpu   "),
            key("p"), text(" pid   "),
            key("t"), text(" time   "),
            key("/"), text(" search   "),
            key("Enter"), text(" details   "),
            key("r"), text(" refresh"),
        ])
    } else {
        Line::from(vec![
            Span::styled(
                format!(" filter: {} ", app.search),
                Style::default().fg(Color::Black).bg(Color::Cyan),
            ),
            text("   "),
            key("Esc"), text(" clear   "),
            key("/"), text(" edit   "),
            key("Enter"), text(" details   "),
            key("gg/G"), text(" first/last"),
        ])
    };

    let paragraph = Paragraph::new(mode_line).alignment(Alignment::Center);
    frame.render_widget(paragraph, area);
}

fn draw_details(frame: &mut Frame, app: &App) {
    let Some(process) = app.details_process() else {
        return;
    };

    let area = centered_rect(76, 70, frame.area());
    frame.render_widget(Clear, area);

    let memory_metric = if cfg!(windows) { "MEM (working set)" } else { "MEM (RSS)" };
    let text = vec![
        detail_line("PID", process.pid.to_string()),
        detail_line("USER", process.user.clone()),
        detail_line("STATUS", process.status.clone()),
        detail_line("CPU", format!("{:.2}%", process.cpu_percent)),
        detail_line(memory_metric, format::bytes(process.memory)),
        detail_line("MEM%", process.memory_percent.map_or_else(|| "-".into(), |percent| format!("{percent:.2}%"))),
        detail_line("ΔMEM", format::signed_bytes(process.memory_delta)),
        detail_line("CPU TIME", format::cpu_time(process.cpu_time_ms)),
        detail_line("ELAPSED", format::elapsed(process.elapsed_seconds)),
        Line::from(""),
        Line::from(Span::styled(
            "COMMAND",
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        )),
        Line::from(process.command.clone()),
        Line::from(""),
        Line::from(Span::styled(
            "Enter / Esc / q  close",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    let detail_title = format!(" {} · PID {} ", process.name, process.pid);

    let paragraph = Paragraph::new(text)
        .block(panel(&detail_title))
        .wrap(Wrap { trim: false });

    frame.render_widget(paragraph, area);
}

fn detail_line(label: &'static str, value: String) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("{label:<12}"),
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ),
        Span::raw(value),
    ])
}

fn panel(title: &str) -> Block<'_> {
    Block::default()
        .title(title)
        .title_style(
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::DarkGray))
}

fn ratio(used: u64, total: u64) -> f64 {
    if total == 0 {
        0.0
    } else {
        (used as f64 / total as f64).clamp(0.0, 1.0)
    }
}

fn usage_color(percent: f64) -> Color {
    if percent >= 85.0 {
        Color::Red
    } else if percent >= 65.0 {
        Color::Yellow
    } else {
        Color::Green
    }
}

fn cpu_color(percent: f32) -> Color {
    if percent >= 100.0 {
        Color::Red
    } else if percent >= 50.0 {
        Color::Yellow
    } else {
        Color::Green
    }
}

fn delta_color(delta: i64) -> Color {
    if delta > 0 {
        Color::Red
    } else if delta < 0 {
        Color::Green
    } else {
        Color::DarkGray
    }
}

fn key(value: &'static str) -> Span<'static> {
    Span::styled(
        value,
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )
}

fn text(value: &'static str) -> Span<'static> {
    Span::styled(value, Style::default().fg(Color::DarkGray))
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}
