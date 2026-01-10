use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "bible", version, about = "A fast, playful Bible CLI (KJV MVP)")]
pub struct Cli {
    #[arg(long, global = true, value_name = "DIR")]
    pub data_dir: Option<PathBuf>,

    #[arg(long, global = true, value_enum, default_value_t = ColorMode::Auto)]
    pub color: ColorMode,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    Read(ReadArgs),
    Search(SearchArgs),
    Today,
    Random,
    Echo(EchoArgs),
    Mood(MoodArgs),
    Cache(CacheArgs),
    Ai(AiArgs),
}

#[derive(Copy, Clone, Debug, ValueEnum)]
pub enum ColorMode {
    Auto,
    Always,
    Never,
}

#[derive(Args)]
pub struct ReadArgs {
    #[arg(required = true)]
    pub reference: Vec<String>,
}

#[derive(Args)]
pub struct SearchArgs {
    pub query: String,

    #[arg(long)]
    pub book: Option<String>,

    #[arg(long, default_value_t = 5)]
    pub limit: usize,
}

#[derive(Args)]
pub struct EchoArgs {
    #[arg(required = true)]
    pub reference: Vec<String>,

    #[arg(long, default_value_t = 2)]
    pub window: u16,
}

#[derive(Args)]
pub struct MoodArgs {
    pub mood: Option<String>,

    #[arg(long)]
    pub list: bool,
}

#[derive(Args)]
pub struct CacheArgs {
    #[arg(long)]
    pub preload: bool,

    #[arg(long)]
    pub source: Option<String>,
}

#[derive(Args)]
pub struct AiArgs {
    #[arg(required = true)]
    pub reference: Vec<String>,

    #[arg(long, default_value = "openai")]
    pub provider: String,

    #[arg(long, default_value = "gpt-4o-mini")]
    pub model: String,

    #[arg(long, default_value_t = 256)]
    pub max_tokens: u32,

    #[arg(long, default_value_t = 0.7)]
    pub temperature: f32,

    #[arg(long, default_value_t = 0)]
    pub window: u16,

    #[arg(long, help = "Start an interactive chat session with the selected passage")]
    pub chat: bool,
}
