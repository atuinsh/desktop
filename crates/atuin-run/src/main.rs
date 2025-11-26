use clap::Parser;
use eyre::Result;

use crate::{app::Args, executor::Executor};

mod app;
mod executor;
mod runbooks;
mod runtime;
mod ui;

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let runbook = runbooks::load_runbook(&args.runbook).await?;

    let interactive = args.is_interactive();
    let mut executor = Executor::new(runbook, interactive);
    match executor.execute().await {
        Err(e) => Err(eyre::eyre!(e)),
        Ok(()) => Ok(()),
    }
}
