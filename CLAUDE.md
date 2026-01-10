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

3. **Reference parsing** (`reference.rs`): Parses user input like "John 3 16" or "John 3:16" into a `ReferenceQuery` with book, optional chapter, and optional verse.

4. **Book normalization** (`books.rs`): Maps book names and aliases (e.g., "gen", "ge", "gn" all map to "Genesis") to canonical names.

### Module Responsibilities

- `cli.rs`: Clap-based argument parsing with all subcommands and their args
- `commands.rs`: Command handlers that orchestrate the other modules
- `ai/mod.rs`: OpenAI and Anthropic API clients with a `ProviderClient` trait
- `moods.rs`: Predefined verse collections for moods like "peace", "courage", "wisdom"
- `output.rs`: Terminal color handling with ANSI codes, respects NO_COLOR and TERM=dumb

### AI Integration

The `ai` command supports two providers (OpenAI, Anthropic) with switchable models. Chat mode (`--chat`) maintains conversation history up to 16 messages. Required env vars: `OPENAI_API_KEY` or `ANTHROPIC_API_KEY`.

## Key Patterns

- All commands require cache to be preloaded first via `bible cache --preload`
- Verse references are flexible: "John 3 16", "John 3:16", "jn 3 16" all work
- Color output auto-detects TTY, respects `--color` flag and `NO_COLOR` env var
