use std::path::PathBuf;

use clap::Parser;

#[derive(Parser, Debug)]
pub struct Args {
    /// Run the runbook non-interactively
    #[arg(short, long, default_value = "true")]
    pub non_interactive: bool,

    /// Path to an .atrb file, or a @user/name identifier
    pub runbook: String,
}
