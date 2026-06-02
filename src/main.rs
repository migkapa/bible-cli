mod ai;
mod books;
mod cache;
mod cli;
mod commands;
mod moods;
mod output;
mod reference;
mod tui;
mod verses;

use anyhow::Result;
use clap::{CommandFactory, Parser};

use crate::cli::{Cli, Commands};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Completions need no cache or data; handle before any loading.
    if let Commands::Completions(args) = &cli.command {
        let mut cmd = Cli::command();
        let bin = cmd.get_name().to_string();
        clap_complete::generate(args.shell, &mut cmd, bin, &mut std::io::stdout());
        return Ok(());
    }

    let paths = cache::cache_paths(cli.data_dir.clone());
    let output = output::OutputStyle::new(cli.color, cli.resolved_format());

    match &cli.command {
        Commands::Cache(args) => commands::run_cache(args, &paths),
        Commands::Read(args) => commands::run_read(args, &paths, &output),
        Commands::Search(args) => commands::run_search(args, &paths, &output),
        Commands::Today(args) => commands::run_today(args, &paths, &output),
        Commands::Random(args) => commands::run_random(args, &paths, &output),
        Commands::Echo(args) => commands::run_echo(args, &paths, &output),
        Commands::Mood(args) => commands::run_mood(args, &paths, &output),
        Commands::Ai(args) => commands::run_ai(args, &paths, &output).await,
        Commands::Tui(args) => commands::run_tui(args, &paths),
        Commands::Completions(_) => unreachable!("handled above"),
    }
}
