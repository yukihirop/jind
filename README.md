<p align="center">
  <img src="https://raw.githubusercontent.com/yukihirop/jind/main/docs/hero.svg" alt="jind — jev × find. Say what you are looking for, in any order. Get the find you meant. Confirmed before it runs." width="880">
</p>

<p align="center">
  <b>jind</b> turns a loose pile of words — out of order, half-remembered, misspelled — into the <code>find</code> command you meant, shows it, and runs it.
</p>

<p align="center">
  <img src="https://raw.githubusercontent.com/yukihirop/jind/main/docs/demo.svg" alt="Terminal demo: --explain showing how each word was classified ('log' as an extension by jev, '7 days' attached as a time amount) and the find confirmed; 'rs edited within an hour' becoming -mmin -60; a '*.tmp except build delete' resolved offline, listing the three files before asking [y/N/e]" width="930">
</p>

```sh
$ jind log files older than 7 days in /var/log delete

find /var/log -type f -iname '*.log' -mtime +7 -delete

  /var/log/system.log.3
  /var/log/install.log.1
  … 40 more

delete these 42 entries? (interpreted by jev, confidence 0.88) [y/N/e]
```

A bare extension (`log`), a duration in words (`older than 7 days`), a directory anywhere in the line, an action at the end. Any order gives the same `find`.

```sh
$ jind rs edited within an hour              # find . -iname '*.rs' -mmin -60
$ jind png bigger than 5MB in ~/Downloads    # find ~/Downloads -iname '*.png' -size +5M
$ jind empty folders depth 2 count           # find . -maxdepth 2 -type d -empty  → prints the count
$ jind '*.log' src except target ls          # find src -name '*.log' -not -path '*/target/*' … -ls
$ jind files changed today                   # find . -type f -mtime -1
```

Sister project of [jurl](https://github.com/yukihirop/jurl) (jev × curl): same shape, same jev client, same confirm prompt.

## How it works

<p align="center">
  <img src="https://raw.githubusercontent.com/yukihirop/jind/main/docs/flow.svg" alt="words → rules → all resolved? yes: find. no: jev (one request) → repair → confirm [Y/n/e] → find" width="880">
</p>

- **rules** — the unambiguous shapes are decided in code: paths, globs, `files`/`dirs`, `delete`/`ls`/`count`, `+7d`, `>10M`, `days`, `except`, `depth`, `today`. If every word resolves, jind runs offline with no prompt.
- **jev** — anything left over goes to [jev](https://openrouter.ai) (TypeSafe System One, via OpenRouter) in **one request**: "what is the role of each word?" plus, for numbers, "which unit?" and "at least or at most?". jev only picks from fixed choices and returns probabilities; it never generates the command.
- **repair** — fixes what jev cannot see word by word: `7 days` attaches the unit to the number, `except X Y` marks the names after it as excluded, `depth 2` claims the neighbouring number.
- **confirm** — whenever jev was involved, jind shows the command before running. `e` opens it in `$EDITOR` (with your original words and the per-word table as comments). Below a confidence floor it refuses to run as is.
- **delete** always asks, always defaults to *No*, and always after listing what would go — the same expression is run without `-delete` first. `-y` does not skip this.

One jev call is 200–700 ms and under $0.0001.

## Setup

```sh
cargo install jind
jind setup        # store your OpenRouter API key in ~/.config/jind/config.toml (0600)
jind demo         # 10 examples on a sample tree in a temp dir, pick with ↑↓ (delete is safe there)
```

`OPENROUTER_API_KEY` in the environment takes precedence; a key saved by `jurl setup` is picked up too. `find` must be on `PATH` (BSD and GNU both work). Tested on macOS; Linux should behave the same. Windows is untested.

## Grammar

| you write | it means |
|---|---|
| `.` `src` `/var/log` `~/Downloads` | where to search (existing directories count; default `.`) |
| `*.log` `report-*` | `-name` glob, as written |
| `log` `rs` `png` | extension → `-iname '*.log'` |
| `report` | word in the name → `-iname '*report*'` |
| `files` `dirs` `folders` `symlinks` | `-type f/d/l` |
| `empty` | `-empty` |
| `older than 7 days` `within 2 hours` `a week` `today` `+7d` `-2h` | `-mtime` / `-mmin`, direction from the words (jev) or the sign |
| `over 10MB` `smaller than 100k` `>1.5GB` | `-size` |
| `depth 2` `2 levels` | `-maxdepth 2` |
| `except node_modules .git` | `-not -path '*/node_modules/*' …` |
| `delete` `ls` `count` `print0` | what to do with matches (default: print) |
| `-perm 644` and anything after `--` | passed to find untouched |
| anything else | jev decides |

| flag | |
|---|---|
| `-n` | print the command and exit |
| `--explain` | per-word role, confidence, and whether a rule or jev decided it |
| `-y` | skip the confirmation (never for `delete`) |
| `--no-jev` | offline only; unresolved words are an error |

## Config (optional)

`~/.config/jind/config.toml`

```toml
[jev]
confirm = "jev"          # "jev" | "confidence" | "never"
confirm_below = 0.8
reject_below = 0.5

[defaults]
find_args = ["-not", "-path", "*/.git/*"]

[aliases]
dl = "~/Downloads"
```

## Where it is weak

- A bare word can be an extension (`*.log`) or a word in the name (`*log*`); jev is usually 0.6–0.9 sure, so you will see the confirm prompt. Write `*.log` to skip it.
- Only modification time (`-mtime`/`-mmin`). No `-newer file`, no exact-day matches (`yesterday`).
- `-size` is integer only, so `1.5GB` becomes `+1536M`.

---

<p align="center"><sub>Each module in <code>src/</code> starts with a comment on what it does. The jev wire format follows eg-jev's <code>packages/recipes/src/lib/{openrouter,questions}.ts</code>. <code>docs/demo.svg</code> is real output captured through a pty and rendered by <code>docs/make-demo.py</code>.</sub></p>
