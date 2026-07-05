mod ai;
mod books;
mod cache;
mod cli;
mod commands;
mod moods;
mod output;
mod plans;
mod reference;
mod topics;
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

    // Resolve the active translation: --translation flag > configured default > "kjv".
    let root = cli
        .data_dir
        .clone()
        .unwrap_or_else(cache::default_cache_root);
    let translation = cli
        .translation
        .clone()
        .or_else(|| cache::load_default_translation(&root))
        .unwrap_or_else(|| cache::DEFAULT_TRANSLATION.to_string());
    let paths = cache::CachePaths::new(root, translation);
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
        Commands::Parallel(args) => commands::run_parallel(args, &paths, &output),
        Commands::Diff(args) => commands::run_diff(args, &paths, &output),
        Commands::Plan(args) => commands::run_plan(args, &paths, &output),
        Commands::Export(args) => commands::run_export(args, &paths, &output),
        Commands::Topic(args) => commands::run_topic(args, &paths, &output),
        Commands::Translation(args) => commands::run_translation(args, &paths),
        Commands::Completions(_) => unreachable!("handled above"),
    }
}
