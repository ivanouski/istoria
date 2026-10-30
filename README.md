# istoria

A fuzzy finder for bash history. Pick a command to put it on the prompt line, or delete it from
the history, every occurrence of it.

## Install

Build and install the binary into `~/.cargo/bin`:

```bash
cargo install --path .
```

Copy `home/.istoriarc` to your home and load the key bindings from `~/.bashrc`, they need bash 4.0
or later:

```bash
[[ -f ~/.istoriarc ]] && . ~/.istoriarc
```

Optionally, install the completion of the commands, options and backup labels, which
bash-completion loads on demand (without it, source the file in `~/.bashrc`):

```bash
cp completions/_istoria ~/.local/share/bash-completion/completions/istoria
```

## Bash history

istoria reads and edits the history file, `$HISTFILE`. Deleting needs this setting in `~/.bashrc`:

```bash
# Save each command at the prompt and reload the file
PROMPT_COMMAND="history -a; history -c; history -r${PROMPT_COMMAND:+; $PROMPT_COMMAND}"
```

Bash saves commands to the file only on exit. `history -a` at each prompt saves them as they run, so
`delete-last` deletes the right ones. Reloading with `history -c; history -r` drops deleted commands from the shell's copy, which Ctrl-P,
Ctrl-N, `!!` and `history` use.

Optionally, for the rest of istoria:

```bash
# Append to the file on exit instead of overwriting what other shells saved
shopt -s histappend
# Save when each command ran, istoria shows it. Commands saved before have no time
HISTTIMEFORMAT='%F %T '
# Keep more than the 500 commands bash keeps by default
HISTSIZE=10000
HISTFILESIZE=10000
```

## Usage

Up or Down opens istoria with the typed text as the query. Enter runs the chosen command, Tab puts
it on the prompt line to edit, Esc puts back the edited query. Ctrl-P and Ctrl-N still step through
the shell's history.

Each word of the query matches anywhere in a command, in any order, ignoring case: `push git`
finds `git -C api push`. With bash 4.4 or later istoria shows your prompt in place of `›`, unless
it takes more than 60% of the width, which hides the times too to leave room for the commands. Bash
expands the prompt each time istoria opens, running any `$(...)` in `PS1` again, which a git status
in a big repo makes slow. `ISTORIA_SHELL_PROMPT=false` in `~/.bashrc` keeps `›`. `ISTORIA_PROMPT`,
written like `PS1`, shows in place of either:

```bash
ISTORIA_PROMPT='\[\e[35m\]mon›\[\e[0m\] '
```

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

`istoria delete-last [N]` deletes the last N commands from history, 1 by default, and prints them.
It needs the history saved at each prompt, see [Bash history](#bash-history).

`istoria --help` lists all commands, options and exit statuses.

## Backups

`istoria backup` saves the commands of `$HISTFILE` in a backup, kept in
`~/.local/share/istoria/history.db` (or under `$XDG_DATA_HOME`). Each entry, a command and when it
ran, is stored once, and each backup keeps which ones it saw, so an entry later deleted from history
or dropped past `HISTFILESIZE` stays in the backups. It prints the backup's label, when it started
in local time, and how many entries it saw and added:

```console
$ istoria backup
20261030-142501  4512 entries, 37 new
$ istoria backup list
20261029-090000	mac	4480
20261030-142501	mac	4512
$ istoria backup delete 20261029-090000
```

Commands the shell hasn't saved yet are left out, so run `history -a` first, or save at each prompt,
see [Bash history](#bash-history).

`istoria search --backup` searches every backup instead of the history, `--backup-id LABEL` one
backup. The prompt says which, `backup›` or `backup:20261030-142501›`. Enter prints the chosen
command, and Ctrl-D deletes nothing from backups, nor from history. Deleting from history leaves the
backups as they are.

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

`height` counts the prompt line and the divider, so it is at least 3.

With `list = "above"` the prompt line is the bottom row and the most recent commands are right over
it. Up, Ctrl-P and PgUp move to older commands, so Up keeps going back in history.

`border` and `preview` take rows from the list and are left out when it would have none or fewer
than 3 rows. The preview shows up to 3 lines.

A color is a name (`yellow`, `light-blue`, `dark-gray`), `0` to `255` or `#rrggbb`, or a table of
`fg`, `bg`, `bold`, `italic`, `underlined` and `reversed`.

## Development

`bundle install`, then `bundle exec rake` runs rubocop, `cargo fmt --check`, `cargo test`, clippy
and the Ruby tests, which drive istoria in a pseudo terminal.
