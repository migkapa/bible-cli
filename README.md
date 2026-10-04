# bible-cli

[![Crates.io](https://img.shields.io/crates/v/bible-cli.svg)](https://crates.io/crates/bible-cli)
[![Downloads](https://img.shields.io/crates/d/bible-cli.svg)](https://crates.io/crates/bible-cli)
[![Homebrew](https://img.shields.io/badge/homebrew-tap-orange)](https://github.com/migkapa/homebrew-tap)

Fast, playful Bible CLI: read, search, compare, and memorize scripture in your
terminal. Built in Rust.

## Quick start

```bash
cargo build
./target/debug/bible cache --preload
./target/debug/bible read John 3 16
./target/debug/bible today
./target/debug/bible mood peace
./target/debug/bible memorize Psalm 23:1-3 --level 4
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

- `bible read <reference>` — a verse, range, list, chapter, chapter range, or passage list (see [References](#references)); a book name alone gives an overview
- `bible search <words> [--book <book>] [--testament ot|nt] [--limit N] [--regex] [--word] [--count]`
- `bible today [--book <book>] [--testament ot|nt]`
- `bible random [-n N] [--book <book>] [--testament ot|nt] [--max-words N] [--seed N]`
- `bible echo <reference> [--window N]` — a verse or range with surrounding context
- `bible mood <mood>` or `bible mood --list`
- `bible topic <name>` or `bible topic --list` (curated study collections; `--refs-only`)
- `bible books [--testament ot|nt]` — the 66 books with chapter and verse counts
- `bible memorize <reference> [--level 0-5] [--quiz] [--seed N]` — memorization practice
- `bible bookmark [add <reference> [--note TEXT] [--tag TAG]] | [list [--tag TAG]] | [remove <N|reference>]`
- `bible parallel <reference> --with kjv,bbe` — compare translations side by side
- `bible diff <reference> --with kjv,bbe` — word-level diff across translations
- `bible plan list|start <id> [--force]|today|done|undo|status|stop` — built-in reading plans
- `bible export <reference> --to md|anki|json|txt`
- `bible translation list|available|add <id> [--source]|default <id>|remove <id>`
- `bible cache [--preload] [--source <url-or-path>] [--status]`
- `bible ai <reference> [--provider openai|anthropic] [--model NAME] [--chat]`
- `bible tui [--book <book>] [--ref <reference>]`
- `bible completions <bash|zsh|fish|powershell|elvish>`

A global `-t/--translation <id>` selects which translation to read from (default:
the configured default, else `kjv`).

Chat commands (with `--chat`): `/help`, `/model <name>`, `/provider <name>`, `/reset`, `/exit`.

## References

Commands that take a passage share one grammar, which covers everything the CLI
itself prints, so a plan heading or a `--json` id can be fed straight back in
(`echo` and `tui --ref` take a single passage rather than a list):

| Form | Examples |
|------|----------|
| Verse | `John 3:16`, `John 3 16`, `jn 3:16`, `Jn3:16` |
| Range | `John 3:16-18`, `John 3:16–18` (en dash) |
| List | `John 3:16,18,20`, `John 3:16-18, 20` |
| Chapter, chapter range | `Psalm 23`, `Matthew 5-7` |
| Across chapters | `John 3:36-4:2` |
| Whole book | `Jude`, `Romans` (an overview in the human view) |
| OSIS ids and codes | `John.3.16`, `1Cor.13.4-7`, `1Cor 13:4`, `Phlm 1:6` |
| One-chapter books | `Jude 5`, `Philemon 6` (verse numbers) |
| Passage lists | `John 3:16; Romans 8:28`, `Genesis 1:1; 2:4` (a bare chapter continues the book) |

```bash
bible read "Gen 1:1-3; John 1:1-3"
bible export "Psalm 23; John 10:11" --to md
bible search love --format ndjson | jq -r .id | head -3 | xargs -n1 bible read
```

## Search

Matches are highlighted, and an interactive terminal tells you when results were
cut off ("Showing 5 of 547 matches"). Quotes are optional for phrases:

```bash
bible search love one another --testament nt
bible search --word lord --count
bible search --regex '^In the beginning' --limit 0     # 0 = no limit
```

## Translations

The CLI is multi-translation. KJV ships as the default; these public-domain
translations install by id alone (`bible translation available` lists them):

| id | Translation |
|----|-------------|
| `kjv` | King James Version (1769) |
| `bbe` | Bible in Basic English (1949/1964) |
| `asv` | American Standard Version (1901) |
| `ylt` | Young's Literal Translation (1898) |
| `webster` | Webster Bible (1833) |
| `geneva` | Geneva Bible (1599) |
| `nheb` | New Heart English Bible (modern English) |

Any other JSON/JSONL source can be installed with `--source`:

```bash
bible translation add nheb             # modern English
bible translation list                 # installed translations (* = active)
bible translation default nheb         # set the default
bible -t asv read John 3:16            # one-off override
bible parallel John 3:16 --with kjv,asv,nheb
bible translation add mine --source ./my-bible.jsonl
```

Translation ids are case-insensitive and may contain letters, digits, `-`, and `_`.

`bible diff` is `git diff` for scripture — a word-level collation of a passage
across translations. Shared words are dimmed; words only in the base are red,
words only in the compared translation are green. With `--json` it emits
per-token `equal`/`insert`/`delete` ops (`--format tsv` gives one row per token):

```bash
bible diff John 3:16 --with kjv,bbe    # first id is the base
bible diff Psalm 23 --with bbe         # single id: base = active translation
bible diff John 3:16 --with kjv,bbe --json | jq '.[0].diffs.bbe'
```

## Memorize

`bible memorize` hides more of a passage at each level, so you can work it into
memory one step at a time:

| Level | Shows |
|-------|-------|
| 0 | the full text |
| 1–3 | a quarter, half, then three quarters of the words blanked |
| 4 | first letters only: `F G s l t w, t h g h o b S, ...` |
| 5 | every word blanked, punctuation kept as a hint |

```bash
bible memorize John 3:16               # level 2 (the default)
bible memorize Psalm 23 --level 4 --raw  # a first-letter card to print
bible memorize John 3:16 --quiz        # recite it; get scored word by word
bible memorize Psalm 23:1-3 --quiz --level 4   # with a first-letter hint
```

Each level hides everything the level below it did, and a verse always hides the
same words (`--seed N` reshuffles them). In a quiz, case and punctuation don't
count against you; missed words are shown in red.

## Bookmarks

Save passages with notes and tags. Bookmarks store the reference, not the text,
so they read in whichever translation is active:

```bash
bible bookmark add John 3:16 --note "memorize this" --tag love --tag gospel
bible bookmark add "Romans 8:28; 1 Corinthians 13:4-7" --tag love
bible bookmark                         # list (same as `bible bookmark list`)
bible bookmark list --tag love
bible -t bbe bookmark list --json      # with verse text in any translation
bible bookmark remove 2                # or: bible bookmark remove John 3:16
```

Bookmarks live in `~/.bible-cli/bookmarks.json`.

## Reading plans

Built-in reading plans turn the CLI into a daily habit. Progress lives in
`~/.bible-cli/plan.json`; portions are derived from the cached corpus, so any
installed translation works:

```bash
bible plan list                        # bible-1y, nt-90, gospels-30, psalms-proverbs-31
bible plan start nt-90                 # start the New Testament in 90 days
bible plan today                       # print today's portion (e.g. Matthew 1-2)
bible plan done                        # check it off: "Day 1/90 done — 1% — 89 days remaining"
bible plan undo                        # marked a day by mistake? unmark it
bible plan status                      # progress bar, pace, start date
bible plan stop                        # clear the active plan
```

`plan today` reads the next unread day, so a missed day is never skipped. It
composes with the rest of the CLI: `bible plan today --refs-only` prints chapter
references (`Matthew 5`), and the global formats work too
(`bible plan today --format ref`, `--json`, `--raw`). Starting a new plan while
one is in progress asks for `--force`, so progress is never lost by accident.

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
bible books --format tsv
```

Ids use OSIS-style book codes (`John.3.16`, `1Cor.13.4`) for stable joins, and
`bible read` accepts them back.

## AI

Use the AI command to get short summaries or reflections for a passage.

Features:
- **Streaming responses** - See responses appear token-by-token in real-time
- **Thinking indicator** - Animated spinner while waiting for the AI
- **Markdown rendering** - Formatted output with headers, lists, and code blocks
- **Clean visuals** - Monochrome theme inspired by modern CLI tools

Example:

```bash
bible ai John 3 16
bible ai "Matthew 5:3-12" --provider anthropic
```

Default models are `gpt-4o-mini` (OpenAI) and `claude-haiku-4-5-20251001`
(Anthropic); pick another with `--model`. Passages are limited to 300 verses.

Chat mode keeps a continuous conversation around the selected passage:

```bash
bible ai John 3 16 --chat
# inside chat:
/model gpt-4o-mini
/provider anthropic                    # also switches to that provider's default model
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
bible tui --ref "John 3:16"            # open at a verse, highlighted
```

```
┌─ Books ─────────┬─ John 3 · KJV ────────────────────────────────┐
│ > Genesis       │                                               │
│   Exodus        │  1  There was a man of the Pharisees, named   │
│   ...           │     Nicodemus, a ruler of the Jews:           │
│ > John          │                                               │
│   Acts          │  16 For God so loved the world, that he gave  │
│   ...           │     his only begotten Son...                  │
├─────────────────┤                                               │
│ Ch 3/21 [n/p]   │                                               │
└─────────────────┴───────────────────────────────────────────────┘
 [READER]  j/k:scroll  n/p:chapter  ::go to  g/G:top/bottom  Tab:books  q:quit
```

**Keybindings:**

| Key | Action |
|-----|--------|
| `Tab` | Switch between Books/Reader mode |
| `j`/`k` | Navigate list or scroll content (the mouse wheel scrolls too) |
| `Enter` | Select book (in Books mode) |
| `n`/`p` | Next/previous chapter |
| `:` | Go to a reference (`John 3:16`, `Ps 23`, or `5` / `3:16` in the current book) |
| `g`/`G` | Go to top/bottom (first/last book in Books mode) |
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
