//! The finder running in its pane: the keys and how it was left

use std::io;
use std::ops::ControlFlow::{self, Break, Continue};
use std::sync::atomic::Ordering;

use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::text::Line;

use crate::backup::Backups;
use crate::config::{Config, ListPosition};
use crate::context::Context;
use crate::finder::Finder;
use crate::history::{Entry, History};
use crate::pane::Pane;
use crate::ui::Status;
use crate::{tty, ui};

/// After Ctrl-D in backups, deletes the command
const CONFIRM_KEY: char = 'y';

/// Where the commands come from
pub enum Source {
    History(History),
    /// From one backup, with its label and start, or from all of them
    Backups {
        /// None before the first backup
        backups: Option<Backups>,
        entries: Vec<Entry>,
        label: Option<String>,
        started_at: Option<i64>,
    },
}

impl Source {
    pub fn entries(&self) -> &[Entry] {
        match self {
            Self::History(history) => history.entries(),
            Self::Backups { entries, .. } => entries,
        }
    }

    /// Shown before ›: `backup`, or `backup:20261030-142501` for one
    fn prompt_label(&self) -> Option<String> {
        match self {
            Self::History(_) => None,
            Self::Backups { label: None, .. } => Some("backup".to_string()),
            Self::Backups {
                label: Some(label), ..
            } => Some(format!("backup:{label}")),
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum Outcome {
    /// Enter on a command, to run it
    Picked(String),
    /// Tab on a command, to edit it first
    Edited(String),
    /// Esc, or Enter and Tab without matches
    Quit,
    /// Ctrl-C
    Abort,
    /// SIGINT, SIGTERM or SIGHUP, with its number
    Signaled(usize),
}

/// Runs the finder until a key or a signal ends it
pub fn pick(
    finder: &mut Finder,
    source: &mut Source,
    config: &Config,
    prompt: Option<&Line>,
) -> io::Result<Outcome> {
    let list = config.list;
    let mut pane = Pane::open(config.height)?;
    let mut offset = 0;
    let mut status = None;
    let mut page = 1;
    let signal = tty::watch_signals()?;
    let label = source.prompt_label();

    let outcome = loop {
        pane.draw(|frame| {
            page = ui::draw(
                frame,
                finder,
                config,
                prompt,
                label.as_deref(),
                status.as_ref(),
                &mut offset,
            );
        })?;

        match tty::next_event(&signal)? {
            None => break Outcome::Signaled(signal.load(Ordering::Relaxed)),
            Some(Event::Key(key)) if key.kind == KeyEventKind::Press => {
                let confirming = matches!(status, Some(Status::Confirm(_)));

                match handle_key(key, finder, source, page, list, confirming) {
                    Break(outcome) => break outcome,
                    Continue(next_status) => status = next_status,
                }
            }
            // Other events, like a resize, only redraw
            Some(_) => {}
        }
    };

    // Bash redraws the shell prompt on the first row, above the list it moves up there first
    let width = pane.width();
    let moved = match list {
        ListPosition::Below => Ok(()),
        ListPosition::Above => pane.draw(|frame| ui::draw_shell_prompt(frame, prompt)),
    };
    let closed = moved.and(pane.close(ui::shell_prompt(prompt, width).map_or(0, Line::width)));

    // After SIGHUP the terminal may be gone, the signal still decides the exit status
    match outcome {
        Outcome::Signaled(_) => Ok(outcome),
        _ => closed.map(|()| outcome),
    }
}

/// Ends the finder with an outcome, or keeps it open with a status to show. Above the prompt line
/// Up goes to older commands. While Ctrl-D asks about backups, only `y` deletes and any other key
/// cancels without doing what it does.
fn handle_key(
    key: KeyEvent,
    finder: &mut Finder,
    source: &mut Source,
    page: usize,
    list: ListPosition,
    confirming: bool,
) -> ControlFlow<Outcome, Option<Status>> {
    let control = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    let above = list == ListPosition::Above;

    if confirming {
        let yes = key.code == KeyCode::Char(CONFIRM_KEY) && !control && !alt;
        return Continue(if yes { delete(finder, source) } else { None });
    }

    match (key.code, control) {
        (KeyCode::Esc, _) => return Break(Outcome::Quit),
        (KeyCode::Char('c'), true) => return Break(Outcome::Abort),
        (KeyCode::Enter | KeyCode::Tab, _) => {
            let pick = match key.code {
                KeyCode::Enter => Outcome::Picked,
                _ => Outcome::Edited,
            };

            return Break(match finder.selection() {
                Some(command) => pick(command.original.clone()),
                None => Outcome::Quit,
            });
        }
        (KeyCode::Char('d'), true) => {
            return Continue(match source {
                Source::History(_) => delete(finder, source),
                Source::Backups { .. } => confirm(finder, source),
            });
        }
        (KeyCode::Up, _) | (KeyCode::Char('p'), true) if above => finder.down(),
        (KeyCode::Down, _) | (KeyCode::Char('n'), true) if above => finder.up(),
        (KeyCode::PageUp, _) if above => finder.page_down(page),
        (KeyCode::PageDown, _) if above => finder.page_up(page),
        (KeyCode::Up, _) | (KeyCode::Char('p'), true) => finder.up(),
        (KeyCode::Down, _) | (KeyCode::Char('n'), true) => finder.down(),
        (KeyCode::PageUp, _) => finder.page_up(page),
        (KeyCode::PageDown, _) => finder.page_down(page),
        // Option-Left/Right on a Mac send Alt-B/F or Alt-Left/Right, Linux Ctrl-Left/Right
        (KeyCode::Left, _) if control || alt => finder.word_left(),
        (KeyCode::Right, _) if control || alt => finder.word_right(),
        (KeyCode::Char('b'), false) if alt => finder.word_left(),
        (KeyCode::Char('f'), false) if alt => finder.word_right(),
        (KeyCode::Left, _) => finder.left(),
        (KeyCode::Right, _) => finder.right(),
        (KeyCode::Home, _) | (KeyCode::Char('a'), true) => finder.home(),
        (KeyCode::End, _) | (KeyCode::Char('e'), true) => finder.end(),
        (KeyCode::Backspace, _) if alt => finder.delete_word_back(),
        (KeyCode::Backspace, _) => finder.delete_back(),
        (KeyCode::Delete, _) => finder.delete_forward(),
        (KeyCode::Char('u'), true) => finder.delete_to_start(),
        (KeyCode::Char('w'), true) => finder.delete_big_word_back(),
        // Alt and a letter is a shortcut elsewhere, not text for the query
        (KeyCode::Char(c), false) if !alt => finder.push(c),
        _ => {}
    }

    Continue(None)
}

/// Asks before deleting the selected command from backups
fn confirm(finder: &Finder, source: &Source) -> Option<Status> {
    let command = finder.selection()?;
    let backups = match source {
        Source::Backups {
            label: Some(label), ..
        } => format!("backup {label}"),
        _ => "every backup".to_string(),
    };

    Some(Status::Confirm(format!(
        "delete from {backups}? {CONFIRM_KEY}: {}",
        command.text
    )))
}

/// Deletes every occurrence of the selected command from history, or from the backups searched.
/// Deleting from history leaves backups as they are.
fn delete(finder: &mut Finder, source: &mut Source) -> Option<Status> {
    let command = finder.selection()?;
    let message = format!("deleted {}", command.text);

    let result = match source {
        Source::History(history) => {
            let deleted = history
                .delete(&command.entry_indices)
                .context("cannot delete");
            let reloaded = history.reload().context("cannot read history");

            deleted.and(reloaded)
        }
        Source::Backups {
            backups: Some(backups),
            entries,
            started_at,
            ..
        } => {
            let selected: Vec<&Entry> = command
                .entry_indices
                .iter()
                .map(|&index| &entries[index])
                .collect();
            let deleted = backups.delete_entries(&selected, *started_at);
            let reloaded = backups
                .entries(*started_at)
                .map(|reloaded| *entries = reloaded);

            deleted.and(reloaded)
        }
        // Without a database there is nothing to select
        Source::Backups { backups: None, .. } => return None,
    };

    // Keep the selection in place, on the command after the deleted one
    finder.reload(source.entries());

    Some(match result {
        Ok(()) => Status::Deleted(message),
        Err(error) => Status::Failed(error),
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::config::SearchMode;
    use crate::test_util::{TempDir, TempFile};
    use crate::{backup, history};

    /// The finder lists the commands most recent first: echo, pwd, ls
    const HISTORY: &str = "ls\npwd\necho\n";
    /// Rows of the list, for PgUp and PgDn
    const PAGE: usize = 2;

    const NONE: KeyModifiers = KeyModifiers::NONE;
    const CONTROL: KeyModifiers = KeyModifiers::CONTROL;
    const ALT: KeyModifiers = KeyModifiers::ALT;

    /// When the first backup started
    const START: i64 = 1_700_000_000;

    /// The finder on a history file of its own, or on backups
    struct Session {
        file: TempFile,
        source: Source,
        finder: Finder,
        list: ListPosition,
        /// Whether Ctrl-D asks about backups
        confirming: bool,
        /// Holds the backups
        dir: Option<TempDir>,
    }

    impl Session {
        fn open(name: &str) -> Self {
            let file = TempFile::new(&format!("keys-{name}"), HISTORY);
            let history = History::load(&file.path).unwrap();
            let finder = Finder::new(history.entries(), String::new(), SearchMode::Keyword);

            Self {
                file,
                source: Source::History(history),
                finder,
                list: ListPosition::Below,
                confirming: false,
                dir: None,
            }
        }

        /// Two backups, of the history and of pwd alone, searched both or the first
        fn open_backups(name: &str, first: bool) -> Self {
            let mut session = Self::open(name);
            let dir = TempDir::new(&format!("keys-{name}"));
            let mut backups = Backups::open(&dir.path.join("history.db")).unwrap();
            backups
                .save(session.source.entries(), "mac", START)
                .unwrap();
            backups
                .save(&history::parse(b"pwd\n"), "mac", START + 1)
                .unwrap();

            let started_at = first.then_some(START);
            let entries = backups.entries(started_at).unwrap();
            session.finder = Finder::new(&entries, String::new(), SearchMode::Keyword);
            session.source = Source::Backups {
                backups: Some(backups),
                entries,
                label: started_at.map(backup::label),
                started_at,
            };
            session.dir = Some(dir);
            session
        }

        /// The commands of a backup or of all of them, oldest first
        fn backed_up(&self, started_at: Option<i64>) -> Vec<String> {
            let Source::Backups {
                backups: Some(backups),
                ..
            } = &self.source
            else {
                panic!("no backups");
            };

            backups
                .entries(started_at)
                .unwrap()
                .into_iter()
                .map(|entry| entry.original)
                .collect()
        }

        fn press(
            &mut self,
            code: KeyCode,
            modifiers: KeyModifiers,
        ) -> ControlFlow<Outcome, Option<Status>> {
            let flow = handle_key(
                KeyEvent::new(code, modifiers),
                &mut self.finder,
                &mut self.source,
                PAGE,
                self.list,
                self.confirming,
            );
            self.confirming = matches!(flow, Continue(Some(Status::Confirm(_))));
            flow
        }

        /// Presses a key that keeps the finder open without a status
        fn edit(&mut self, code: KeyCode, modifiers: KeyModifiers) {
            assert_eq!(self.press(code, modifiers), Continue(None));
        }
    }

    #[test]
    fn moves_the_selection_with_arrows_and_ctrl_p_n() {
        let mut session = Session::open("move");

        for (code, modifiers, selected) in [
            (KeyCode::Up, NONE, 0),
            (KeyCode::Down, NONE, 1),
            (KeyCode::Char('n'), CONTROL, 2),
            (KeyCode::Down, NONE, 2),
            (KeyCode::Up, NONE, 1),
            (KeyCode::Char('p'), CONTROL, 0),
        ] {
            session.edit(code, modifiers);
            assert_eq!(session.finder.selected(), selected);
        }
    }

    #[test]
    fn moves_the_selection_by_a_page_with_page_keys() {
        let mut session = Session::open("page");

        for (code, selected) in [
            (KeyCode::PageDown, 2),
            (KeyCode::PageDown, 2),
            (KeyCode::Up, 1),
            (KeyCode::PageUp, 0),
        ] {
            session.edit(code, NONE);
            assert_eq!(session.finder.selected(), selected);
        }
    }

    #[test]
    fn moves_the_selection_up_the_screen_to_older_commands_above_the_prompt_line() {
        let mut session = Session::open("move-above");
        session.list = ListPosition::Above;

        for (code, modifiers, selected) in [
            (KeyCode::Down, NONE, 0),
            (KeyCode::Up, NONE, 1),
            (KeyCode::Char('p'), CONTROL, 2),
            (KeyCode::Up, NONE, 2),
            (KeyCode::Down, NONE, 1),
            (KeyCode::Char('n'), CONTROL, 0),
            (KeyCode::PageUp, NONE, 2),
            (KeyCode::PageDown, NONE, 0),
        ] {
            session.edit(code, modifiers);
            assert_eq!(session.finder.selected(), selected);
        }
    }

    #[test]
    fn edits_the_query() {
        let mut session = Session::open("edit");

        session.edit(KeyCode::Char('e'), NONE);
        session.edit(KeyCode::Char('C'), KeyModifiers::SHIFT);
        assert_eq!(session.finder.query(), "eC");

        session.edit(KeyCode::Backspace, NONE);
        assert_eq!(session.finder.query(), "e");

        session.edit(KeyCode::Char('u'), CONTROL);
        assert_eq!(session.finder.query(), "");
    }

    #[test]
    fn moves_the_cursor_and_edits_at_it() {
        let mut session = Session::open("cursor");

        for c in "gt lg".chars() {
            session.edit(KeyCode::Char(c), NONE);
        }

        for (code, modifiers, query, cursor) in [
            (KeyCode::Left, NONE, "gt lg", 4),
            (KeyCode::Char('o'), NONE, "gt log", 5),
            (KeyCode::Home, NONE, "gt log", 0),
            (KeyCode::Right, NONE, "gt log", 1),
            (KeyCode::Char('i'), NONE, "git log", 2),
            (KeyCode::End, NONE, "git log", 7),
            (KeyCode::Char('a'), CONTROL, "git log", 0),
            (KeyCode::Delete, NONE, "it log", 0),
            (KeyCode::Char('e'), CONTROL, "it log", 6),
            (KeyCode::Left, NONE, "it log", 5),
            (KeyCode::Left, NONE, "it log", 4),
            (KeyCode::Char('u'), CONTROL, "og", 0),
        ] {
            session.edit(code, modifiers);
            assert_eq!(
                (session.finder.query(), session.finder.cursor()),
                (query, cursor)
            );
        }
    }

    #[test]
    fn moves_the_cursor_and_deletes_by_words() {
        let mut session = Session::open("words");

        for c in "cd ~/src/utils".chars() {
            session.edit(KeyCode::Char(c), NONE);
        }

        for (code, modifiers, query, cursor) in [
            (KeyCode::Char('b'), ALT, "cd ~/src/utils", 9),
            (KeyCode::Left, ALT, "cd ~/src/utils", 5),
            (KeyCode::Left, CONTROL, "cd ~/src/utils", 0),
            (KeyCode::Char('f'), ALT, "cd ~/src/utils", 2),
            (KeyCode::Right, ALT, "cd ~/src/utils", 8),
            (KeyCode::Right, CONTROL, "cd ~/src/utils", 14),
            (KeyCode::Backspace, ALT, "cd ~/src/", 9),
            (KeyCode::Char('w'), CONTROL, "cd ", 3),
        ] {
            session.edit(code, modifiers);
            assert_eq!(
                (session.finder.query(), session.finder.cursor()),
                (query, cursor)
            );
        }
    }

    #[test]
    fn leaves_alt_and_control_letters_out_of_the_query() {
        let mut session = Session::open("alt");

        session.edit(KeyCode::Char('x'), ALT);
        session.edit(KeyCode::Char('x'), CONTROL);

        assert_eq!(session.finder.query(), "");
    }

    #[test]
    fn picks_the_selection_with_enter_to_run_and_tab_to_edit() {
        let mut session = Session::open("pick");

        assert_eq!(
            session.press(KeyCode::Enter, NONE),
            Break(Outcome::Picked("echo".to_string()))
        );

        session.edit(KeyCode::Down, NONE);

        assert_eq!(
            session.press(KeyCode::Tab, NONE),
            Break(Outcome::Edited("pwd".to_string()))
        );
    }

    #[test]
    fn quits_on_enter_and_tab_without_matches() {
        let mut session = Session::open("no-matches");
        session.edit(KeyCode::Char('z'), NONE);

        assert_eq!(session.press(KeyCode::Enter, NONE), Break(Outcome::Quit));
        assert_eq!(session.press(KeyCode::Tab, NONE), Break(Outcome::Quit));
    }

    #[test]
    fn quits_on_esc_and_aborts_on_ctrl_c() {
        let mut session = Session::open("quit");

        assert_eq!(session.press(KeyCode::Esc, NONE), Break(Outcome::Quit));
        assert_eq!(
            session.press(KeyCode::Char('c'), CONTROL),
            Break(Outcome::Abort)
        );
    }

    #[test]
    fn deletes_the_selection_and_selects_the_next_command() {
        let mut session = Session::open("delete");
        session.edit(KeyCode::Down, NONE);

        assert_eq!(
            session.press(KeyCode::Char('d'), CONTROL),
            Continue(Some(Status::Deleted("deleted pwd".to_string())))
        );
        assert_eq!(session.file.read(), "ls\necho\n");
        assert_eq!(session.finder.selection().unwrap().text, "ls");
    }

    #[test]
    fn shows_the_error_when_another_shell_changed_the_history() {
        let mut session = Session::open("delete-changed");
        fs::write(&session.file.path, "date\n").unwrap();

        assert_eq!(
            session.press(KeyCode::Char('d'), CONTROL),
            Continue(Some(Status::Failed(
                "cannot delete: history file was changed by another process, try again".to_string()
            )))
        );
        assert_eq!(session.file.read(), "date\n");
        // Reloaded all the same, so the finder shows what the file holds
        assert_eq!(session.finder.selection().unwrap().text, "date");
    }

    #[test]
    fn deletes_from_every_backup_after_y() {
        let mut session = Session::open_backups("delete-backups", false);
        session.edit(KeyCode::Down, NONE);

        assert_eq!(
            session.press(KeyCode::Char('d'), CONTROL),
            Continue(Some(Status::Confirm(
                "delete from every backup? y: pwd".to_string()
            )))
        );
        assert_eq!(session.backed_up(None), ["ls", "pwd", "echo"]);

        assert_eq!(
            session.press(KeyCode::Char(CONFIRM_KEY), NONE),
            Continue(Some(Status::Deleted("deleted pwd".to_string())))
        );
        assert_eq!(session.backed_up(None), ["ls", "echo"]);
        assert_eq!(session.backed_up(Some(START + 1)), Vec::<String>::new());
        assert_eq!(session.finder.selection().unwrap().text, "ls");
        assert_eq!(session.file.read(), HISTORY);
    }

    #[test]
    fn deletes_only_from_the_backup_searched() {
        let mut session = Session::open_backups("delete-backup", true);
        session.edit(KeyCode::Down, NONE);

        assert_eq!(
            session.press(KeyCode::Char('d'), CONTROL),
            Continue(Some(Status::Confirm(format!(
                "delete from backup {}? y: pwd",
                backup::label(START)
            ))))
        );
        assert_eq!(
            session.press(KeyCode::Char(CONFIRM_KEY), NONE),
            Continue(Some(Status::Deleted("deleted pwd".to_string())))
        );
        assert_eq!(session.backed_up(Some(START)), ["ls", "echo"]);
        assert_eq!(session.backed_up(Some(START + 1)), ["pwd"]);
        assert_eq!(session.finder.selection().unwrap().text, "ls");
    }

    #[test]
    fn cancels_a_delete_from_backups_on_any_other_key() {
        let mut session = Session::open_backups("delete-backup-cancel", false);

        for (code, modifiers) in [
            (KeyCode::Esc, NONE),
            (KeyCode::Char('c'), CONTROL),
            (KeyCode::Char('d'), CONTROL),
            (KeyCode::Char(CONFIRM_KEY), CONTROL),
            (KeyCode::Char('Y'), KeyModifiers::SHIFT),
            (KeyCode::Char('n'), NONE),
            (KeyCode::Enter, NONE),
            (KeyCode::Down, NONE),
        ] {
            assert!(matches!(
                session.press(KeyCode::Char('d'), CONTROL),
                Continue(Some(Status::Confirm(_)))
            ));
            session.edit(code, modifiers);

            assert_eq!(session.finder.query(), "", "{code:?}");
            assert_eq!(session.finder.selected(), 0, "{code:?}");
        }

        assert_eq!(session.backed_up(None), ["ls", "pwd", "echo"]);
        // Asks again after a cancel
        assert!(matches!(
            session.press(KeyCode::Char('d'), CONTROL),
            Continue(Some(Status::Confirm(_)))
        ));
    }

    #[test]
    fn asks_nothing_without_matches_in_backups() {
        let mut session = Session::open_backups("delete-backup-none", false);
        session.edit(KeyCode::Char('z'), NONE);
        session.edit(KeyCode::Char('d'), CONTROL);
        session.edit(KeyCode::Char(CONFIRM_KEY), NONE);

        assert_eq!(session.finder.query(), format!("z{CONFIRM_KEY}"));
        assert_eq!(session.backed_up(None), ["ls", "pwd", "echo"]);
    }

    #[test]
    fn deletes_nothing_without_matches() {
        let mut session = Session::open("delete-none");
        session.edit(KeyCode::Char('z'), NONE);
        session.edit(KeyCode::Char('d'), CONTROL);

        assert_eq!(session.file.read(), HISTORY);
    }
}
