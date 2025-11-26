use std::io::IsTerminal;

use clap::Parser;

#[derive(Parser, Debug)]
pub struct Args {
    /// Run the runbook non-interactively (auto-detected from TTY if not specified)
    #[arg(short, long)]
    pub non_interactive: bool,

    /// Path to an .atrb file, or a @user/name identifier
    pub runbook: String,
}

impl Args {
    /// Determine if we should run in interactive mode
    ///
    /// Returns false (non-interactive) if:
    /// - --non-interactive flag is set
    /// - NO_TTY environment variable is set
    /// - CI environment variable is set
    /// - stdout is not a TTY
    pub fn is_interactive(&self) -> bool {
        if self.non_interactive {
            return false;
        }

        // Check environment variables
        if std::env::var("NO_TTY").is_ok() {
            return false;
        }

        if std::env::var("CI").is_ok() {
            return false;
        }

        // Check if stdout is a TTY
        std::io::stdout().is_terminal()
    }
}
