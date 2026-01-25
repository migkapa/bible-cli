# bible-cli

[![Crates.io](https://img.shields.io/crates/v/bible-cli.svg)](https://crates.io/crates/bible-cli)
[![Downloads](https://img.shields.io/crates/d/bible-cli.svg)](https://crates.io/crates/bible-cli)
[![Homebrew](https://img.shields.io/badge/homebrew-tap-orange)](https://github.com/migkapa/homebrew-tap)

Fast, playful Bible CLI (KJV MVP). Built in Rust.

## Quick start

```bash
cargo build
./target/debug/bible cache --preload
./target/debug/bible read John 3 16
./target/debug/bible today
./target/debug/bible mood peace
./target/debug/bible ai John 3 16 --chat
./target/debug/bible tui
```

## Install

**Homebrew (macOS/Linux)**

```bash
brew tap migkapa/tap
brew install bible-cli
```

**Cargo**

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
- `bible ai <reference> [--chat]`
- `bible tui [--book <book>]`

Chat commands (with `--chat`): `/help`, `/model <name>`, `/provider <name>`, `/reset`, `/exit`.

## AI

Use the AI command to get short summaries or reflections for a specific verse.

Features:
- **Streaming responses** - See responses appear token-by-token in real-time
- **Thinking indicator** - Animated spinner while waiting for the AI
- **Markdown rendering** - Formatted output with headers, lists, and code blocks
- **Clean visuals** - Monochrome theme inspired by modern CLI tools

Example:

```bash
bible ai John 3 16 --provider openai --model gpt-4o-mini
```

Chat mode keeps a continuous conversation around the selected passage:

```bash
bible ai John 3 16 --chat
# inside chat:
/model gpt-4o-mini
/provider anthropic
```

Required environment variables (set at least one for the provider you use):

- `OPENAI_API_KEY`
- `ANTHROPIC_API_KEY`

Notes:

- Pick models based on your desired quality, speed, and cost; faster/smaller models are usually cheaper.
- API usage may incur provider charges; check your provider pricing.
- Requests are sent to the selected provider; avoid sharing sensitive data if you are concerned about privacy.

## Interactive TUI

Launch a full-screen terminal interface for browsing the Bible:

```bash
bible tui
bible tui --book John
```

```
┌─ Books ─────────┬─ John 3 ──────────────────────────────────────┐
│ > Genesis       │                                               │
│   Exodus        │  1  There was a man of the Pharisees, named   │
│   ...           │     Nicodemus, a ruler of the Jews:           │
│ > John          │                                               │
│   Acts          │  16 For God so loved the world, that he gave  │
│   ...           │     his only begotten Son...                  │
├─────────────────┤                                               │
│ Ch 3/21 [n/p]   │                                               │
└─────────────────┴───────────────────────────────────────────────┘
 [READER]  j/k:scroll  n/p:chapter  Tab:books  g/G:top/bottom  q:quit
```

**Keybindings:**

| Key | Action |
|-----|--------|
| `Tab` | Switch between Books/Reader mode |
| `j`/`k` | Navigate list or scroll content |
| `Enter` | Select book (in Books mode) |
| `n`/`p` | Next/previous chapter |
| `g`/`G` | Go to top/bottom |
| `Ctrl-d`/`Ctrl-u` | Page down/up |
| `q` | Quit |

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
