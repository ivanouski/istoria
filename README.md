# istoria

A fuzzy finder for bash history. Pick a command to put it on the prompt line, delete it from the
history, or find it in backups of the history.

## Install

```bash
cargo install --path .
```

Copy `home/.istoriarc` to your home and load it from `~/.bashrc`, it needs bash 4.0 or later:

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

Up opens istoria with the typed text as the query, Down does the same in the [backups](#backups).
Each word matches anywhere in a command, in any order, ignoring case: `push git` finds
`git -C api push`. Ctrl-P and Ctrl-N still step through the shell's history.

| Key                           | Action                                   |
| ----------------------------- | ---------------------------------------- |
| Up, Down                      | move the selection (also Ctrl-P, Ctrl-N) |
| PgUp, PgDn                    | move the selection by a page             |
| Enter                         | run the selected command                 |
| Tab                           | put the selected command on the line     |
| Ctrl-D                        | delete the selected command from history |
| Left, Right, Home, End        | move the cursor (also Ctrl-A, Ctrl-E)    |
| Alt-B, Alt-F, Alt/Ctrl-Arrows | move the cursor by word                  |
| Backspace, Delete             | delete a character                       |
| Alt-Backspace                 | delete a word                            |
| Ctrl-W                        | delete back to whitespace                |
| Ctrl-U                        | delete to the start of the query         |
| Esc                           | quit, keeping the query                  |
| Ctrl-C                        | quit, leaving the prompt line as it was  |

With bash 4.4 or later istoria shows your prompt in place of `›`, unless it takes more than 60% of
the width. That runs any `$(...)` in `PS1` each time istoria opens, so `ISTORIA_SHELL_PROMPT=false`
keeps `›`. `ISTORIA_PROMPT`, written like `PS1`, shows in place of either:

```bash
ISTORIA_PROMPT='\[\e[35m\]hist›\[\e[0m\] '
```

`istoria delete-last [N]` deletes the last N commands from history, 1 by default, and prints them.
`istoria --help` lists every command, option and exit status.

## Backups

`istoria backup` saves the history to `~/.local/share/istoria/history.db` (or under
`$XDG_DATA_HOME`), where it outlives deleting and `HISTFILESIZE`. Each entry, a command and when it
ran, is stored once, and each backup keeps which ones it saw. A backup's label is when it started,
in local time:

```console
$ istoria backup
20261030-142501  4512 entries, 37 new
$ istoria backup list
20261029-090000	mac	4480
20261030-142501	mac	4512
$ istoria backup delete 20261029-090000
```

`istoria search --backup` searches every backup, `--backup-id LABEL` one of them, with `backup›` or
`backup:LABEL›` as the prompt. Down opens `--backup` the way Up opens the history, so Enter
runs the command and Tab puts it on the line. Typed as a command, istoria only prints it. Ctrl-D
deletes nothing. Commands the shell hasn't saved yet aren't backed up, see
[Bash history](#bash-history).

## Config

`istoria init-config` writes the defaults to `~/.config/istoria/config.toml` (or under
`$XDG_CONFIG_HOME`). Keys left out keep their defaults:

```toml
height  = 12      # rows of the pane, the terminal's at most
list    = "below" # or "above" the prompt line, like fzf
border  = false   # a line on the far edge of the pane
preview = false   # the selected command in full when its row is cut
[colors]
prompt   = { fg = "blue", bold = true }
backup   = { fg = "magenta", bold = true }  # the prompt searching backups
count    = "dark-gray"     # matches/commands
deleted  = "green"         # the status after Ctrl-D
error    = "red"           # Ctrl-D failed, the count of no matches
match    = { fg = "yellow", bold = true }
dim      = "dark-gray"     # times, run counts, cuts, dividers, border
selected = { bg = "238" }  # the selected row, under the match
```

`height` counts the prompt line and the divider, so it is at least 3. With `list = "above"` the
most recent commands are right over the prompt line, and Up keeps going back in history. `border`
and `preview` take rows from the list, and are left out when it would get too short. The preview
shows up to 3 lines.

A color is a name (`yellow`, `light-blue`), `0` to `255`, `#rrggbb`, or a table of `fg`, `bg`,
`bold`, `italic`, `underlined` and `reversed`.

## Development

`bundle install`, then `bundle exec rake` runs rubocop, `cargo fmt --check`, `cargo test`, clippy
and the Ruby tests, which drive istoria in a pseudo terminal.
