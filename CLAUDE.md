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

This is a Rust CLI application for reading the King James Version (KJV) Bible. The binary is named `bible`.

### Core Data Flow

1. **Cache system** (`cache.rs`): Downloads KJV JSON from a remote source, normalizes it to JSONL format, and stores in `~/.bible-cli/translations/kjv/`. The cache handles multiple JSON input formats (array of verses, nested books/chapters structure, JSONL).

2. **Verse loading** (`verses.rs`): Reads cached JSONL into `Vec<Verse>` structs with book, chapter, verse number, and text.

3. **Reference parsing** (`reference.rs`): Parses user input like "John 3 16", "John 3:16", "John 3:16-18" (ranges), or "John 3:16,18,20" (lists) into a `ReferenceQuery` with book, optional chapter, verse, verse_end, and verse_list. `verses::VerseIndex::resolve` turns a query into the matching `Vec<&Verse>` using an O(1) HashMap index.

4. **Book normalization** (`books.rs`): Maps book names and aliases (e.g., "gen", "ge", "gn" all map to "Genesis") to canonical names.

### Module Responsibilities

- `cli.rs`: Clap-based argument parsing with all subcommands and their args
- `commands.rs`: Command handlers that orchestrate the other modules
- `ai/mod.rs`: OpenAI and Anthropic API clients with a `ProviderClient` trait
- `moods.rs`: Predefined verse collections for moods like "peace", "courage", "wisdom"
- `output/mod.rs`: Terminal color handling with ANSI codes (respects NO_COLOR and TERM=dumb) and the `Format` enum (plain/json/ndjson/tsv/ref/raw). `OutputStyle::emit_verses` is the single render path every command uses; `is_structured()` suppresses decorative output for machine formats.

### AI Integration

The `ai` command supports two providers (OpenAI, Anthropic) with switchable models. Chat mode (`--chat`) maintains conversation history up to 16 messages. Required env vars: `OPENAI_API_KEY` or `ANTHROPIC_API_KEY`.

## Key Patterns

- All commands require cache to be preloaded first via `bible cache --preload` (`bible cache --status` lists installed translations)
- Verse references are flexible: "John 3 16", "John 3:16", "jn 3 16", ranges "John 3:16-18", and lists "John 3:16,18,20" all work
- Color output auto-detects TTY, respects `--color` flag and `NO_COLOR` env var; machine formats (`--json`, `--format ...`, `--raw`) are never colorized
- Output format is a global flag resolved in `Cli::resolved_format()` and passed into `OutputStyle::new`
