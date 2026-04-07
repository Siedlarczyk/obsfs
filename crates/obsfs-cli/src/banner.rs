//! Startup banner and progress display.

use std::time::Instant;

// ANSI color codes
const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const GREEN: &str = "\x1b[32m";
const CYAN: &str = "\x1b[36m";
const YELLOW: &str = "\x1b[33m";
const RED: &str = "\x1b[31m";

// Logo width for alignment (visual width, not byte length)
const LOGO_WIDTH: usize = 14;

const LOGO: [&str; 12] = [
    "   ObsFS      ",
    "   ├──┬──●    ",
    "   │  └──●    ",
    "   ├──●       ",
    "   │  ┌──●    ",
    "   ├──┼──●    ",
    "   │  └──●    ",
    "   ├──●       ",
    "   │  ┌──●    ",
    "   └──┴──●    ",
    "              ",
    "              ",
];

/// Tracks startup progress and displays the banner.
pub struct StartupBanner {
    start_time: Instant,
    current_line: usize,
}

impl StartupBanner {
    pub fn new() -> Self {
        Self {
            start_time: Instant::now(),
            current_line: 0,
        }
    }

    /// Print the initial banner with version.
    pub fn print_header(&mut self, version: &str) {
        println!();
        self.print_line(&format!(
            "{BOLD}{CYAN}ObsFS{RESET} {DIM}v{version}{RESET}"
        ));
        self.print_line(&format!("{DIM}Initializing...{RESET}"));
    }

    /// Print a status line for a plugin/component.
    pub fn print_status(&mut self, name: &str, success: bool) {
        let status = if success {
            format!("{GREEN}OK{RESET}")
        } else {
            format!("{RED}FAIL{RESET}")
        };
        let dots = ".".repeat(20 - name.len().min(20));
        self.print_line(&format!(
            "{DIM}{name}{RESET} {DIM}{dots}{RESET} {status}"
        ));
    }

    /// Print the mount status.
    pub fn print_mount(&mut self, path: &str) {
        let label = format!("Mount: {}", path);
        let dots = ".".repeat(20 - label.len().min(20));
        self.print_line(&format!(
            "{BOLD}{label}{RESET} {DIM}{dots}{RESET} {GREEN}OK{RESET}"
        ));
    }

    /// Print the final ready message.
    pub fn print_ready(&mut self) {
        let elapsed = self.start_time.elapsed().as_secs_f64();

        // Print remaining empty logo lines if needed
        while self.current_line < LOGO.len() {
            println!("{CYAN}{}{RESET}", LOGO[self.current_line]);
            self.current_line += 1;
        }

        println!();
        println!(
            "{:width$} {BOLD}Ready.{RESET} {DIM}Observe everything as files.{RESET}",
            "",
            width = LOGO_WIDTH
        );
        println!(
            "{DIM}{:width$} Started in {:.3}s | PID {} | Ctrl+C to stop{RESET}",
            "",
            elapsed,
            std::process::id(),
            width = LOGO_WIDTH
        );
        println!();
    }

    fn print_line(&mut self, content: &str) {
        let elapsed = self.start_time.elapsed().as_secs_f64();
        let timestamp = format!("{DIM}[{:>6.3}]{RESET}", elapsed);

        if self.current_line < LOGO.len() {
            println!(
                "{CYAN}{}{RESET} {timestamp} {content}",
                LOGO[self.current_line]
            );
            self.current_line += 1;
        } else {
            // Padding to match LOGO width
            println!("{:width$} {timestamp} {content}", "", width = LOGO_WIDTH);
        }
    }
}

impl Default for StartupBanner {
    fn default() -> Self {
        Self::new()
    }
}

/// Print a simple shutdown message.
pub fn print_shutdown() {
    println!();
    println!("{DIM}[  ....]{RESET} {YELLOW}Shutting down...{RESET}");
}

/// Print unmount success.
pub fn print_unmounted() {
    println!("{DIM}[  ....]{RESET} {GREEN}Unmounted successfully{RESET}");
    println!();
}
