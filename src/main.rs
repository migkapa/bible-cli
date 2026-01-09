mod ai;
mod books;
mod cache;
mod cli;
mod commands;
mod moods;
mod output;
mod reference;
mod verses;

use anyhow::Result;
use clap::Parser;

use crate::cli::Commands;

fn main() -> Result<()> {
    let cli = cli::Cli::parse();
    let paths = cache::cache_paths(cli.data_dir.clone());
    let output = output::OutputStyle::new(cli.color);

    match &cli.command {
        Commands::Cache(args) => commands::run_cache(args, &paths),
        Commands::Read(args) => commands::run_read(args, &paths, &output),
        Commands::Search(args) => commands::run_search(args, &paths, &output),
        Commands::Today => commands::run_today(&paths, &output),
        Commands::Random => commands::run_random(&paths, &output),
        Commands::Echo(args) => commands::run_echo(args, &paths, &output),
        Commands::Mood(args) => commands::run_mood(args, &paths, &output),
    }
}
