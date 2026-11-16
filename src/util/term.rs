//! Tiny ANSI helpers for the non-TUI output.

use std::io::IsTerminal;

/// Whether stdout is a terminal and colours are allowed.
pub fn stdout_colors() -> bool {
    std::env::var_os("NO_COLOR").is_none() && std::io::stdout().is_terminal()
}

/// Whether stderr is a terminal and colours are allowed.
pub fn stderr_colors() -> bool {
    std::env::var_os("NO_COLOR").is_none() && std::io::stderr().is_terminal()
}

fn wrap(code: &str, text: &str, enabled: bool) -> String {
    if enabled {
        format!("\x1b[{code}m{text}\x1b[0m")
    } else {
        text.to_string()
    }
}

pub fn bold(text: &str) -> String {
    wrap("1", text, stdout_colors())
}

pub fn dim(text: &str) -> String {
    wrap("2", text, stdout_colors())
}

pub fn green(text: &str) -> String {
    wrap("32", text, stdout_colors())
}

pub fn red(text: &str) -> String {
    wrap("31", text, stdout_colors())
}

pub fn yellow(text: &str) -> String {
    wrap("33", text, stdout_colors())
}

pub fn cyan(text: &str) -> String {
    wrap("36", text, stdout_colors())
}

pub fn magenta(text: &str) -> String {
    wrap("35", text, stdout_colors())
}

/// Banner printed by `sorce doctor`, `sorce --version` and friends.
pub fn banner() -> String {
    let art = r"
   ███████╗ ██████╗ ██████╗  ██████╗███████╗██████╗ ███████╗██████╗
   ██╔════╝██╔═══██╗██╔══██╗██╔════╝██╔════╝██╔══██╗██╔════╝██╔══██╗
   ███████╗██║   ██║██████╔╝██║     █████╗  ██████╔╝█████╗  ██████╔╝
   ╚════██║██║   ██║██╔══██╗██║     ██╔══╝  ██╔══██╗██╔══╝  ██╔══██╗
   ███████║╚██████╔╝██║  ██║╚██████╗███████╗██║  ██║███████╗██║  ██║
   ╚══════╝ ╚═════╝ ╚═╝  ╚═╝ ╚═════╝╚══════╝╚═╝  ╚═╝╚══════╝╚═╝  ╚═╝";
    format!(
        "{}\n   {} · {}\n",
        magenta(art),
        crate::version_line(),
        crate::TAGLINE
    )
}
