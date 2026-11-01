//! The distinct commands, the query and its matches

use std::collections::{BTreeSet, HashMap};
use std::mem;
use std::ops::Range;

use chrono::{DateTime, Local};
use unicode_width::UnicodeWidthStr;

use crate::config::SearchMode;
use crate::history::Entry;

const TIME_FORMAT: &str = "%Y-%m-%d %H:%M";
pub const TIME_WIDTH: usize = "2023-11-14 22:13".len();
/// Query words this long or longer match with a typo, shorter ones would match almost anything
const TYPO_MIN_CHARS: usize = 3;

/// A distinct command and every history entry holding it, most recent first
#[derive(Debug)]
pub struct Command {
    /// As shown in the finder, without trailing whitespace
    pub text: String,
    /// As stored in the most recent entry, for putting on the prompt line
    pub original: String,
    /// Local time of the most recent entry with a timestamp
    pub time: Option<String>,
    /// The hosts of its entries' backups, sorted and apart by commas, empty in history
    pub hosts: String,
    pub entry_indices: Vec<usize>,
}

#[derive(Debug)]
pub struct Match {
    pub command_index: usize,
    /// Matched bytes of the command: sorted, apart, none for a query without words
    pub ranges: Vec<Range<usize>>,
}

#[derive(Debug)]
pub struct Finder {
    commands: Vec<Command>,
    /// Whether to show a time column
    has_times: bool,
    /// The widest hosts of a command, 0 without a hosts column
    hosts_width: usize,
    mode: SearchMode,
    /// Whether query words match with a typo, after the commands they match without
    typos: bool,
    query: String,
    /// Byte offset into the query, on a character boundary
    cursor: usize,
    matches: Vec<Match>,
    /// Index into the matches, 0 without any
    selected: usize,
}

impl Finder {
    pub fn new(entries: &[Entry], query: String, mode: SearchMode, typos: bool) -> Self {
        let commands = commands(entries);
        let mut finder = Self {
            has_times: commands.iter().any(|command| command.time.is_some()),
            hosts_width: commands
                .iter()
                .map(|command| command.hosts.width())
                .max()
                .unwrap_or(0),
            commands,
            mode,
            typos,
            cursor: query.len(),
            query,
            matches: Vec::new(),
            selected: 0,
        };
        finder.refilter();
        finder
    }

    /// Rebuilds the commands from reloaded entries, keeping the query, its cursor and the selection
    pub fn reload(&mut self, entries: &[Entry]) {
        let (selected, cursor) = (self.selected, self.cursor);
        *self = Self::new(entries, mem::take(&mut self.query), self.mode, self.typos);
        self.cursor = cursor;
        self.select(selected);
    }

    pub fn commands(&self) -> &[Command] {
        &self.commands
    }

    pub fn has_times(&self) -> bool {
        self.has_times
    }

    pub fn hosts_width(&self) -> usize {
        self.hosts_width
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn matches(&self) -> &[Match] {
        &self.matches
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn push(&mut self, c: char) {
        self.query.insert(self.cursor, c);
        self.cursor += c.len_utf8();
        self.refilter();
    }

    pub fn delete_back(&mut self) {
        if let Some(start) = self.previous_boundary() {
            self.delete_back_to(start);
        }
    }

    pub fn delete_forward(&mut self) {
        if self.cursor < self.query.len() {
            self.query.remove(self.cursor);
            self.refilter();
        }
    }

    /// Like Ctrl-U in bash, keeps what is after the cursor
    pub fn delete_to_start(&mut self) {
        self.delete_back_to(0);
    }

    /// Letters and digits, like Alt-Backspace in bash
    pub fn delete_word_back(&mut self) {
        self.delete_back_to(self.word_start(char::is_alphanumeric));
    }

    /// Everything up to whitespace, like Ctrl-W in bash
    pub fn delete_big_word_back(&mut self) {
        self.delete_back_to(self.word_start(|c| !c.is_whitespace()));
    }

    pub fn left(&mut self) {
        if let Some(start) = self.previous_boundary() {
            self.cursor = start;
        }
    }

    pub fn right(&mut self) {
        if let Some(end) = self.next_boundary() {
            self.cursor = end;
        }
    }

    pub fn word_left(&mut self) {
        self.cursor = self.word_start(char::is_alphanumeric);
    }

    pub fn word_right(&mut self) {
        let after = &self.query[self.cursor..];
        let rest = after
            .trim_start_matches(|c: char| !c.is_alphanumeric())
            .trim_start_matches(char::is_alphanumeric);

        self.cursor = self.query.len() - rest.len();
    }

    pub fn home(&mut self) {
        self.cursor = 0;
    }

    pub fn end(&mut self) {
        self.cursor = self.query.len();
    }

    pub fn up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    pub fn down(&mut self) {
        self.select(self.selected + 1);
    }

    pub fn page_up(&mut self, rows: usize) {
        self.selected = self.selected.saturating_sub(rows.max(1));
    }

    pub fn page_down(&mut self, rows: usize) {
        self.select(self.selected + rows.max(1));
    }

    /// Clamped to the matches
    pub fn select(&mut self, index: usize) {
        self.selected = index.min(self.matches.len().saturating_sub(1));
    }

    pub fn selection(&self) -> Option<&Command> {
        self.matches
            .get(self.selected)
            .map(|found| &self.commands[found.command_index])
    }

    fn delete_back_to(&mut self, start: usize) {
        if start < self.cursor {
            self.query.drain(start..self.cursor);
            self.cursor = start;
            self.refilter();
        }
    }

    /// Start of the word before the cursor, `in_word` tells which characters make words
    fn word_start(&self, in_word: impl Fn(char) -> bool) -> usize {
        self.query[..self.cursor]
            .trim_end_matches(|c| !in_word(c))
            .trim_end_matches(in_word)
            .len()
    }

    /// Where the character before the cursor starts
    fn previous_boundary(&self) -> Option<usize> {
        let (start, _) = self.query[..self.cursor].char_indices().next_back()?;

        Some(start)
    }

    /// Where the character after the cursor ends
    fn next_boundary(&self) -> Option<usize> {
        let c = self.query[self.cursor..].chars().next()?;

        Some(self.cursor + c.len_utf8())
    }

    fn refilter(&mut self) {
        let find = match self.mode {
            SearchMode::Keyword => find_words,
            SearchMode::Prefix => find_prefix,
        };

        let mut matches: Vec<_> = self
            .commands
            .iter()
            .enumerate()
            .filter_map(|(command_index, Command { text, .. })| {
                let (ranges, typo) = find(text, &self.query, self.typos)?;
                let found = Match {
                    command_index,
                    ranges,
                };

                Some((found, typo))
            })
            .collect();
        // Stable, the matches with typos go last and stay most recent first
        matches.sort_by_key(|&(_, typo)| typo);

        self.matches = matches.into_iter().map(|(found, _)| found).collect();
        self.selected = 0;
    }
}

/// One command per distinct text, most recent first. `ls ` and `ls` are one, bash keeps the space
/// after a completion.
fn commands(entries: &[Entry]) -> Vec<Command> {
    let mut commands: Vec<Command> = Vec::new();
    let mut hosts: Vec<BTreeSet<&str>> = Vec::new();
    let mut positions: HashMap<&str, usize> = HashMap::new();

    for (index, entry) in entries.iter().enumerate().rev() {
        let text = entry.text.trim_end();
        let position = *positions.entry(text).or_insert_with(|| {
            commands.push(Command {
                text: text.to_string(),
                original: entry.original.clone(),
                time: None,
                hosts: String::new(),
                entry_indices: Vec::new(),
            });
            hosts.push(BTreeSet::new());
            commands.len() - 1
        });
        let command = &mut commands[position];

        command.entry_indices.push(index);
        command.time = command.time.take().or_else(|| local_time(entry.time?));
        hosts[position].extend(entry.hosts.iter().map(String::as_str));
    }

    for (command, hosts) in commands.iter_mut().zip(hosts) {
        command.hosts = hosts.into_iter().collect::<Vec<_>>().join(",");
    }

    commands
}

fn local_time(seconds: i64) -> Option<String> {
    let time = DateTime::from_timestamp(seconds, 0)?.with_timezone(&Local);

    Some(time.format(TIME_FORMAT).to_string())
}

/// Each query word's first occurrence, in any order, ignoring case, touching ranges merged, and
/// whether a word matched with a typo, allowed by typos for a word that is missing otherwise. None
/// when a word is missing.
fn find_words(haystack: &str, query: &str, typos: bool) -> Option<(Vec<Range<usize>>, bool)> {
    let mut typo = false;
    let mut found = query
        .split_whitespace()
        .map(|word| match find(haystack, word) {
            Some(range) => Some(range),
            None if typos => {
                typo = true;
                find_typo(haystack, word)
            }
            None => None,
        })
        .collect::<Option<Vec<_>>>()?;
    found.sort_by_key(|range| range.start);

    let mut ranges: Vec<Range<usize>> = Vec::with_capacity(found.len());

    for range in found {
        match ranges.last_mut() {
            Some(last) if range.start <= last.end => last.end = last.end.max(range.end),
            _ => ranges.push(range),
        }
    }

    Some((ranges, typo))
}

/// The query at the start of the command, ignoring case and whitespace before either, and whether
/// its first word matched with a typo, allowed by typos when the query is missing otherwise.
/// Whitespace after the query also matches the end of the command, which has none after it. None
/// when the command starts otherwise.
fn find_prefix(haystack: &str, query: &str, typos: bool) -> Option<(Vec<Range<usize>>, bool)> {
    let start = haystack.len() - haystack.trim_start().len();
    let command = &haystack[start..];
    let query = query.trim_start();
    let text = query.trim_end();
    let ends_right = |len: usize| {
        let rest = &command[len..];

        text.len() == query.len() || rest.is_empty() || rest.starts_with(char::is_whitespace)
    };
    let (len, typo) = match prefix_len(command, text) {
        Some(len) => (ends_right(len).then_some(len)?, false),
        None if typos => (typo_prefix_len(command, text, ends_right)?, true),
        None => return None,
    };
    let range = (!text.is_empty()).then_some(start..start + len);

    Some((range.into_iter().collect(), typo))
}

/// The first word of the haystack that starts with the query word but for a typo, see `typo_lens`
fn find_typo(haystack: &str, word: &str) -> Option<Range<usize>> {
    let word = typo_word(word)?;

    word_starts(haystack).find_map(|start| {
        let len = *typo_lens(&haystack[start..], &word).first()?;

        Some(start..start + len)
    })
}

/// Byte length of text at the start of haystack, ignoring case, its first word but for a typo, the
/// first one that `ends_right` takes
fn typo_prefix_len(
    haystack: &str,
    text: &str,
    ends_right: impl Fn(usize) -> bool,
) -> Option<usize> {
    let (word, rest) = text.split_at(text.find(char::is_whitespace).unwrap_or(text.len()));
    let word = typo_word(word)?;

    typo_lens(haystack, &word)
        .into_iter()
        .filter_map(|len| Some(len + prefix_len(&haystack[len..], rest)?))
        .find(|&len| ends_right(len))
}

/// The query word to match with a typo, case folded, None when too short for one
fn typo_word(word: &str) -> Option<Vec<char>> {
    let word: Vec<char> = word.chars().map(fold).collect();

    (word.len() >= TYPO_MIN_CHARS).then_some(word)
}

/// Where words start: after whitespace, and letters or digits after other characters
fn word_starts(haystack: &str) -> impl Iterator<Item = usize> {
    let mut previous = ' ';

    haystack.char_indices().filter_map(move |(start, c)| {
        let starts = !c.is_whitespace()
            && (previous.is_whitespace() || c.is_alphanumeric() && !previous.is_alphanumeric());
        previous = c;

        starts.then_some(start)
    })
}

/// Byte lengths of the starts of the haystack's first word that are the query word but for a typo,
/// whole words first. One that matches only the start of the query word, a character shorter or
/// with the last one changed, has to be the whole word, or `gix` would find `gist`.
fn typo_lens(haystack: &str, word: &[char]) -> Vec<usize> {
    let first_word = haystack
        .split(char::is_whitespace)
        .next()
        .unwrap_or_default();
    if !may_be_one_typo(first_word, word) {
        return Vec::new();
    }

    let chars: Vec<(usize, char)> = first_word.char_indices().collect();
    let folded: Vec<char> = chars.iter().map(|&(_, c)| fold(c)).collect();
    let n = word.len();
    let ends_word = |count: usize| chars.get(count).is_none_or(|&(_, c)| !c.is_alphanumeric());
    let only_start = |count: usize| count < n || count == n && folded[..n - 1] == word[..n - 1];
    let mut counts: Vec<(usize, bool)> = [n, n + 1, n - 1]
        .into_iter()
        .filter(|&count| count <= folded.len() && at_most_one_typo(word, &folded[..count]))
        .map(|count| (count, ends_word(count)))
        .filter(|&(count, whole)| whole || !only_start(count))
        .collect();
    counts.sort_by_key(|&(_, whole)| !whole);

    counts
        .into_iter()
        .map(|(count, _)| chars.get(count).map_or(first_word.len(), |&(end, _)| end))
        .collect()
}

/// A quick check before `at_most_one_typo`: with one typo, the first two characters of the words
/// share one
fn may_be_one_typo(first_word: &str, word: &[char]) -> bool {
    let word_start = &word[..2];

    first_word
        .chars()
        .take(2)
        .map(fold)
        .any(|c| word_start.contains(&c))
}

/// Whether a and b are the same but for at most one typo: a character missing, extra, changed, or
/// swapped with the next one
fn at_most_one_typo(a: &[char], b: &[char]) -> bool {
    let (short, long) = if a.len() <= b.len() { (a, b) } else { (b, a) };
    let Some(i) = short.iter().zip(long).position(|(x, y)| x != y) else {
        return long.len() - short.len() <= 1;
    };

    match long.len() - short.len() {
        0 => {
            let swapped = short.get(i + 1) == Some(&long[i]) && long.get(i + 1) == Some(&short[i]);

            short[i + 1..] == long[i + 1..] || swapped && short[i + 2..] == long[i + 2..]
        }
        1 => short[i..] == long[i + 1..],
        _ => false,
    }
}

/// Case-insensitive substring search
fn find(haystack: &str, needle: &str) -> Option<Range<usize>> {
    let mut starts = haystack
        .char_indices()
        .map(|(start, _)| start)
        .chain([haystack.len()]);

    starts.find_map(|start| Some(start..start + prefix_len(&haystack[start..], needle)?))
}

/// Byte length of needle at the start of haystack, ignoring case
fn prefix_len(haystack: &str, needle: &str) -> Option<usize> {
    let mut chars = haystack.chars();
    let mut len = 0;

    for expected in needle.chars() {
        let c = chars.next().filter(|&c| fold(c) == fold(expected))?;
        len += c.len_utf8();
    }

    Some(len)
}

fn fold(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

#[cfg(test)]
// A command matched in one place has one range of bytes, not a range to collect
#[allow(clippy::single_range_in_vec_init)]
mod tests {
    use super::*;
    use crate::{history, test_util};

    /// Two commands match p
    const HISTORY: &[u8] = b"ls\npwd\nps\n";

    fn finder(query: &str) -> Finder {
        test_util::finder(&history::parse(HISTORY), query)
    }

    /// The texts of the matches, in order
    fn texts(finder: &Finder) -> Vec<&str> {
        finder
            .matches()
            .iter()
            .map(|found| finder.commands()[found.command_index].text.as_str())
            .collect()
    }

    #[test]
    fn starts_with_the_cursor_after_the_query() {
        assert_eq!(finder("ls").cursor, 2);
    }

    #[test]
    fn types_and_deletes_at_the_cursor() {
        let mut finder = finder("sbx rn");

        finder.left();
        finder.push('u');
        assert_eq!((finder.query.as_str(), finder.cursor), ("sbx run", 6));

        finder.delete_back();
        assert_eq!((finder.query.as_str(), finder.cursor), ("sbx rn", 5));

        finder.delete_forward();
        assert_eq!((finder.query.as_str(), finder.cursor), ("sbx r", 5));
    }

    #[test]
    fn moves_the_cursor_by_characters_within_the_query() {
        let mut finder = finder("äö");

        finder.left();
        assert_eq!(finder.cursor, 2);
        finder.left();
        finder.left();
        assert_eq!(finder.cursor, 0);
        finder.right();
        assert_eq!(finder.cursor, 2);
        finder.end();
        finder.right();
        assert_eq!(finder.cursor, 4);
        finder.home();
        assert_eq!(finder.cursor, 0);
    }

    #[test]
    fn deletes_before_the_cursor_like_bash() {
        let mut finder = finder("git log");

        for _ in 0..3 {
            finder.left();
        }
        finder.delete_to_start();

        assert_eq!((finder.query.as_str(), finder.cursor), ("log", 0));
    }

    #[test]
    fn deletes_nothing_past_the_ends() {
        let mut finder = finder("p");
        finder.down();

        finder.delete_forward();
        finder.home();
        finder.delete_back();
        finder.delete_to_start();

        // The selection stays, as the matches did not change
        assert_eq!((finder.query.as_str(), finder.selected), ("p", 1));
    }

    #[test]
    fn moves_the_selection_by_pages_and_stops_at_the_ends() {
        let mut finder = finder("");

        finder.page_down(2);
        assert_eq!(finder.selected, 2);

        finder.page_down(2);
        assert_eq!(finder.selected, 2);

        finder.page_up(1);
        assert_eq!(finder.selected, 1);

        finder.page_up(2);
        assert_eq!(finder.selected, 0);

        // A pane too short for any rows still moves
        finder.page_down(0);
        assert_eq!(finder.selected, 1);
    }

    #[test]
    fn moves_the_cursor_by_words_of_letters_and_digits() {
        let mut finder = finder("git log --all");

        for cursor in [10, 4, 0, 0] {
            finder.word_left();
            assert_eq!(finder.cursor, cursor);
        }

        for cursor in [3, 7, 13, 13] {
            finder.word_right();
            assert_eq!(finder.cursor, cursor);
        }
    }

    #[test]
    fn deletes_words_before_the_cursor() {
        let mut finder = finder("cd ~/src/utils ");

        finder.delete_word_back();
        assert_eq!((finder.query.as_str(), finder.cursor), ("cd ~/src/", 9));

        finder.delete_big_word_back();
        assert_eq!((finder.query.as_str(), finder.cursor), ("cd ", 3));

        finder.home();
        finder.delete_big_word_back();
        assert_eq!(finder.query, "cd ");
    }

    #[test]
    fn keeps_the_cursor_on_reload() {
        let mut finder = finder("ls");

        finder.left();
        finder.reload(&history::parse(HISTORY));

        assert_eq!(finder.cursor, 1);
    }

    #[test]
    fn joins_the_hosts_of_every_entry_of_a_command() {
        let host_entry = |command: &str, hosts: &[&str]| {
            let hosts = hosts.iter().map(|host| (*host).to_string()).collect();
            Entry::new(command.to_string(), None, hosts)
        };
        let entries = [
            host_entry("ls", &["mac"]),
            host_entry("pwd", &["air"]),
            host_entry("ls ", &["studio", "air"]),
        ];
        let finder = test_util::finder(&entries, "");
        let hosts: Vec<(&str, &str)> = finder
            .commands()
            .iter()
            .map(|command| (command.text.as_str(), command.hosts.as_str()))
            .collect();

        assert_eq!(hosts, [("ls", "air,mac,studio"), ("pwd", "air")]);
        assert_eq!(finder.hosts_width(), "air,mac,studio".len());
    }

    #[test]
    fn has_no_hosts_in_history() {
        let finder = finder("");

        assert!(
            finder
                .commands()
                .iter()
                .all(|command| command.hosts.is_empty())
        );
        assert_eq!(finder.hosts_width(), 0);
    }

    #[test]
    fn formats_times_as_wide_as_the_column() {
        assert_eq!(local_time(0).unwrap().len(), TIME_WIDTH);
        assert_eq!(local_time(1_700_000_000).unwrap().len(), TIME_WIDTH);
    }

    #[test]
    fn finds_substrings_ignoring_case() {
        assert_eq!(find("sbx run Claude", "clau"), Some(8..12));
        assert_eq!(find("CLAUDE", "clau"), Some(0..4));
        assert_eq!(find("cl-au", "clau"), None);
    }

    #[test]
    fn finds_the_first_occurrence() {
        assert_eq!(find("ab ab", "ab"), Some(0..2));
    }

    #[test]
    fn empty_query_matches_everything() {
        assert_eq!(find("ls", ""), Some(0..0));
        assert_eq!(find("", ""), Some(0..0));
    }

    #[test]
    fn treats_special_characters_literally() {
        assert_eq!(find("ls *.rb", "*.rb"), Some(3..7));
        assert_eq!(find("claude", "*aude"), None);
    }

    #[test]
    fn finds_every_word_in_any_order() {
        assert_eq!(
            find_words("git -C api push", "push git", false).unwrap().0,
            [0..3, 11..15]
        );
        assert_eq!(find_words("git -C api push", "git pull", false), None);
    }

    #[test]
    fn merges_words_that_overlap_or_touch() {
        assert_eq!(find_words("git push", "gi git", false).unwrap().0, [0..3]);
        assert_eq!(find_words("ls ls", "ls ls", false).unwrap().0, [0..2]);
        assert_eq!(find_words("abc", "a bc", false).unwrap().0, [0..3]);
        assert_eq!(find_words("git push", "ls ls", false), None);
    }

    #[test]
    fn ignores_the_spaces_between_words() {
        assert_eq!(
            find_words("ls -la", "  ls\t -la  ", false).unwrap().0,
            [0..2, 3..6]
        );
        assert_eq!(find_words("lsof", "ls ", false).unwrap().0, [0..2]);
    }

    #[test]
    fn query_without_words_matches_everything() {
        for (haystack, query) in [("ls", ""), ("ls", "   "), ("", "")] {
            assert_eq!(
                find_words(haystack, query, false).unwrap().0,
                [],
                "{query:?}"
            );
        }
    }

    #[test]
    fn finds_words_ignoring_case_in_any_script() {
        assert_eq!(
            find_words("echo Привет Мир", "мир ПРИВЕТ", false)
                .unwrap()
                .0,
            [5..17, 18..24]
        );
    }

    #[test]
    fn finds_the_query_at_the_start_ignoring_case() {
        assert_eq!(find_prefix("git push", "git", false).unwrap().0, [0..3]);
        assert_eq!(find_prefix("Git Push", "git p", false).unwrap().0, [0..5]);
        assert_eq!(find_prefix("sudo git push", "git", false), None);
        assert_eq!(find_prefix("git  push", "git p", false), None);
        assert_eq!(find_prefix("git", "git push", false), None);
    }

    #[test]
    fn ignores_whitespace_before_the_prefix() {
        assert_eq!(find_prefix("ls -la", "  ls", false).unwrap().0, [0..2]);
        assert_eq!(find_prefix(" ls -la", "ls", false).unwrap().0, [1..3]);
    }

    #[test]
    fn matches_whitespace_after_the_prefix_at_a_word_end() {
        assert_eq!(find_prefix("ls", "ls ", false).unwrap().0, [0..2]);
        assert_eq!(find_prefix("ls -la", "ls  ", false).unwrap().0, [0..2]);
        assert_eq!(find_prefix("lsof", "ls ", false), None);
        assert_eq!(find_prefix("lsof", "ls", false).unwrap().0, [0..2]);
    }

    #[test]
    fn prefix_without_text_matches_everything() {
        for (haystack, query) in [("ls", ""), ("ls", "   "), ("", ""), (" ls", " ")] {
            assert_eq!(
                find_prefix(haystack, query, false).unwrap().0,
                [],
                "{query:?}"
            );
        }
    }

    #[test]
    fn finds_prefixes_in_any_script() {
        assert_eq!(
            find_prefix("Привет мир", "привет ", false).unwrap().0,
            [0..12]
        );
    }

    #[test]
    fn filters_commands_by_the_search_mode() {
        let entries = history::parse(
            b"ls -la
sudo ls
lsof
ls
",
        );
        let finder = |mode| Finder::new(&entries, "ls ".to_string(), mode, false);

        assert_eq!(
            texts(&finder(SearchMode::Keyword)),
            ["ls", "lsof", "sudo ls", "ls -la"]
        );
        assert_eq!(texts(&finder(SearchMode::Prefix)), ["ls", "ls -la"]);
    }

    #[test]
    fn keeps_the_search_mode_on_reload() {
        let entries = history::parse(
            b"sudo ls
ls
",
        );
        let mut finder = Finder::new(&entries, "ls".to_string(), SearchMode::Prefix, false);

        finder.reload(&entries);

        assert_eq!(finder.matches().len(), 1);
    }

    #[test]
    fn lists_commands_matched_with_a_typo_after_the_others() {
        let entries = history::parse(
            b"git push
gti status
git log
",
        );
        let finder = Finder::new(&entries, "gti".to_string(), SearchMode::Keyword, true);

        assert_eq!(texts(&finder), ["gti status", "git log", "git push"]);
    }

    #[test]
    fn matches_typos_only_when_asked() {
        let entries = history::parse(b"git push\n");
        let finder = |typos| Finder::new(&entries, "gti".to_string(), SearchMode::Keyword, typos);

        assert_eq!(finder(true).matches().len(), 1);
        assert!(finder(false).matches().is_empty());
    }

    #[test]
    fn keeps_typos_on_reload() {
        let entries = history::parse(b"git push\n");
        let mut finder = Finder::new(&entries, "gti".to_string(), SearchMode::Keyword, true);

        finder.reload(&entries);

        assert_eq!(finder.matches().len(), 1);
    }

    #[test]
    fn tells_one_typo_from_more() {
        let chars = |text: &str| text.chars().collect::<Vec<_>>();
        let one_typo = |a, b| at_most_one_typo(&chars(a), &chars(b));

        for (a, b) in [
            ("git", "git"),
            ("gti", "git"),
            ("igt", "git"),
            ("gut", "git"),
            ("giit", "git"),
            ("gt", "git"),
            ("dcker", "docker"),
        ] {
            assert!(one_typo(a, b), "{a} {b}");
            assert!(one_typo(b, a), "{b} {a}");
        }

        for (a, b) in [
            ("tig", "git"),
            ("gitxx", "git"),
            ("g", "git"),
            ("gxix", "git"),
        ] {
            assert!(!one_typo(a, b), "{a} {b}");
            assert!(!one_typo(b, a), "{b} {a}");
        }
    }

    #[test]
    fn checks_quickly_without_missing_typos_in_the_first_character() {
        let lens = |text, word: &str| typo_lens(text, &word.chars().collect::<Vec<_>>());

        // Changed, swapped, extra, missing
        for (text, word) in [
            ("git", "xit"),
            ("git", "igt"),
            ("git", "xgit"),
            ("gait", "ait"),
        ] {
            assert!(!lens(text, word).is_empty(), "{text} {word}");
        }

        assert_eq!(lens("push", "gti"), []);
        assert_eq!(lens("g", "gti"), []);
    }

    #[test]
    fn finds_words_with_a_typo_at_the_start_of_words() {
        let words = |haystack, query| find_words(haystack, query, true);

        assert_eq!(words("sudo git push", "gti"), Some((vec![5..8], true)));
        assert_eq!(
            words("git push", "gti psuh"),
            Some((vec![0..3, 4..8], true))
        );
        assert_eq!(
            words("sudo git push", "git psuh"),
            Some((vec![5..8, 9..13], true))
        );
        assert_eq!(
            words("docker-compose up", "dokcer comp"),
            Some((vec![0..6, 7..11], true))
        );
        assert_eq!(words("docker ps", "dcker"), Some((vec![0..6], true)));
        assert_eq!(
            words("git push --force", "--forse"),
            Some((vec![9..16], true))
        );
        assert_eq!(words("echo Привет", "пирвет"), Some((vec![5..17], true)));
        assert_eq!(words("git push", "git"), Some((vec![0..3], false)));
        assert_eq!(words("legit", "lgeit"), Some((vec![0..5], true)));
        assert_eq!(words("legit", "gti"), None);
    }

    #[test]
    fn finds_words_shorter_by_a_typo_only_whole() {
        assert_eq!(
            find_words("git push", "giit", true),
            Some((vec![0..3], true))
        );
        assert_eq!(find_words("gist", "gti", true), None);
    }

    #[test]
    fn finds_words_with_the_last_character_changed_only_whole() {
        assert_eq!(find_words("gist", "gix", true), None);
        assert_eq!(
            find_words("git push", "gix", true),
            Some((vec![0..3], true))
        );
        assert_eq!(find_words("ls --force", "--x", true), None);
    }

    #[test]
    fn highlights_whole_words_before_other_typos() {
        assert_eq!(
            find_words("docker ps", "dockr", true),
            Some((vec![0..6], true))
        );
    }

    #[test]
    fn finds_short_words_only_without_typos() {
        assert_eq!(find_words("ls -la", "sl", true), None);
        assert_eq!(find_words("ls -la", "sl gti", true), None);
    }

    #[test]
    fn finds_the_prefix_with_a_typo_in_its_first_word() {
        let prefix = |haystack, query| find_prefix(haystack, query, true);

        assert_eq!(prefix("git push", "gti"), Some((vec![0..3], true)));
        assert_eq!(prefix("git push", "gti pu"), Some((vec![0..6], true)));
        assert_eq!(prefix(" docker ps", "dcker p"), Some((vec![1..9], true)));
        assert_eq!(prefix("git push", "gti "), Some((vec![0..3], true)));
        assert_eq!(prefix("gitk", "gti "), None);
        assert_eq!(prefix("docker ps", "dockr "), Some((vec![0..6], true)));
        assert_eq!(prefix("abdcd x", "abcd "), Some((vec![0..5], true)));
        assert_eq!(prefix("git push", "git psuh"), None);
        assert_eq!(prefix("sudo git push", "gti"), None);
        assert_eq!(prefix("git push", "git"), Some((vec![0..3], false)));
    }

    #[test]
    fn handles_multibyte_characters() {
        assert_eq!(find("echo Привет", "привет"), Some(5..17));
        assert_eq!(find("ünï", "ï"), Some(3..5));
    }
}
