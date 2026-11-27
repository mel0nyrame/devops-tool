#[cfg(target_os = "macos")]
mod app;
#[cfg(target_os = "macos")]
mod bundle;
#[cfg(target_os = "macos")]
mod discovery;
#[cfg(target_os = "macos")]
mod locations;
#[cfg(target_os = "macos")]
mod ui;

#[cfg(target_os = "macos")]
fn main() -> std::io::Result<()> {
    use std::io::{self, IsTerminal};

    if std::env::args_os().len() != 1 {
        return Err(io::Error::other(
            "lfd does not take arguments; enter an application name or Bundle ID interactively",
        ));
    }
    let locations = locations::Locations::current()?;
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err(io::Error::other("lfd requires an interactive terminal"));
    }
    ratatui::run(|terminal| app::run(terminal, &locations))
}

#[cfg(not(target_os = "macos"))]
fn main() -> std::process::ExitCode {
    eprintln!("lfd is only supported on macOS");
    std::process::ExitCode::FAILURE
}
