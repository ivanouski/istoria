# hdf

A fuzzy finder for bash history. Pick a command to put it on the prompt line, or delete it from
the history, every occurrence of it.

## Install

Build and install the binary into `~/.cargo/bin`:

```bash
cargo install --path .
```

Copy `home/.hdfrc` to your home and load the key bindings from `~/.bashrc`, they need bash 4.0 or
later:

```bash
[[ -f ~/.hdfrc ]] && . ~/.hdfrc
```

Optionally, install the completion of the options, which bash-completion loads on demand (without
it, source the file in `~/.bashrc`):

```bash
cp completions/_hdf ~/.local/share/bash-completion/completions/hdf
```

## Bash history

hdf reads and edits the history file, `$HISTFILE`. Deleting needs two settings in `~/.bashrc`:

```bash
# Save each command at the prompt and reload the file
PROMPT_COMMAND="history -a; history -c; history -r${PROMPT_COMMAND:+; $PROMPT_COMMAND}"
# Keep hdf --delete-last out of the history, add a pattern for an alias of it too
HISTIGNORE="${HISTIGNORE:+$HISTIGNORE:}hdf --delete-last*"
```

Bash saves commands to the file only on exit. `history -a` at each prompt saves them as they run, so
`--delete-last` deletes the right ones, and `HISTIGNORE` keeps it from deleting itself next time.
Reloading with `history -c; history -r` drops deleted commands from the shell's copy, which Ctrl-P,
Ctrl-N, `!!` and `history` use.

Optionally, for the rest of hdf:

```bash
# Append to the file on exit instead of overwriting what other shells saved
shopt -s histappend
# Save when each command ran, hdf shows it. Commands saved before have no time
HISTTIMEFORMAT='%F %T '
# Keep more than the 500 commands bash keeps by default
HISTSIZE=10000
HISTFILESIZE=10000
```

## Usage

Up or Down opens hdf with the typed text as the query. Enter runs the chosen command, Tab puts it
on the prompt line to edit, Esc puts back the edited query. Ctrl-P and Ctrl-N still step through
the shell's history.

Each word of the query matches anywhere in a command, in any order, ignoring case: `push git`
finds `git -C api push`. With bash 4.4 or later hdf shows your prompt in place of `›`, unless it
takes more than 60% of the width. Bash expands the prompt each time hdf opens, running any `$(...)`
in `PS1` again, which a git status in a big repo makes slow. `HDF_SHELL_PROMPT=false` in
`~/.bashrc` keeps `›`.

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

`hdf --delete-last [N]` deletes the last N commands from history, 1 by default, and prints them.
It needs the history saved at each prompt, see [Bash history](#bash-history).

`hdf --help` lists all options and exit statuses.

## Config

`hdf --init-config` writes the defaults to `~/.config/hdf/config.toml` (or under
`$XDG_CONFIG_HOME`). Keys left out keep their defaults:

```toml
height  = 12      # rows of the pane, the terminal's at most
list    = "below" # or "above" the prompt line, like fzf
border  = false   # a line on the far edge of the pane
preview = false   # the selected command in full when its row is cut
[colors]
prompt   = { fg = "blue", bold = true }
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
and the Ruby tests, which drive hdf in a pseudo terminal.
