//! The command line arguments and the help text

use std::ffi::OsString;

use crate::{backup, config};

const USAGE: &str = "\
Usage: istoria COMMAND [ARGS]

Commands:
  search [OPTIONS] [QUERY]  search bash history and print the chosen command
  delete-last [N]           delete the last N commands from history, 1 by default
  init-config               write the default config, unless it exists
  backup                    save the commands of $HISTFILE in a backup
  backup list               list the backups, oldest first
  backup delete LABEL       delete a backup

Options:
  -h, --help     print this help
  -V, --version  print the version

search reads $HISTFILE. Each word of the query matches anywhere in a command, in
any order, ignoring case.

Search options:
  --print-query      on Esc, print the query and exit with 3
  --prompt TEXT      show TEXT, the shell prompt as bash prints it, instead of ›
  --backup           search every backup instead of $HISTFILE
  --backup-id LABEL  search the backup LABEL instead, like 20261030-142501

Keys:
  Up, Down, Ctrl-P, Ctrl-N  move the selection
  PgUp, PgDn                move the selection by a page
  Enter                     print the selected command
  Tab                       print the selected command, exit with 4
  Ctrl-D                    delete the selected command from history or backups
  Left, Right, Home, End    move the cursor in the query
  Ctrl-A, Ctrl-E            move the cursor to the start, the end
  Alt-B, Alt-F              move the cursor a word back, forward
  Alt/Ctrl-Left, -Right     move the cursor a word back, forward
  Backspace, Delete         delete the character before, at the cursor
  Alt-Backspace             delete the word before the cursor
  Ctrl-W                    delete back to whitespace
  Ctrl-U                    delete the query before the cursor
  Esc                       quit, exit with 1
  Ctrl-C                    quit, exit with 130

Without matches, Enter and Tab quit like Esc. With the list above the prompt
line, Up, Ctrl-P and PgUp move to older commands. In history, Ctrl-D leaves
backups as they are. In backups it asks first: y deletes the command from the
backups searched, any other key cancels.

Backups: $XDG_DATA_HOME/istoria/history.db, labeled by when they started, in
local time.

Config: $XDG_CONFIG_HOME/istoria/config.toml, defaults:

{defaults}

Errors exit with 2, signals with 128 + the signal.
";

const DELETE_LAST_ALONE: &str = "delete-last takes no other arguments";
const PROMPT_TEXT: &str = "--prompt takes the prompt text";
const BACKUP_LABEL: &str = "--backup-id takes a backup label";
const ONE_BACKUP_LABEL: &str = "--backup-id takes one backup label";
const BACKUP_AND_LABEL: &str = "--backup and --backup-id can't be used together";
const DELETE_BACKUP_LABEL: &str = "backup delete takes a backup label";

#[derive(Debug, PartialEq)]
pub enum Args {
    Search(Search),
    DeleteLast(usize),
    InitConfig,
    Backup,
    BackupList,
    /// The label of the backup
    BackupDelete(String),
    Help,
    Version,
}

#[derive(Debug, Default, PartialEq)]
pub struct Search {
    pub query: String,
    pub print_query: bool,
    pub prompt: Option<String>,
    pub searched: Searched,
}

/// What search reads the commands from
#[derive(Debug, Default, PartialEq)]
pub enum Searched {
    #[default]
    History,
    /// Every backup
    Backups,
    /// One backup, by its label
    Backup(String),
}

/// The arguments without the program name
pub fn parse_args(args: impl IntoIterator<Item = OsString>) -> Result<Args, String> {
    let args: Vec<OsString> = args.into_iter().collect();

    let Some((command, args)) = args.split_first() else {
        return Ok(Args::Help);
    };

    let command = command.to_string_lossy();

    match command.as_ref() {
        "search" => parse_search(args),
        "delete-last" => parse_delete_last(args),
        "init-config" => parse_alone(args, Args::InitConfig, "init-config"),
        "backup" => parse_backup(args),
        _ => unknown(&command, "command"),
    }
}

/// The arguments after `search`, options and query words in any order
fn parse_search(args: &[OsString]) -> Result<Args, String> {
    let mut words = Vec::new();
    let mut options = true;
    let mut search = Search::default();
    let mut backups = false;
    let mut label = None;
    let mut args = args.iter();

    while let Some(arg) = args.next() {
        // A query with its invalid bytes replaced would change the line bash gets back
        let Some(text) = arg.to_str() else {
            return Err(format!("not valid UTF-8: {}", arg.to_string_lossy()));
        };

        match text {
            "--" if options => options = false,
            "-h" | "--help" if options => return Ok(Args::Help),
            "-V" | "--version" if options => return Ok(Args::Version),
            "--print-query" if options => search.print_query = true,
            // Taken as is, a prompt may start with a dash. It is only shown, so invalid bytes
            // become replacement characters.
            "--prompt" if options => match args.next() {
                Some(text) => search.prompt = Some(text.to_string_lossy().into_owned()),
                None => return Err(usage_error(PROMPT_TEXT)),
            },
            "--backup" if options => backups = true,
            "--backup-id" if options => {
                if label.is_some() {
                    return Err(usage_error(ONE_BACKUP_LABEL));
                }

                match args.next() {
                    Some(text) => label = Some(parse_label(&text.to_string_lossy())?),
                    None => return Err(usage_error(BACKUP_LABEL)),
                }
            }
            option if options && option.starts_with('-') && option.len() > 1 => {
                return Err(usage_error(&format!("invalid option: {option}")));
            }
            _ => words.push(text.to_string()),
        }
    }

    search.query = words.join(" ");
    search.searched = match (backups, label) {
        (false, None) => Searched::History,
        (true, None) => Searched::Backups,
        (false, Some(label)) => Searched::Backup(label),
        (true, Some(_)) => return Err(usage_error(BACKUP_AND_LABEL)),
    };

    Ok(Args::Search(search))
}

/// The arguments after `delete-last`, which takes nothing but a count
fn parse_delete_last(args: &[OsString]) -> Result<Args, String> {
    match args {
        [] => Ok(Args::DeleteLast(1)),
        [arg] => parse_one(arg, |count| parse_count(count).map(Args::DeleteLast)),
        _ => Err(usage_error(DELETE_LAST_ALONE)),
    }
}

/// The arguments after `backup`: nothing, or `list` or `delete` and theirs
fn parse_backup(args: &[OsString]) -> Result<Args, String> {
    let Some((command, args)) = args.split_first() else {
        return Ok(Args::Backup);
    };

    let command = command.to_string_lossy();

    match command.as_ref() {
        "list" => parse_alone(args, Args::BackupList, "backup list"),
        "delete" => match args {
            [arg] => parse_one(arg, |label| parse_label(label).map(Args::BackupDelete)),
            _ => Err(usage_error(DELETE_BACKUP_LABEL)),
        },
        _ => unknown(&command, "backup command"),
    }
}

/// The arguments after a command that takes none
fn parse_alone(args: &[OsString], parsed: Args, command: &str) -> Result<Args, String> {
    match args {
        [] => Ok(parsed),
        [arg] => parse_one(arg, |_| Err(no_arguments(command))),
        _ => Err(no_arguments(command)),
    }
}

fn no_arguments(command: &str) -> String {
    usage_error(&format!("{command} takes no arguments"))
}

/// The only argument of a command: help or version, else what parse makes of it
fn parse_one(
    arg: &OsString,
    parse: impl FnOnce(&str) -> Result<Args, String>,
) -> Result<Args, String> {
    let arg = arg.to_string_lossy();

    exit_option(&arg).map_or_else(|| parse(&arg), Ok)
}

/// Help and version in place of a command, else an invalid option or command
fn unknown(word: &str, kind: &str) -> Result<Args, String> {
    match exit_option(word) {
        Some(exit) => Ok(exit),
        None if word.starts_with('-') => Err(usage_error(&format!("invalid option: {word}"))),
        None => Err(usage_error(&format!("invalid {kind}: {word}"))),
    }
}

/// Help and version, which every command takes
fn exit_option(arg: &str) -> Option<Args> {
    match arg {
        "-h" | "--help" => Some(Args::Help),
        "-V" | "--version" => Some(Args::Version),
        _ => None,
    }
}

fn parse_label(label: &str) -> Result<String, String> {
    match backup::parse_label(label) {
        Some(_) => Ok(label.to_string()),
        None => Err(usage_error(&format!("invalid backup label: {label}"))),
    }
}

/// Digits only, so a sign or an option in its place is rejected
fn parse_count(count: &str) -> Result<usize, String> {
    let digits = !count.is_empty() && count.bytes().all(|byte| byte.is_ascii_digit());

    match count.parse() {
        Ok(number) if digits && number > 0 => Ok(number),
        _ => Err(usage_error(&format!("invalid count: {count}"))),
    }
}

fn usage_error(message: &str) -> String {
    format!("{message}\nTry 'istoria --help' for usage.")
}

/// The usage with the default config, indented
pub fn help() -> String {
    let defaults: String = config::DEFAULT
        .lines()
        .flat_map(|line| ["    ", line, "\n"])
        .collect();

    USAGE.replace("{defaults}\n", &defaults)
}

#[cfg(test)]
mod tests {
    use std::os::unix::ffi::OsStringExt;

    use super::*;

    const LABEL: &str = "20261030-142501";

    fn parse(args: &[&str]) -> Result<Args, String> {
        parse_args(args.iter().map(OsString::from))
    }

    fn invalid_utf8(text: &[u8]) -> OsString {
        OsString::from_vec(text.to_vec())
    }

    fn search(query: &str, print_query: bool) -> Args {
        Args::Search(Search {
            query: query.to_string(),
            print_query,
            ..Search::default()
        })
    }

    fn search_in(searched: Searched, query: &str) -> Args {
        Args::Search(Search {
            query: query.to_string(),
            searched,
            ..Search::default()
        })
    }

    fn error(args: &[&str]) -> String {
        let error = parse(args).unwrap_err();
        assert!(
            error.ends_with("\nTry 'istoria --help' for usage."),
            "{error}"
        );

        error.lines().next().unwrap().to_string()
    }

    #[test]
    fn shows_help_without_a_command() {
        assert_eq!(parse(&[]), Ok(Args::Help));
    }

    #[test]
    fn joins_the_words_into_the_query() {
        assert_eq!(parse(&["search"]), Ok(search("", false)));
        assert_eq!(
            parse(&["search", "git", "log"]),
            Ok(search("git log", false))
        );
    }

    #[test]
    fn reads_options_between_the_words() {
        assert_eq!(
            parse(&["search", "git", "--print-query", "log"]),
            Ok(search("git log", true))
        );
    }

    #[test]
    fn takes_the_next_argument_for_the_prompt() {
        assert_eq!(
            parse(&["search", "--prompt", "-$ ", "--print-query", "--", "git"]),
            Ok(Args::Search(Search {
                query: "git".to_string(),
                print_query: true,
                prompt: Some("-$ ".to_string()),
                searched: Searched::History,
            }))
        );
    }

    #[test]
    fn replaces_invalid_bytes_in_the_prompt() {
        let args = [
            OsString::from("search"),
            OsString::from("--prompt"),
            invalid_utf8(b"\xff$ "),
        ];

        assert_eq!(
            parse_args(args),
            Ok(Args::Search(Search {
                prompt: Some("\u{fffd}$ ".to_string()),
                ..Search::default()
            }))
        );
    }

    #[test]
    fn rejects_invalid_bytes_in_the_query() {
        assert_eq!(
            parse_args([OsString::from("search"), invalid_utf8(b"ls \xff")]),
            Err("not valid UTF-8: ls \u{fffd}".to_string())
        );
    }

    #[test]
    fn rejects_a_prompt_option_without_the_text() {
        assert_eq!(error(&["search", "--prompt"]), PROMPT_TEXT);
    }

    #[test]
    fn takes_everything_after_a_double_dash_for_the_query() {
        assert_eq!(
            parse(&["search", "--", "--help", "-x", "--backup"]),
            Ok(search("--help -x --backup", false))
        );
    }

    #[test]
    fn takes_a_lone_dash_for_the_query() {
        assert_eq!(parse(&["search", "cd", "-"]), Ok(search("cd -", false)));
    }

    #[test]
    fn takes_command_names_after_search_for_the_query() {
        assert_eq!(
            parse(&["search", "backup", "list"]),
            Ok(search("backup list", false))
        );
    }

    #[test]
    fn reads_help_and_version_anywhere_in_a_search() {
        assert_eq!(parse(&["search", "ls", "--help"]), Ok(Args::Help));
        assert_eq!(parse(&["search", "-h"]), Ok(Args::Help));
        assert_eq!(parse(&["search", "ls", "-V"]), Ok(Args::Version));
        assert_eq!(parse(&["search", "--version"]), Ok(Args::Version));
    }

    #[test]
    fn reads_help_and_version_in_place_of_a_command() {
        assert_eq!(parse(&["-h"]), Ok(Args::Help));
        assert_eq!(parse(&["--help", "search"]), Ok(Args::Help));
        assert_eq!(parse(&["-V"]), Ok(Args::Version));
        assert_eq!(parse(&["--version"]), Ok(Args::Version));
    }

    #[test]
    fn rejects_unknown_commands_and_options() {
        assert_eq!(error(&["git"]), "invalid command: git");
        assert_eq!(error(&["-x"]), "invalid option: -x");
        assert_eq!(error(&["--print-query"]), "invalid option: --print-query");
        assert_eq!(error(&["search", "-x"]), "invalid option: -x");
    }

    #[test]
    fn rejects_the_options_of_old_versions() {
        assert_eq!(error(&["--delete-last"]), "invalid option: --delete-last");
        assert_eq!(error(&["--init-config"]), "invalid option: --init-config");
        assert_eq!(
            error(&["search", "--delete-last"]),
            "invalid option: --delete-last"
        );
    }

    #[test]
    fn deletes_one_command_by_default() {
        assert_eq!(parse(&["delete-last"]), Ok(Args::DeleteLast(1)));
        assert_eq!(parse(&["delete-last", "10"]), Ok(Args::DeleteLast(10)));
    }

    #[test]
    fn shows_help_and_version_after_commands() {
        for command in [
            &["delete-last"][..],
            &["init-config"],
            &["backup"],
            &["backup", "list"],
            &["backup", "delete"],
        ] {
            for help in ["-h", "--help"] {
                assert_eq!(parse(&[command, &[help]].concat()), Ok(Args::Help));
            }
            for version in ["-V", "--version"] {
                assert_eq!(parse(&[command, &[version]].concat()), Ok(Args::Version));
            }
        }
    }

    #[test]
    fn rejects_delete_last_with_other_arguments() {
        assert_eq!(error(&["delete-last", "1", "2"]), DELETE_LAST_ALONE);
    }

    #[test]
    fn rejects_counts_other_than_positive_numbers() {
        for count in ["0", "-1", "+1", "1x", "", "--print-query"] {
            assert_eq!(
                error(&["delete-last", count]),
                format!("invalid count: {count}")
            );
        }
    }

    #[test]
    fn writes_the_config_without_arguments() {
        assert_eq!(parse(&["init-config"]), Ok(Args::InitConfig));
        assert_eq!(
            error(&["init-config", "x"]),
            "init-config takes no arguments"
        );
    }

    #[test]
    fn reads_the_backup_commands() {
        assert_eq!(parse(&["backup"]), Ok(Args::Backup));
        assert_eq!(parse(&["backup", "list"]), Ok(Args::BackupList));
        assert_eq!(
            parse(&["backup", "delete", LABEL]),
            Ok(Args::BackupDelete(LABEL.to_string()))
        );
    }

    #[test]
    fn rejects_backup_commands_with_wrong_arguments() {
        assert_eq!(error(&["backup", "x"]), "invalid backup command: x");
        assert_eq!(error(&["backup", "-x"]), "invalid option: -x");
        assert_eq!(
            error(&["backup", "list", "x"]),
            "backup list takes no arguments"
        );
        assert_eq!(error(&["backup", "delete"]), DELETE_BACKUP_LABEL);
        assert_eq!(
            error(&["backup", "delete", LABEL, LABEL]),
            DELETE_BACKUP_LABEL
        );
        assert_eq!(
            error(&["backup", "delete", "2026"]),
            "invalid backup label: 2026"
        );
    }

    #[test]
    fn searches_the_backups() {
        assert_eq!(
            parse(&["search", "--backup", "git"]),
            Ok(search_in(Searched::Backups, "git"))
        );
        assert_eq!(
            parse(&["search", "git", "--backup-id", LABEL]),
            Ok(search_in(Searched::Backup(LABEL.to_string()), "git"))
        );
    }

    #[test]
    fn rejects_backup_options_that_go_apart() {
        assert_eq!(error(&["search", "--backup-id"]), BACKUP_LABEL);
        assert_eq!(
            error(&["search", "--backup-id", "x"]),
            "invalid backup label: x"
        );
        assert_eq!(
            error(&["search", "--backup-id", LABEL, "--backup-id", LABEL]),
            ONE_BACKUP_LABEL
        );
        assert_eq!(
            error(&["search", "--backup", "--backup-id", LABEL]),
            BACKUP_AND_LABEL
        );
    }

    #[test]
    fn takes_a_prompt_with_backups() {
        assert_eq!(
            parse(&["search", "--prompt", "$ ", "--backup"]),
            Ok(Args::Search(Search {
                prompt: Some("$ ".to_string()),
                searched: Searched::Backups,
                ..Search::default()
            }))
        );
    }
}
