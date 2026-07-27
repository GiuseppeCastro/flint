# flint

Fast, local-first autocomplete and history search for Zsh, powered by a small
Rust ranking engine that learns from what you actually run, where, and in
which project.

```
$ docker co
$ docker compose up -d
       ^^^^^^^^^^^^^^^ ghost suggestion, ranked by your history + cwd/git context
```

## Install

```
brew install GiuseppeCastro/flint/flint
```

(Building from source: `cargo install --path .` with a recent stable Rust.)

## Setup

Add this as the **last line** of `~/.zshrc` (after any prompt/plugin manager,
so exit-code and syntax-highlighting tracking stay accurate), then restart
your shell:

```
eval "$(flint init zsh)"
```

On first launch flint imports your existing `$HISTFILE` and keeps learning
from every command you run afterward — no further setup needed.

## Usage

| Key                  | Action                                    |
|-----------------------|--------------------------------------------|
| `→` / `End`           | Accept the full ghost suggestion           |
| `Option+→` / `Alt+f`  | Accept just the next word of it            |
| `Ctrl+R`              | Ranked, fuzzy history search               |
| `Tab`                 | Unchanged — normal Zsh completion          |

Ranking blends prefix/fuzzy match quality, frequency, recency, current
directory and git repo/branch, learned command-sequence transitions (e.g.
`git add` → `git commit`), success rate, and past suggestion acceptance —
all computed locally in a few milliseconds.

`flint status` shows a quick summary; `flint doctor` diagnoses integration or
performance issues; `flint stats` lists your most-used commands.

## Privacy

Everything is local: one SQLite database in `~/.flint`, no network calls, no
telemetry, no account. Commands that look like they carry a secret (AWS/GitHub
tokens, bearer tokens, passwords, credentials in URLs, etc.) are detected and
never stored. Add your own patterns under `[privacy] ignore_patterns` in
`~/.flint/config.toml` if needed.

## Uninstall

```
flint uninstall          # prints the line to remove from ~/.zshrc
flint uninstall --purge-data --yes   # also deletes ~/.flint
brew uninstall flint
```
