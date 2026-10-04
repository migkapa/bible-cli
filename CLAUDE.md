# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build and Run Commands

```bash
cargo build                          # Build debug binary
cargo build --release               # Build release binary
cargo run -- <command>              # Run with arguments
cargo test                          # Run all tests
cargo clippy                        # Run linter
cargo fmt                           # Format code
```

Before testing commands, preload the verse cache:
```bash
./target/debug/bible cache --preload
```

## Architecture

This is a Rust CLI application for reading the Bible (KJV by default, with more public-domain translations installable). The binary is named `bible`.

### Core Data Flow

1. **Cache system** (`cache.rs`): Downloads a translation's JSON from a remote source, normalizes it to JSONL, and stores it in `~/.bible-cli/translations/<id>/`. Multi-translation: `CachePaths{root, translation}` computes per-id paths via `verses_path()`/`manifest_path()`/`*_for(id)`. `preload(id, source)` installs any translation (`KNOWN_TRANSLATIONS`/`known_source` cover kjv, bbe, asv, ylt, webster, geneva, nheb); `installed_translations`/`remove_translation` manage them; the default translation persists in `~/.bible-cli/config.json`. Translation ids become directory names, so every id from the user goes through `normalize_translation_id` (lowercase; letters, digits, `-`, `_`), and `remove_translation` re-validates. `dir_for` still finds pre-0.7 uppercase id directories case-insensitively (nothing is renamed), and `remove_translation` only deletes directories containing `verses.jsonl`. All state files are written with `write_atomic` (per-process temp file + rename, writing through symlinks). Network fetches run on a dedicated thread (`http_get`) because `reqwest::blocking` panics inside the tokio runtime. The cache handles multiple JSON input formats (array of verses, nested books/chapters, JSONL).

2. **Verse loading** (`verses.rs`): Reads cached JSONL into `Vec<Verse>` structs with book, chapter, verse number, and text.

3. **Reference parsing** (`reference.rs`): `parse_references` turns input into `Vec<ReferenceQuery>`: verses ("John 3:16", "John 3 16"), ranges ("John 3:16-18"), lists ("John 3:16,18,20"), chapters and chapter ranges ("Psalm 23", "Genesis 1-3" via `chapter_end`), cross-chapter ranges ("John 3:36-4:2"), OSIS ids ("John.3.16"), single-chapter books ("Jude 5" = Jude 1:5), and `;`-separated lists where a bare chapter continues the previous book ("Gen 1:1; 2:4"). `parse_reference` requires exactly one passage. `ReferenceQuery` implements `Display` in the same grammar.

4. **Resolution** (`verses.rs`): `VerseIndex` (O(1) HashMap index) resolves queries (`resolve`, `resolve_all`), widens selections (`with_context`), and produces canonical citation labels for a resolved selection (`label`: "John 3:16-18, 20", "Psalms 23", "John 3:16; Romans 8:28") that parse back to the same selection. Labels compress across a translation's missing verses, so anything stored (bookmarks) uses `reference::format_references` instead: the parsed form of what was typed, independent of versification.

5. **Book normalization** (`books.rs`): Maps names, aliases, and OSIS codes ("gen", "1Cor", "1 co", "Phlm") to canonical names through a lookup table built once (`OnceLock`); a test guards against a spelling mapping to two books.

### Module Responsibilities

- `cli.rs`: Clap-based argument parsing with all subcommands and their args
- `commands.rs`: Command handlers that orchestrate the other modules
- `ai/mod.rs`: OpenAI and Anthropic streaming clients; `AiProvider::default_model` gives each provider its own default model
- `moods.rs`: Predefined verse collections for moods like "peace", "courage", "wisdom"
- `topics.rs`: Curated doctrinal/study verse collections (faith, grace, salvation, ...), same shape as `moods.rs`
- `plans.rs`: Built-in reading plans (`bible-1y`, `nt-90`, `gospels-30`, `psalms-proverbs-31`). Day portions are derived from the cached corpus at runtime (`build_days` chunks chapters evenly), so any installed translation works; progress persists in `~/.bible-cli/plan.json` (`PlanState`)
- `memorize.rs`: `cloze` (levels 0-5, nested hidden sets chosen by a stable per-verse hash) and `score_recall` (word-level scoring for `memorize --quiz`)
- `bookmarks.rs`: `Bookmark` storage in `~/.bible-cli/bookmarks.json`; a corrupt file is an error, never treated as empty (`plans::load_state` follows the same rule for `plan.json`)
- `diff.rs`: Token-level LCS diff shared by `bible diff` and quiz scoring
- `hashing.rs`: Stable hashes (`splitmix64`, `fnv1a`) for deterministic choices (verse of the day, cloze patterns); std hashers are randomly seeded, so never use them for this
- `output/mod.rs`: Terminal color handling with ANSI codes (respects NO_COLOR and TERM=dumb) and the `Format` enum (plain/json/ndjson/tsv/ref/raw). `OutputStyle::emit_verses` is the single render path for verses; `emit_json_records` handles json/ndjson for non-verse records (parallel, diff, books, bookmarks); `is_structured()` suppresses decorative output for machine formats; `interactive` (plain format on a TTY) gates hints that must not pollute pipes
- `tui/`: ratatui reader. Scroll bounds come from `Paragraph::line_count` (ratatui feature `unstable-rendered-line-info`), measured during render; `:` opens a go-to prompt; tests drive `App` and render into `TestBackend`

### AI Integration

The `ai` command supports two providers (OpenAI, Anthropic) with switchable models. Chat mode (`--chat`) maintains conversation history up to 16 messages; `/provider` also switches to that provider's default model. Passages are capped at 300 verses. Required env vars: `OPENAI_API_KEY` or `ANTHROPIC_API_KEY`.

## Key Patterns

- All commands require the active translation to be cached first via `bible cache --preload` or `bible translation add <id>` (`bible cache --status` / `bible translation list` show installed translations; `*` marks the active one)
- Active translation resolves in `main.rs` as `--translation` flag > `config.json` default > `kjv`, then flows through `CachePaths`
- Commands that take passages call `parse_references` + `VerseIndex::resolve_all`; single-passage commands (echo, tui `--ref`) use `parse_reference`
- Color output auto-detects TTY, respects `--color` flag and `NO_COLOR` env var; machine formats (`--json`, `--format ...`, `--raw`) are never colorized
- Output format is a global flag resolved in `Cli::resolved_format()` and passed into `OutputStyle::new`; new commands should honor every `Format` (or bail with a clear message)
- Commands that don't touch the network (all but `ai`, `cache --preload`, `translation add`) restore the default SIGPIPE in `main.rs`, so piping into `head` exits quietly instead of panicking in `println!`
