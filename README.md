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

- `bible read <reference>` — single verse, range (`John 3:16-18`), list (`John 3:16,18,20`), whole chapter (`Psalm 23`), or book overview
- `bible search <query> [--book <book>] [--limit N] [--regex] [--word] [--count]`
- `bible today [--book <book>] [--testament ot|nt]`
- `bible random [-n N] [--book <book>] [--testament ot|nt] [--max-words N] [--seed N]`
- `bible echo <book> <chapter> <verse> [--window N]`
- `bible mood <mood>` or `bible mood --list`
- `bible topic <name>` or `bible topic --list` (curated study collections; `--refs-only`)
- `bible parallel <reference> --with kjv,bbe` — compare translations side by side
- `bible diff <reference> --with kjv,bbe` — word-level diff across translations
- `bible plan list|start <id>|today|done|status|stop` — built-in reading plans
- `bible export <reference> --to md|anki|json|txt`
- `bible translation list|add <id> [--source]|default <id>|remove <id>`
- `bible cache [--preload] [--source <url-or-path>] [--status]`
- `bible ai <reference> [--chat]`
- `bible tui [--book <book>]`
- `bible completions <bash|zsh|fish|powershell|elvish>`

A global `-t/--translation <id>` selects which translation to read from (default:
the configured default, else `kjv`).

Chat commands (with `--chat`): `/help`, `/model <name>`, `/provider <name>`, `/reset`, `/exit`.

## Translations

The CLI is multi-translation. KJV ships as the default; install more from any
JSON/JSONL source (known public-domain ids like `bbe` need no `--source`):

```bash
bible translation add bbe              # Bible in Basic English
bible translation list                 # installed translations (* = active)
bible translation default bbe          # set the default
bible -t bbe read John 3:16            # one-off override
bible parallel John 3:16 --with kjv,bbe
```

`bible diff` is `git diff` for scripture — a word-level collation of a passage
across translations. Shared words are dimmed; words only in the base are red,
words only in the compared translation are green. With `--json` it emits
per-token `equal`/`insert`/`delete` ops:

```bash
bible diff John 3:16 --with kjv,bbe    # first id is the base
bible diff Psalm 23 --with bbe         # single id: base = active translation
bible diff John 3:16 --with kjv,bbe --json | jq '.[0].diffs.bbe'
```

## Reading plans

Built-in reading plans turn the CLI into a daily habit. Progress lives in
`~/.bible-cli/plan.json`; portions are derived from the cached corpus, so any
installed translation works:

```bash
bible plan list                        # bible-1y, nt-90, gospels-30, psalms-proverbs-31
bible plan start nt-90                 # start the New Testament in 90 days
bible plan today                       # print today's portion (e.g. Matthew 1-2)
bible plan done                        # check it off: "Day 1/90 done — 1% — 89 days remaining"
bible plan status                      # progress bar, pace, start date
bible plan stop                        # clear the active plan
```

`plan today` reads the next unread day, so a missed day is never skipped. It
composes with the rest of the CLI: `bible plan today --refs-only` prints chapter
references (`Matthew 5`), and the global formats work too
(`bible plan today --format ref`, `--json`, `--raw`).

## Output formats

Every verse-producing command accepts a global output format, turning the CLI
into a scriptable data source:

- `--json` — a JSON array of verse records (`id`, `reference`, `book`, `chapter`, `verse`, `text`)
- `--format ndjson` — one JSON object per line
- `--format tsv` — `id`, `book`, `chapter`, `verse`, `text` (tab-separated)
- `--format ref` — references only (`John 3:16`)
- `--raw` — verse text only, no reference or color

```bash
bible read John 3:16 --json
bible search love --limit 50 --format ndjson | jq -r .reference
bible random --seed 42 --book Proverbs --raw | pbcopy
```

Ids use OSIS-style book codes (`John.3.16`, `1Cor.13.4`) for stable joins.

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
https://raw.githubusercontent.com/scrollmapper/bible_databases/master/formats/json/KJV.json
```

You can pass a local path or your own JSONL via `--source`.

> Upgrading from v0.5 or earlier? Run `bible cache --preload` (and
> `bible translation add bbe` if installed) to refresh from the corrected
> source — the previous one was missing Matthew 2:16 and misnumbered the
> rest of that chapter.

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
