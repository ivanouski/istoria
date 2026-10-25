//! The part of the terminal from the cursor's line down that the finder draws in

use std::fs::{File, OpenOptions};
use std::io::{self, BufWriter, Write};

use ratatui::backend::{Backend, ClearType, CrosstermBackend};
use ratatui::crossterm::terminal::{BeginSynchronizedUpdate, EndSynchronizedUpdate};
use ratatui::crossterm::{cursor, execute, queue, terminal};
use ratatui::layout::Position;
use ratatui::{Frame, Terminal, TerminalOptions, Viewport};

use crate::tty::{RawMode, StdoutToTty};

/// Fits a whole frame, so it's written at once
const FRAME_BUFFER: usize = 1 << 16;

/// Drawn on /dev/tty, so it works with stdout captured
pub struct Pane {
    terminal: Terminal<CrosstermBackend<BufWriter<File>>>,
    /// The cursor's when opened, on text to go back to unless 0
    column: u16,
    /// Fields drop in order, so these are restored after the terminal is gone
    _raw_mode: RawMode,
    _stdout: StdoutToTty,
}

impl Pane {
    /// At most the terminal's height
    pub fn open(height: u16) -> io::Result<Self> {
        let mut tty = OpenOptions::new().read(true).write(true).open("/dev/tty")?;
        let stdout = StdoutToTty::new(&tty)?;
        let raw_mode = RawMode::enable()?;

        // Bash clears the prompt line for bind -x, if text is still there the pane opens below it
        let (column, _) = cursor::position()?;

        if column > 0 {
            tty.write_all(b"\r\n")?;
        }

        let height = terminal::size()?.1.min(height);
        let terminal = Terminal::with_options(
            CrosstermBackend::new(BufWriter::with_capacity(FRAME_BUFFER, tty)),
            TerminalOptions {
                viewport: Viewport::Inline(height),
            },
        )?;

        Ok(Self {
            terminal,
            column,
            _raw_mode: raw_mode,
            _stdout: stdout,
        })
    }

    /// In a synchronized update, ended even when drawing fails
    pub fn draw(&mut self, render: impl FnOnce(&mut Frame)) -> io::Result<()> {
        queue!(self.terminal.backend_mut(), BeginSynchronizedUpdate)?;
        let drawn = self.terminal.draw(render).map(|_| ());
        let ended = execute!(self.terminal.backend_mut(), EndSynchronizedUpdate);
        drawn.and(ended)
    }

    pub fn width(&mut self) -> u16 {
        self.terminal.get_frame().area().width
    }

    /// Clears the pane but the shell prompt, which bash redraws in place, so the line doesn't flash
    /// empty
    pub fn close(mut self, kept: usize) -> io::Result<()> {
        let area = self.terminal.get_frame().area();
        // Opened under some text, the cursor goes back to it and the pane has no prompt to keep
        let (kept, cursor) = if self.column > 0 {
            (0, Position::new(self.column, area.y.saturating_sub(1)))
        } else {
            (
                u16::try_from(kept).unwrap_or(u16::MAX).min(area.width),
                area.as_position(),
            )
        };
        let backend = self.terminal.backend_mut();
        backend.set_cursor_position(Position::new(kept, area.y))?;
        backend.clear_region(ClearType::AfterCursor)?;
        backend.set_cursor_position(cursor)?;

        self.terminal.show_cursor()
    }
}
