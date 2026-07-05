use clap::{Args, Parser, Subcommand, ValueEnum};
use clap_complete::Shell;
use std::path::PathBuf;

pub use crate::output::Format;

#[derive(Parser)]
#[command(name = "bible", version, about = "A fast, playful Bible CLI (KJV MVP)")]
pub struct Cli {
    #[arg(long, global = true, value_name = "DIR")]
    pub data_dir: Option<PathBuf>,

    #[arg(long, global = true, value_enum, default_value_t = ColorMode::Auto)]
    pub color: ColorMode,

    /// Output format. Turns verse output into a scriptable data source.
    #[arg(long, global = true, value_enum, value_name = "FORMAT")]
    pub format: Option<Format>,

    /// Shorthand for `--format json`.
    #[arg(long, global = true, conflicts_with_all = ["format", "raw"])]
    pub json: bool,

    /// Shorthand for `--format raw` (verse text only).
    #[arg(long, global = true, conflicts_with_all = ["format", "json"])]
    pub raw: bool,

    /// Translation id to read from (default: configured default, else "kjv").
    #[arg(short = 't', long, global = true, value_name = "ID")]
    pub translation: Option<String>,

    #[command(subcommand)]
    pub command: Commands,
}

impl Cli {
    /// Resolve the effective output format from `--format`/`--json`/`--raw`.
    pub fn resolved_format(&self) -> Format {
        if self.json {
            Format::Json
        } else if self.raw {
            Format::Raw
        } else {
            self.format.unwrap_or(Format::Plain)
        }
    }
}

#[derive(Subcommand)]
pub enum Commands {
    Read(ReadArgs),
    Search(SearchArgs),
    Today(TodayArgs),
    Random(RandomArgs),
    Echo(EchoArgs),
    Mood(MoodArgs),
    Cache(CacheArgs),
    Ai(AiArgs),
    Tui(TuiArgs),
    /// Compare a passage across translations side by side.
    Parallel(ParallelArgs),
    /// Word-level diff of a passage across translations.
    Diff(DiffArgs),
    /// Follow a reading plan (Bible in a year, NT in 90 days, ...).
    Plan(PlanArgs),
    /// Export a passage to Markdown, Anki, JSON, or plain text.
    Export(ExportArgs),
    /// Curated topical verse collections for study.
    Topic(TopicArgs),
    /// Manage installed translations.
    Translation(TranslationArgs),
    /// Generate a shell completion script (bash, zsh, fish, powershell, elvish).
    Completions(CompletionsArgs),
}

#[derive(Args)]
pub struct ParallelArgs {
    #[arg(required = true)]
    pub reference: Vec<String>,

    /// Comma-separated translation ids to compare (e.g. `kjv,bbe`).
    #[arg(long, value_name = "IDS")]
    pub with: String,
}

#[derive(Args)]
pub struct DiffArgs {
    #[arg(required = true)]
    pub reference: Vec<String>,

    /// Comma-separated translation ids to diff; the first is the base. A single
    /// id is diffed against the active translation (e.g. `--with bbe`).
    #[arg(long, value_name = "IDS")]
    pub with: String,
}

#[derive(Args)]
pub struct PlanArgs {
    #[command(subcommand)]
    pub action: PlanAction,
}

#[derive(Subcommand)]
pub enum PlanAction {
    /// List available reading plans.
    List,
    /// Start a reading plan.
    Start(PlanStartArgs),
    /// Show today's reading portion.
    Today(PlanTodayArgs),
    /// Mark a day's reading as done.
    Done(PlanDoneArgs),
    /// Show progress through the active plan.
    Status,
    /// Stop the active plan and clear its progress.
    Stop,
}

#[derive(Args)]
pub struct PlanStartArgs {
    /// Plan id (see `bible plan list`).
    pub id: String,
}

#[derive(Args)]
pub struct PlanTodayArgs {
    /// Show a specific day instead of the next unread one.
    #[arg(long, value_name = "N")]
    pub day: Option<u32>,

    /// Print chapter references only (e.g. `Matthew 5`), one per line.
    #[arg(long)]
    pub refs_only: bool,
}

#[derive(Args)]
pub struct PlanDoneArgs {
    /// Mark a specific day instead of the next unread one.
    #[arg(long, value_name = "N")]
    pub day: Option<u32>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum ExportTarget {
    /// Markdown with a heading and verses.
    Md,
    /// Anki-importable TSV (reference<TAB>text).
    Anki,
    /// JSON array of verse records.
    Json,
    /// Plain text, one verse per line.
    Txt,
}

#[derive(Args)]
pub struct ExportArgs {
    #[arg(required = true)]
    pub reference: Vec<String>,

    /// Export target format.
    #[arg(long, value_enum, default_value_t = ExportTarget::Md)]
    pub to: ExportTarget,
}

#[derive(Args)]
pub struct TopicArgs {
    pub topic: Option<String>,

    /// List available topics.
    #[arg(long)]
    pub list: bool,

    /// Print only references, not verse text.
    #[arg(long)]
    pub refs_only: bool,
}

#[derive(Args)]
pub struct TranslationArgs {
    #[command(subcommand)]
    pub action: TranslationAction,
}

#[derive(Subcommand)]
pub enum TranslationAction {
    /// List installed translations.
    List,
    /// Download and install a translation.
    Add(TranslationAddArgs),
    /// Set the default translation.
    Default(TranslationDefaultArgs),
    /// Remove an installed translation.
    Remove(TranslationRemoveArgs),
}

#[derive(Args)]
pub struct TranslationAddArgs {
    /// Translation id (e.g. `bbe`). Known ids install without `--source`.
    pub id: String,

    /// Source URL or file path (required for unknown ids).
    #[arg(long)]
    pub source: Option<String>,
}

#[derive(Args)]
pub struct TranslationDefaultArgs {
    /// Translation id to set as default.
    pub id: String,
}

#[derive(Args)]
pub struct TranslationRemoveArgs {
    /// Translation id to remove.
    pub id: String,
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

    /// Treat the query as a regular expression.
    #[arg(long)]
    pub regex: bool,

    /// Match whole words only.
    #[arg(long)]
    pub word: bool,

    /// Print only the number of matches across the whole text.
    #[arg(long)]
    pub count: bool,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum Testament {
    /// Old Testament (Genesis–Malachi).
    Ot,
    /// New Testament (Matthew–Revelation).
    Nt,
}

#[derive(Args)]
pub struct TodayArgs {
    /// Restrict the verse of the day to a single book.
    #[arg(long)]
    pub book: Option<String>,

    /// Restrict to a testament.
    #[arg(long, value_enum)]
    pub testament: Option<Testament>,
}

#[derive(Args)]
pub struct RandomArgs {
    /// How many verses to draw.
    #[arg(short = 'n', long = "count", default_value_t = 1)]
    pub count: usize,

    /// Restrict to a single book.
    #[arg(long)]
    pub book: Option<String>,

    /// Restrict to a testament.
    #[arg(long, value_enum)]
    pub testament: Option<Testament>,

    /// Only verses with at most this many words.
    #[arg(long)]
    pub max_words: Option<usize>,

    /// Seed for reproducible output.
    #[arg(long)]
    pub seed: Option<u64>,
}

#[derive(Args)]
pub struct CompletionsArgs {
    /// Shell to generate completions for.
    #[arg(value_enum)]
    pub shell: Shell,
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

    /// Show installed translations and cache sizes.
    #[arg(long)]
    pub status: bool,
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

    #[arg(
        long,
        help = "Start an interactive chat session with the selected passage"
    )]
    pub chat: bool,
}

#[derive(Args)]
pub struct TuiArgs {
    #[arg(long, help = "Start at a specific book")]
    pub book: Option<String>,

    #[arg(
        long,
        value_name = "REF",
        help = "Start at a specific reference (e.g., 'John 3:16')"
    )]
    pub r#ref: Option<String>,
}
