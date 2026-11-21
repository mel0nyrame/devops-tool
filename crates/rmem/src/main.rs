mod app;
mod format;
mod process;
mod ui;

use std::io;

fn main() -> io::Result<()> {
    ratatui::run(app::run)
}
