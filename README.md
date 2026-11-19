# istoria

A finder for bash history that matches words in any order and forgives typos. Pick a command to
run it or put it on the prompt line, delete it from the history, or find it in backups that outlive
the history.

<img src="assets/demo.gif" width="900" alt="istoria finding git push from gti psu, sbx run claude from sbz r cla and docker compose up -d from dokcer comp">

## Install

With [Homebrew](https://brew.sh), on macOS or Linux, which installs the completion too:

```bash
brew install ivanouski/tap/istoria
```

Then load its `.istoriarc` from `~/.bashrc`, after the `brew shellenv` line, it needs bash 4.0 or
later (macOS ships 3.2, see `brew install bash`). It's upgraded along with istoria:

```bash
[[ -f $HOMEBREW_PREFIX/share/istoria/istoriarc ]] && . "$HOMEBREW_PREFIX/share/istoria/istoriarc"
```

Or download the binary for your platform from the
[latest release](https://github.com/ivanouski/istoria/releases/latest) into `~/.local/bin`, or
another directory in `PATH`:

```bash
asset=istoria-macos   # or istoria-linux-x86_64, istoria-linux-arm64
curl -fL --create-dirs -o ~/.local/bin/istoria "https://github.com/ivanouski/istoria/releases/latest/download/$asset" && chmod +x ~/.local/bin/istoria
```

Or build and install it into `~/.cargo/bin` from a clone:

```bash
cargo install --path .
```

Copy `home/.istoriarc` to your home and load it from `~/.bashrc`, after the line that puts
`istoria` in `PATH`, it needs bash 4.0 or later:

```bash
[[ -f ~/.istoriarc ]] && . ~/.istoriarc
```

Optionally, install the completion, which bash-completion loads on demand (without it, source the
file in `~/.bashrc`):

```bash
cp completions/_istoria ~/.local/share/bash-completion/completions/istoria
```

## Bash history

istoria reads and edits `$HISTFILE`, which bash writes only on exit. Deleting needs each command
saved at the prompt, and the file reloaded to drop deleted commands from the shell's history:

```bash
PROMPT_COMMAND="history -a; history -c; history -r${PROMPT_COMMAND:+; $PROMPT_COMMAND}"
```

Optionally:

```bash
shopt -s histappend      # append on exit instead of overwriting what other shells saved
HISTTIMEFORMAT='%F %T '  # save when each command ran, istoria shows it
HISTSIZE=10000           # more than the 500 commands bash keeps by default
HISTFILESIZE=10000
```

## Usage

Up or Down opens istoria with the typed text as the query. Alt-Up or Alt-Down (Option on a Mac)
opens it in the [backups](#backups), most recent first. Ctrl-P and Ctrl-N still step through the
shell's history.

Each word matches anywhere in a command, in any order, ignoring case: `push git` finds
`git -C api push`. With `search = "prefix"` in the [config](#config) or `--search prefix`, the query
matches the start of a command instead. A space after the query ends a word there, so `ls ` finds
`ls` and `ls -la` but not `lsof`.

A word of 3 or more characters also matches with one typo, listed after the exact matches: `gti`
finds `git push` and `dokcer comp` finds `docker compose up`. `typos = false` in the config or
`--no-typos` turns this off.

Once a word is typed, the list starts with the commands run the most lately, each after a bar of how
much. Without a query it starts with the most recent command, so Up and Enter on an empty line runs
the last command again. `layout = "recent"` or `--layout recent` always puts the most recent first.

```
mon› ~/src/istoria› main› sbx
────────────────────────────────────────────────────────────────────── 25/378 ──
           ▪▪▪▪▪▪▪▪▪▪▪▪▪  sbx run claude                        2026-11-14 10:04
                 ▪▪▪▪▪▪▪  sbx logs commands -p -n 100           2026-11-12 15:39
                  ▪▪▪▪▪▪  sbx run claude -v ../homebrew-tap/ …  2026-11-14 10:42
```

| Key                           | Action                                   |
| ----------------------------- | ---------------------------------------- |
| Up, Down                      | move the selection (also Ctrl-P, Ctrl-N) |
| PgUp, PgDn                    | move the selection by a page             |
| Enter                         | run the selected command                 |
| Tab                           | put the selected command on the line     |
| Ctrl-D                        | delete the selected command              |
| Left, Right, Home, End        | move the cursor (also Ctrl-A, Ctrl-E)    |
| Alt-B, Alt-F, Alt/Ctrl-Arrows | move the cursor by word                  |
| Backspace, Delete             | delete a character                       |
| Alt-Backspace                 | delete a word                            |
| Ctrl-W                        | delete back to whitespace                |
| Ctrl-U                        | delete to the start of the query         |
| Esc                           | quit, keeping the query                  |
| Ctrl-C                        | quit, leaving the prompt line as it was  |

Pasted text goes in the query on one line, so a paste doesn't run a command, given bracketed paste
in readline (on by default from bash 5.1).

With bash 4.4 or later istoria shows your prompt in place of `›`, running any `$(...)` in `PS1`.
`ISTORIA_SHELL_PROMPT=false` keeps `›`, and `ISTORIA_PROMPT`, written like `PS1`, replaces either.

Run directly, `istoria search` prints the picked command instead of running it.
`istoria delete-last [N]` deletes the last N commands from history, 1 by default, and prints them.
`istoria --help` lists every command, option and exit status.

## Backups

`istoria backup` saves the history to `~/.local/share/istoria/history.db` (or under
`$XDG_DATA_HOME`), where it outlives deleting and `HISTFILESIZE`. Each entry, a command and when it
ran, is stored once, and each backup keeps which ones it saw:

```console
$ istoria backup
20261030-142501  4512 entries, 37 new
$ istoria backup list
20261029-090000	mac	4480
20261030-142501	mac	4512
$ istoria backup delete 20261029-090000
```

`--host NAME` saves a backup from another host, like the history of another laptop copied over:

```console
$ HISTFILE=~/air_history istoria backup --host air
20261030-150012  3120 entries, 912 new
```

`istoria search --backup` searches every backup, `--backup-id LABEL` one of them, showing the hosts
each command was backed up from. Commands the shell hasn't saved yet aren't backed up, see
[Bash history](#bash-history).

Ctrl-D in backups asks first, and `y` deletes the command from every backup, or only from `LABEL`
with `--backup-id`. Ctrl-D in history leaves backups as they are.

## Config

`istoria init-config` writes the defaults to `~/.config/istoria/config.toml` (or under
`$XDG_CONFIG_HOME`). Keys left out keep their defaults:

```toml
height  = 12        # rows of the pane, the terminal's at most
list    = "below"   # or "above" the prompt line, like fzf
layout  = "weights" # the most run lately first, or "recent": the latest first
bar     = "▪"       # a step of the weight bars, one column: "❯", "█", "•", "|", "#"
border  = false     # a line on the far edge of the pane
head    = 10        # columns kept of a long command's start, 0 for none
gap     = "…"       # between that start and the rest, one column: "⋯", "┆", "»", "✂"
preview = false     # the selected command in full when its row is cut
times   = true      # when each command ran, if the history has times
search  = "keyword" # words anywhere, or "prefix": the start of commands
typos   = true      # words of 3+ characters also match with one typo, listed after
[colors]
prompt   = { fg = "blue", bold = true }
backup   = "yellow"              # the prompt searching backups
count    = "dark-gray"           # matches/commands
deleted  = "green"               # the status after Ctrl-D
error    = "red"                 # Ctrl-D asking or failed, the count of no matches
match    = { fg = "yellow", bold = true }
dim      = "dark-gray"           # times, bars, run counts, hosts, cuts, dividers, border
selected = { bg = "dark-gray" }  # the selected row, under the match
```

A color is a name (`yellow`, `light-blue`), `0` to `255`, `#rrggbb`, or a table of `fg`, `bg`,
`bold`, `italic`, `underlined` and `reversed`.

## Development

`bundle install`, then `bundle exec rake` runs rubocop, `cargo fmt --check`, `cargo test`, clippy
and the Ruby tests, which drive istoria in a pseudo terminal.
