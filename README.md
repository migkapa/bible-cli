# bible-cli

Fast, playful Bible CLI (KJV MVP). Built in Rust.

## Quick start

```bash
cargo build
./target/debug/bible cache --preload
./target/debug/bible read John 3 16
./target/debug/bible today
./target/debug/bible mood peace
```

## Install

```bash
cargo install bible-cli
```

## Commands

- `bible read <book> [chapter] [verse]`
- `bible search <query> [--book <book>] [--limit N]`
- `bible today`
- `bible random`
- `bible echo <book> <chapter> <verse> [--window N]`
- `bible mood <mood>` or `bible mood --list`
- `bible cache [--preload] [--source <url-or-path>]`

## AI

Use the AI command to get short summaries or reflections for a specific verse.

Example:

```bash
bible ai John 3 16 --provider openai --model gpt-4o-mini
```

Required environment variables (set at least one for the provider you use):

- `OPENAI_API_KEY`
- `ANTHROPIC_API_KEY`

Notes:

- Pick models based on your desired quality, speed, and cost; faster/smaller models are usually cheaper.
- API usage may incur provider charges; check your provider pricing.
- Requests are sent to the selected provider; avoid sharing sensitive data if you are concerned about privacy.

## Cache

Defaults to `~/.bible-cli`. Override with `--data-dir <dir>`.

The default KJV source URL is:

```
https://raw.githubusercontent.com/thiagobodruk/bible/master/json/en_kjv.json
```

You can pass a local path or your own JSONL via `--source`.

## Color output

By default, colors are enabled only when stdout is a TTY. You can override with:

- `--color auto` (default)
- `--color always`
- `--color never`

## Data format

Cached verses are stored as JSONL:

```json
{"book":"Genesis","chapter":1,"verse":1,"text":"In the beginning God created the heaven and the earth."}
```
