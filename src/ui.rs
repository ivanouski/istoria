//! Drawing the finder: the prompt line, the divider, the list and the preview

use std::iter;
use std::ops::Range;

use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListDirection, ListItem, ListState, Paragraph};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::config::{self, Config, ListPosition, Theme};
use crate::finder::{Command, Finder, Match, TIME_WIDTH};

/// Before rows that don't line up under the query
const INDENT: usize = 2;
const TIME_COLUMN: usize = TIME_WIDTH + 2;
/// Most columns of the hosts at the right edge, before the gap after the command
const MAX_HOSTS_WIDTH: usize = 20;
/// Before the hosts and the time at the right edge
const EDGE_GAP: usize = 2;
/// Left for the command next to the hosts or the time at the right edge, else they aren't shown
const MIN_COMMAND_WIDTH: usize = 20;
const ELLIPSIS: &str = "…";
const NO_MATCHES: &str = "no matches";
const NO_COMMANDS: &str = "no commands";
/// Most lines of the preview
const PREVIEW_LINES: usize = 3;
/// Left for the list under the preview
const MIN_LIST_ROWS: usize = 3;
const DASH: &str = "─";
const COUNT_END: &str = " ──";

/// The outcome of the last Ctrl-D, or its question, shown in place of the count until the next key
#[derive(Debug, PartialEq)]
pub enum Status {
    Deleted(String),
    Failed(String),
    /// Before deleting from backups
    Confirm(String),
}

/// Draws the pane, returns the list's rows for PgUp and PgDn
pub fn draw(
    frame: &mut Frame,
    finder: &Finder,
    config: &Config,
    prompt: Option<&Line>,
    label: Option<&str>,
    status: Option<&Status>,
    offset: &mut usize,
) -> usize {
    let (theme, list) = (&config.colors, config.list);
    let area = frame.area();
    let shell_prompt = shell_prompt(prompt, area.width);
    let prompt_line = prompt_line(shell_prompt, label, theme);
    // Under the query after a shell prompt, which a label moves along
    let layout = RowLayout::new(
        finder,
        config,
        prompt,
        shell_prompt.map(|_| prompt_line.width()),
        area.width,
    );
    let [prompt_area, divider_area, body] = split(area, list);
    let (body, border_area) = split_border(body, config.border, list);
    let preview = config
        .preview
        .then(|| preview(finder, &layout, body, theme, list))
        .flatten()
        .unwrap_or_default();
    let (list_area, preview_area) = split_preview(body, preview.len(), list);

    draw_prompt_line(frame, prompt_area, prompt_line, finder);
    draw_divider(frame, divider_area, finder, theme, status);
    let height = draw_list(frame, list_area, finder, &layout, theme, list, offset);
    frame.render_widget(Paragraph::new(preview), preview_area);
    draw_border(frame, border_area, theme);

    height
}

/// The prompt line, the divider and the body, from the bottom up above the list
fn split(area: Rect, list: ListPosition) -> [Rect; 3] {
    let row = Constraint::Length(1);

    match list {
        ListPosition::Below => Layout::vertical([row, row, Constraint::Fill(1)]).areas(area),
        ListPosition::Above => {
            // Split one row at a time, rows given together go to the first when short
            let [upper, prompt] = Layout::vertical([Constraint::Fill(1), row]).areas(area);
            let [rest, divider] = Layout::vertical([Constraint::Fill(1), row]).areas(upper);
            [prompt, divider, rest]
        }
    }
}

/// The border on the body's far edge, when it's on and leaves a row for the list
fn split_border(body: Rect, border: bool, list: ListPosition) -> (Rect, Option<Rect>) {
    if !border || body.height < 2 {
        return (body, None);
    }

    let row = Constraint::Length(1);

    match list {
        ListPosition::Below => {
            let [rest, edge] = Layout::vertical([Constraint::Fill(1), row]).areas(body);
            (rest, Some(edge))
        }
        ListPosition::Above => {
            let [edge, rest] = Layout::vertical([row, Constraint::Fill(1)]).areas(body);
            (rest, Some(edge))
        }
    }
}

/// The list and the preview past it, as tall as its lines
fn split_preview(body: Rect, lines: usize, list: ListPosition) -> (Rect, Rect) {
    let rest = Constraint::Fill(1);
    let preview = Constraint::Length(saturating_u16(lines));

    match list {
        ListPosition::Below => {
            let [list, preview] = Layout::vertical([rest, preview]).areas(body);
            (list, preview)
        }
        ListPosition::Above => {
            let [preview, list] = Layout::vertical([preview, rest]).areas(body);
            (list, preview)
        }
    }
}

/// Draws the matches in view, returns the list's rows. Above the prompt line the list goes up.
fn draw_list(
    frame: &mut Frame,
    area: Rect,
    finder: &Finder,
    layout: &RowLayout,
    theme: &Theme,
    list: ListPosition,
    offset: &mut usize,
) -> usize {
    // Only the rows in view are built, the list scrolls like ratatui's would with all of them
    let height = usize::from(area.height);
    *offset = list_offset(*offset, finder, height);

    if finder.matches().is_empty() {
        // On the row next to the divider, where the first match would be
        let [row] = match list {
            ListPosition::Below => Layout::vertical([Constraint::Length(1)]).areas(area),
            ListPosition::Above => Layout::vertical([Constraint::Length(1)])
                .flex(Flex::End)
                .areas(area),
        };
        frame.render_widget(Paragraph::new(nothing_found(finder, layout, theme)), row);
        return height;
    }

    let rows = finder
        .matches()
        .iter()
        .enumerate()
        .skip(*offset)
        .take(height)
        .map(|(index, found)| row(finder, found, index == finder.selected(), layout, theme));

    let direction = match list {
        ListPosition::Below => ListDirection::TopToBottom,
        ListPosition::Above => ListDirection::BottomToTop,
    };

    frame.render_stateful_widget(
        List::new(rows).direction(direction),
        area,
        &mut ListState::default().with_selected(Some(finder.selected() - *offset)),
    );

    height
}

/// The first match in view, scrolled just enough to keep the selection in it
fn list_offset(offset: usize, finder: &Finder, height: usize) -> usize {
    let offset = offset.min(finder.matches().len().saturating_sub(1));

    if finder.selected() < offset {
        finder.selected()
    } else if finder.selected() >= offset + height {
        (finder.selected() + 1).saturating_sub(height.max(1))
    } else {
        offset
    }
}

/// The shell prompt when it shows anything and leaves the query room, None for ›
pub fn shell_prompt<'a>(prompt: Option<&'a Line<'a>>, width: u16) -> Option<&'a Line<'a>> {
    prompt.filter(|prompt| prompt.width() > 0 && !too_wide(prompt, width))
}

/// Wider than 60% of the pane
fn too_wide(prompt: &Line, width: u16) -> bool {
    prompt.width() * 5 > usize::from(width) * 3
}

/// Only the shell prompt, on the first row, for the pane to keep when it closes
pub fn draw_shell_prompt(frame: &mut Frame, prompt: Option<&Line>) {
    if let Some(prompt) = shell_prompt(prompt, frame.area().width) {
        frame.render_widget(Paragraph::new(prompt.clone()), frame.area());
    }
}

/// The shell prompt, then the label and ›. Without either, › alone.
fn prompt_line<'a>(
    shell_prompt: Option<&Line<'a>>,
    label: Option<&str>,
    theme: &Theme,
) -> Line<'a> {
    let mut line = shell_prompt.cloned().unwrap_or_default();

    if let Some(label) = label {
        line.push_span(Span::styled(format!("{label}› "), theme.backup));
    } else if shell_prompt.is_none() {
        line.push_span(Span::styled("› ", theme.prompt));
    }

    line
}

/// The prompt and the query after it
fn draw_prompt_line(frame: &mut Frame, area: Rect, mut line: Line, finder: &Finder) {
    let prompt_width = line.width();
    let room = usize::from(area.width).saturating_sub(prompt_width);
    let (query, column) = scroll(finder.query(), finder.cursor(), room);
    line.push_span(Span::raw(query));

    frame.set_cursor_position((area.x + saturating_u16(prompt_width + column), area.y));
    frame.render_widget(Paragraph::new(line), area);
}

/// Dashes with the count near the right end, only dashes when it doesn't fit
fn draw_divider(
    frame: &mut Frame,
    area: Rect,
    finder: &Finder,
    theme: &Theme,
    status: Option<&Status>,
) {
    let width = usize::from(area.width);
    let room = width.saturating_sub(" ".width() + COUNT_END.width());
    let (text, style) = count(finder, theme, status);
    // A cut count would read as other numbers, a cut status still tells what happened
    let text = match status {
        Some(_) => cut(&text, room),
        None if text.width() <= room => text,
        None => String::new(),
    };

    let line = if text.is_empty() {
        dashes(width, theme)
    } else {
        let before = DASH.repeat(room.saturating_sub(text.width()));

        Line::from(vec![
            Span::styled(format!("{before} "), theme.dim),
            Span::styled(text, style),
            Span::styled(COUNT_END, theme.dim),
        ])
    };

    frame.render_widget(Paragraph::new(line), area);
}

fn draw_border(frame: &mut Frame, area: Option<Rect>, theme: &Theme) {
    if let Some(area) = area {
        frame.render_widget(Paragraph::new(dashes(usize::from(area.width), theme)), area);
    }
}

fn dashes(width: usize, theme: &Theme) -> Line<'static> {
    Line::styled(DASH.repeat(width), theme.dim)
}

/// The query in room columns and the cursor's column, cut from the start to keep the cursor in view
fn scroll(query: &str, cursor: usize, room: usize) -> (String, usize) {
    let column = query[..cursor].width();

    if column < room {
        return (query.to_string(), column);
    }

    let start = suffix_start(&query[..cursor], room.saturating_sub(2));

    (
        format!("{ELLIPSIS}{}", &query[start..]),
        ELLIPSIS.width() + query[start..cursor].width(),
    )
}

/// The text in width columns, its end cut with … in its place when it needs more
fn cut(text: &str, width: usize) -> String {
    if text.width() <= width {
        text.to_string()
    } else if width == 0 {
        String::new()
    } else {
        format!("{}{ELLIPSIS}", &text[..prefix_end(text, 0, width - 1)])
    }
}

/// Matches and commands, or the status in their place until the next key
fn count(finder: &Finder, theme: &Theme, status: Option<&Status>) -> (String, Style) {
    match status {
        Some(Status::Deleted(message)) => (message.clone(), theme.deleted),
        Some(Status::Failed(message) | Status::Confirm(message)) => (message.clone(), theme.error),
        None => {
            let text = format!("{}/{}", finder.matches().len(), finder.commands().len());
            let style = if finder.matches().is_empty() {
                theme.error
            } else {
                theme.count
            };

            (text, style)
        }
    }
}

/// In place of the list when nothing matches, where the commands would start
fn nothing_found(finder: &Finder, layout: &RowLayout, theme: &Theme) -> Line<'static> {
    let text = if finder.commands().is_empty() {
        NO_COMMANDS
    } else {
        NO_MATCHES
    };

    Line::from(vec![
        Span::raw(" ".repeat(layout.indent.len() + layout.column_width())),
        Span::styled(text, theme.dim),
    ])
}

/// What goes before the commands
#[derive(Clone, Copy, Debug, PartialEq)]
enum Column {
    Times,
    Weights,
}

/// What every row starts with
struct RowLayout {
    column: Option<Column>,
    /// After the commands run more than once, in the recent layout
    runs: bool,
    /// Before the column, or before the command without it
    indent: String,
    /// Left for the command and its number of runs
    command_width: usize,
    /// Of the hosts at the right edge, 0 without them
    hosts_width: usize,
    /// The time at the right edge, after the hosts, in the weights layout
    edge_time: bool,
    /// A step of the weight bars
    bar: char,
    pane_width: usize,
}

impl RowLayout {
    /// The time at the right edge, then the hosts up to their widest, when that leaves the command
    /// room
    fn new(
        finder: &Finder,
        config: &Config,
        prompt: Option<&Line>,
        prompt_width: Option<usize>,
        pane_width: u16,
    ) -> Self {
        let column = column(finder, config, prompt, pane_width);
        // Without the bars, the runs still tell why the weights put a command first
        let runs = finder.layout() == config::Layout::Recent || column.is_none();
        let column_width = column_width(column);
        let command_column = command_column(prompt_width, column_width);
        let pane_width = usize::from(pane_width);
        let mut room = pane_width.saturating_sub(command_column);
        let mut edge = |width: usize| {
            let fits = width > 0 && room >= MIN_COMMAND_WIDTH + EDGE_GAP + width;
            if fits {
                room -= EDGE_GAP + width;
            }

            fits
        };
        let edge_time = finder.layout() == config::Layout::Weights
            && config.times
            && finder.has_times()
            && edge(TIME_WIDTH);
        let hosts_width = finder.hosts_width().min(MAX_HOSTS_WIDTH);
        let hosts_width = if edge(hosts_width) { hosts_width } else { 0 };

        Self {
            column,
            runs,
            indent: " ".repeat(command_column - column_width),
            command_width: room,
            hosts_width,
            edge_time,
            bar: config.bar,
            pane_width,
        }
    }

    fn column_width(&self) -> usize {
        column_width(self.column)
    }

    fn runs(&self, command: &Command) -> Option<String> {
        self.runs.then(|| runs(command)).flatten()
    }

    /// The part of a command its row shows, keeping the matches in view
    fn fit(&self, command: &Command, ranges: &[Range<usize>]) -> Fitted {
        let runs_width = self.runs(command).map_or(0, |runs| runs.width());

        fit(
            &command.text,
            &around(ranges),
            self.command_width.saturating_sub(runs_width),
        )
    }
}

/// The times or the weights. A pane too narrow for the prompt is too narrow for them as well.
fn column(
    finder: &Finder,
    config: &Config,
    prompt: Option<&Line>,
    pane_width: u16,
) -> Option<Column> {
    let column = match finder.layout() {
        config::Layout::Recent => (config.times && finder.has_times()).then_some(Column::Times),
        config::Layout::Weights => Some(Column::Weights),
    };

    column.filter(|_| !prompt.is_some_and(|prompt| too_wide(prompt, pane_width)))
}

/// Of the times or the weights and the gap after them, 0 without them
fn column_width(column: Option<Column>) -> usize {
    if column.is_some() { TIME_COLUMN } else { 0 }
}

/// Under the query, the times before it, unless the prompt is missing or too narrow for them
fn command_column(prompt_width: Option<usize>, time_width: usize) -> usize {
    prompt_width
        .filter(|&width| width >= time_width)
        .unwrap_or(INDENT + time_width)
}

fn row<'a>(
    finder: &'a Finder,
    found: &Match,
    selected: bool,
    layout: &'a RowLayout,
    theme: &Theme,
) -> ListItem<'a> {
    let command = &finder.commands()[found.command_index];
    // Not dimmed on the selected row, dim is hard to read on its background
    let dim = if selected { Style::new() } else { theme.dim };
    let fitted = layout.fit(command, &found.ranges);
    let mut spans = vec![Span::raw(layout.indent.as_str())];

    match layout.column {
        Some(Column::Times) => {
            let time = command.time.as_deref().unwrap_or_default();
            spans.push(Span::styled(format!("{time:<TIME_WIDTH$}  "), dim));
        }
        Some(Column::Weights) => {
            let steps = weight_steps(finder.weight_share(command));
            let bar: String = iter::repeat_n(layout.bar, steps).collect();
            spans.push(Span::styled(format!("{bar:>TIME_WIDTH$}  "), dim));
        }
        None => {}
    }

    if fitted.cut_start {
        spans.push(Span::styled(ELLIPSIS, dim));
    }

    spans.extend(highlight(
        &command.text,
        &found.ranges,
        fitted.shown,
        theme.matched,
    ));

    if fitted.cut_end {
        spans.push(Span::styled(ELLIPSIS, dim));
    }

    if let Some(runs) = layout.runs(command) {
        spans.push(Span::styled(runs, dim));
    }

    let mut edge = Vec::new();

    if layout.hosts_width > 0 {
        edge.push(cut(&command.hosts, layout.hosts_width));
    }

    if layout.edge_time {
        let time = command.time.as_deref().unwrap_or_default();
        edge.push(format!("{time:<TIME_WIDTH$}"));
    }

    if !edge.is_empty() {
        // At the right edge, the command fits before the gap
        let edge = edge.join(&" ".repeat(EDGE_GAP));
        let used: usize = spans.iter().map(Span::width).sum();
        let padding = layout.pane_width.saturating_sub(used + edge.width());
        spans.push(Span::raw(" ".repeat(padding)));
        spans.push(Span::styled(edge, dim));
    }

    // Styled per item, a list highlight style would cover the match
    let item = ListItem::new(Line::from(spans));

    if selected {
        item.style(theme.selected)
    } else {
        item
    }
}

/// The squares of a weight bar for its share of the heaviest weight, rounded, at least one
fn weight_steps(share: f64) -> usize {
    let width = u16::try_from(TIME_WIDTH).unwrap_or(u16::MAX);
    let scaled = share * f64::from(width);

    1 + (1..width)
        .filter(|&step| scaled >= f64::from(step) + 0.5)
        .count()
}

/// After a command run more than once
fn runs(command: &Command) -> Option<String> {
    let runs = command.entry_indices.len();

    (runs > 1).then(|| format!(" ({runs})"))
}

/// The selected command in full past the list, when its row is cut and there's room
fn preview<'a>(
    finder: &'a Finder,
    layout: &RowLayout,
    area: Rect,
    theme: &Theme,
    list: ListPosition,
) -> Option<Vec<Line<'a>>> {
    let found = finder.matches().get(finder.selected())?;
    let command = &finder.commands()[found.command_index];

    if !layout.fit(command, &found.ranges).is_cut() {
        return None;
    }

    let width = usize::from(area.width);
    let mut lines = wrap(&command.text, width);
    let cut = lines.len() > PREVIEW_LINES;
    lines.truncate(PREVIEW_LINES);

    if usize::from(area.height) < MIN_LIST_ROWS + 1 + lines.len() {
        return None;
    }

    if cut && let Some(last) = lines.last_mut() {
        last.end = prefix_end(&command.text, last.start, width.saturating_sub(1));
    }

    let divider = dashes(width, theme);
    let mut preview: Vec<Line> = lines
        .into_iter()
        .map(|shown| {
            Line::from(highlight(
                &command.text,
                &found.ranges,
                shown,
                theme.matched,
            ))
        })
        .collect();

    if cut && let Some(last) = preview.last_mut() {
        last.push_span(Span::styled(ELLIPSIS, theme.dim));
    }

    match list {
        ListPosition::Below => preview.insert(0, divider),
        ListPosition::Above => preview.push(divider),
    }

    Some(preview)
}

/// The shown part of text, its matches highlighted. The ranges are sorted and apart.
fn highlight<'a>(
    text: &'a str,
    ranges: &[Range<usize>],
    shown: Range<usize>,
    matched: Style,
) -> Vec<Span<'a>> {
    let clamp = |index: usize| index.clamp(shown.start, shown.end);
    let mut spans = Vec::new();
    let mut position = shown.start;

    for range in ranges {
        let (start, end) = (clamp(range.start), clamp(range.end));

        if start < end {
            spans.push(Span::raw(&text[position..start]));
            spans.push(Span::styled(&text[start..end], matched));
            position = end;
        }
    }

    spans.push(Span::raw(&text[position..shown.end]));
    spans
}

/// From the start of the first match to the end of the last, empty without matches
fn around(ranges: &[Range<usize>]) -> Range<usize> {
    match (ranges.first(), ranges.last()) {
        (Some(first), Some(last)) => first.start..last.end,
        _ => 0..0,
    }
}

/// The part of a command shown in a row, and whether an ellipsis goes before or after it
#[derive(Debug, PartialEq)]
struct Fitted {
    shown: Range<usize>,
    cut_start: bool,
    cut_end: bool,
}

impl Fitted {
    fn is_cut(&self) -> bool {
        self.cut_start || self.cut_end
    }
}

/// Fits text into width columns, keeping the match in view: the start, else the end, else the
/// match a third of the way in
fn fit(text: &str, range: &Range<usize>, width: usize) -> Fitted {
    let fitted = |shown, cut_start, cut_end| Fitted {
        shown,
        cut_start,
        cut_end,
    };

    if text.width() <= width {
        return fitted(0..text.len(), false, false);
    }

    let room = width.saturating_sub(1);

    if text[..range.end].width() <= room {
        return fitted(0..prefix_end(text, 0, room), false, true);
    }

    let end_start = suffix_start(text, room);

    if end_start <= range.start {
        return fitted(end_start..text.len(), true, false);
    }

    let room = width.saturating_sub(2);
    let start = suffix_start(&text[..range.start], room / 3);

    fitted(start..prefix_end(text, start, room), true, true)
}

/// Lines of text wrapped at width columns, after spaces where possible. A space ending a full line
/// stays on it.
fn wrap(text: &str, width: usize) -> Vec<Range<usize>> {
    let mut lines = Vec::new();
    let mut start = 0;

    while start < text.len() {
        let end = prefix_end(text, start, width);
        let end = if end == text.len() {
            end
        } else if text[end..].starts_with(' ') {
            end + 1
        } else if let Some(space) = text[start..end].rfind(' ') {
            start + space + 1
        } else if end == start {
            // A character wider than the line
            start + text[start..].chars().next().map_or(1, char::len_utf8)
        } else {
            end
        };

        lines.push(start..end);
        start = end;
    }

    lines
}

/// Where text from start ends when it may take width columns
fn prefix_end(text: &str, start: usize, width: usize) -> usize {
    let mut used = 0;

    for (index, c) in text[start..].char_indices() {
        used += c.width().unwrap_or(0);

        if used > width {
            return start + index;
        }
    }

    text.len()
}

/// Columns or rows as ratatui takes them, capped at `u16::MAX`
fn saturating_u16(value: usize) -> u16 {
    u16::try_from(value).unwrap_or(u16::MAX)
}

/// Where the end of text starts when it may take width columns
fn suffix_start(text: &str, width: usize) -> usize {
    let mut used = 0;

    for (index, c) in text.char_indices().rev() {
        used += c.width().unwrap_or(0);

        if used > width {
            return index + c.len_utf8();
        }
    }

    0
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;
    use ratatui::layout::Position;
    use ratatui::style::{Color, Modifier};

    use super::*;
    use crate::history::{self, Entry};
    use crate::{prompt, test_util};

    const DIVIDER_ROW: u16 = 1;
    const FIRST_ROW: u16 = 2;

    fn finder(content: &str) -> Finder {
        test_util::finder(&history::parse(content.as_bytes()), "ls")
    }

    /// The cursor at the end of the query, or at its start for home
    fn query_finder(query: &str, home: bool) -> Finder {
        let mut finder = test_util::finder(&history::parse(b"ls\n"), query);
        if home {
            finder.home();
        }

        finder
    }

    /// Draws on a 40 by 4 terminal with the test config unless set otherwise
    struct Render<'a> {
        finder: &'a Finder,
        config: Config,
        prompt: Option<&'a str>,
        label: Option<&'a str>,
        status: Option<Status>,
        width: u16,
        height: u16,
        offset: usize,
    }

    struct Rendered {
        buffer: Buffer,
        cursor: Position,
        page: usize,
        offset: usize,
    }

    impl<'a> Render<'a> {
        fn new(finder: &'a Finder) -> Self {
            Self {
                finder,
                config: test_util::config(),
                prompt: None,
                label: None,
                status: None,
                width: 40,
                height: 4,
                offset: 0,
            }
        }

        fn theme(mut self, theme: Theme) -> Self {
            self.config.colors = theme;
            self
        }

        fn prompt(self, prompt: &'a str) -> Self {
            Self {
                prompt: Some(prompt),
                ..self
            }
        }

        fn label(self, label: &'a str) -> Self {
            Self {
                label: Some(label),
                ..self
            }
        }

        fn status(self, status: Status) -> Self {
            Self {
                status: Some(status),
                ..self
            }
        }

        fn above(mut self) -> Self {
            self.config.list = ListPosition::Above;
            self
        }

        fn border(mut self) -> Self {
            self.config.border = true;
            self
        }

        fn preview(mut self) -> Self {
            self.config.preview = true;
            self
        }

        fn no_times(mut self) -> Self {
            self.config.times = false;
            self
        }

        fn bar(mut self, bar: char) -> Self {
            self.config.bar = bar;
            self
        }

        fn width(self, width: u16) -> Self {
            Self { width, ..self }
        }

        fn height(self, height: u16) -> Self {
            Self { height, ..self }
        }

        fn offset(self, offset: usize) -> Self {
            Self { offset, ..self }
        }

        fn run(self) -> Rendered {
            let prompt = self.prompt.map(prompt::parse);
            let mut terminal = Terminal::new(TestBackend::new(self.width, self.height)).unwrap();
            let mut offset = self.offset;
            let mut page = 0;
            terminal
                .draw(|frame| {
                    page = draw(
                        frame,
                        self.finder,
                        &self.config,
                        prompt.as_ref(),
                        self.label,
                        self.status.as_ref(),
                        &mut offset,
                    );
                })
                .unwrap();

            Rendered {
                buffer: terminal.backend().buffer().clone(),
                cursor: terminal.get_cursor_position().unwrap(),
                page,
                offset,
            }
        }
    }

    /// The commands on the rows of the list, with the offset kept from the draws before
    fn listed(finder: &Finder, height: u16, offset: &mut usize) -> Vec<String> {
        let rendered = Render::new(finder).height(height).offset(*offset).run();
        *offset = rendered.offset;

        (FIRST_ROW..height)
            .map(|y| text(&rendered.buffer, y).trim().to_string())
            .collect()
    }

    fn text(buffer: &Buffer, y: u16) -> String {
        (0..buffer.area.width)
            .map(|x| buffer[(x, y)].symbol())
            .collect()
    }

    /// The column of the first command, on the selected row
    fn first_command_column(buffer: &Buffer) -> usize {
        text(buffer, FIRST_ROW).find("ls").unwrap()
    }

    /// The match is at 20 and 21, after the indent, the time and two spaces
    const HISTORY: &str = "#1700000000\nls -la\n#1700000000\nls\n";

    #[test]
    fn shows_the_selected_row_on_its_background_with_the_match_over_it() {
        let buffer = Render::new(&finder(HISTORY)).run().buffer;
        let cell = |x| &buffer[(x, FIRST_ROW)];

        assert!((0..40).all(|x| cell(x).bg == Color::DarkGray));
        assert!((20..22).all(|x| cell(x).fg == Color::Yellow));
        assert!(cell(20).modifier.contains(Modifier::BOLD));
        // Neither the time nor the rest of the command dimmed
        assert!((0..20).chain(22..40).all(|x| cell(x).fg == Color::Reset));
    }

    #[test]
    fn scrolls_the_list_only_to_keep_the_selection_in_view() {
        let mut finder = finder("ls 1\nls 2\nls 3\nls 4\nls 5\nls 6\n");
        let commands: Vec<String> = finder
            .matches()
            .iter()
            .map(|found| finder.commands()[found.command_index].text.clone())
            .collect();
        let offset = &mut 0;

        assert_eq!(listed(&finder, 5, offset), commands[0..3]);

        finder.select(4);
        assert_eq!(listed(&finder, 5, offset), commands[2..5]);

        // Back up a row, still in view
        finder.select(3);
        assert_eq!(listed(&finder, 5, offset), commands[2..5]);

        finder.select(1);
        assert_eq!(listed(&finder, 5, offset), commands[1..4]);
    }

    #[test]
    fn draws_without_rows_for_the_list() {
        let mut finder = finder("ls 1\nls 2\n");
        finder.select(1);
        let offset = &mut 0;

        assert_eq!(listed(&finder, 2, offset), Vec::<String>::new());
        assert_eq!(*offset, 1);
    }

    #[test]
    fn says_so_where_the_commands_would_start_when_nothing_matches() {
        let finder = test_util::finder(&history::parse(HISTORY.as_bytes()), "pwd");
        let theme = test_util::theme();
        let buffer = Render::new(&finder).run().buffer;

        assert_eq!(
            text(&buffer, FIRST_ROW).trim_end(),
            format!("{}no matches", " ".repeat(20))
        );
        assert_eq!(buffer[(20, FIRST_ROW)].fg, Color::DarkGray);
        assert!(text(&buffer, DIVIDER_ROW).ends_with(" 0/2 ──"));
        assert_eq!(buffer[(34, DIVIDER_ROW)].fg, theme.error.fg.unwrap());
    }

    #[test]
    fn says_there_are_no_commands_for_an_empty_history() {
        let buffer = Render::new(&finder("")).prompt("$ ").run().buffer;

        assert_eq!(text(&buffer, FIRST_ROW).trim_end(), "  no commands");
    }

    #[test]
    fn dims_the_times_of_other_rows() {
        let buffer = Render::new(&finder(HISTORY)).run().buffer;
        let cell = |x| &buffer[(x, FIRST_ROW + 1)];

        assert!((0..40).all(|x| cell(x).bg == Color::Reset));
        assert!((2..18).all(|x| cell(x).fg == Color::DarkGray));
        assert!((20..22).all(|x| cell(x).fg == Color::Yellow));
        assert_eq!(cell(23).fg, Color::Reset);
    }

    #[test]
    fn draws_with_the_theme() {
        let theme = Theme {
            prompt: Style::new().fg(Color::Green),
            count: Style::new().fg(Color::Magenta),
            matched: Style::new().fg(Color::Red),
            dim: Style::new().fg(Color::Gray),
            selected: Style::new().bg(Color::Blue),
            ..test_util::theme()
        };
        let buffer = Render::new(&finder(HISTORY)).theme(theme).run().buffer;

        assert_eq!(buffer[(0, 0)].fg, Color::Green);
        assert_eq!(buffer[(0, DIVIDER_ROW)].fg, Color::Gray);
        assert_eq!(buffer[(34, DIVIDER_ROW)].fg, Color::Magenta);
        assert_eq!(buffer[(0, FIRST_ROW)].bg, Color::Blue);
        assert_eq!(buffer[(20, FIRST_ROW)].fg, Color::Red);
        assert_eq!(buffer[(20, FIRST_ROW)].bg, Color::Blue);
        assert_eq!(buffer[(2, FIRST_ROW + 1)].fg, Color::Gray);
        assert_eq!(buffer[(20, FIRST_ROW + 1)].fg, Color::Red);
    }

    #[test]
    fn pads_the_time_column() {
        let buffer = Render::new(&finder("ls\n#1700000000\nls -la\n"))
            .run()
            .buffer;

        // The command after the time and two spaces, also where the time is blank
        assert_eq!(
            text(&buffer, FIRST_ROW + 1).find("ls").unwrap(),
            first_command_column(&buffer)
        );
        assert_eq!(first_command_column(&buffer), 2 + TIME_COLUMN);
    }

    #[test]
    fn shows_the_label_before_the_query_in_its_color() {
        let theme = Theme {
            backup: Style::new().fg(Color::Magenta),
            ..test_util::theme()
        };
        let rendered = Render::new(&finder("ls\n"))
            .label("backup:20261030-142501")
            .theme(theme)
            .run();

        assert_eq!(
            text(&rendered.buffer, 0).trim_end(),
            "backup:20261030-142501› ls"
        );
        assert_eq!(rendered.buffer[(0, 0)].fg, Color::Magenta);
        assert_eq!(rendered.buffer[(24, 0)].fg, Color::Reset);
        assert_eq!(rendered.cursor, Position::new(26, 0));
    }

    #[test]
    fn shows_the_label_after_the_shell_prompt_and_the_commands_under_the_query() {
        let rendered = Render::new(&finder("#1700000000\nls\n"))
            .prompt("[user@pc ~/src]$ ")
            .label("backup")
            .width(60)
            .run();

        assert_eq!(
            text(&rendered.buffer, 0).trim_end(),
            "[user@pc ~/src]$ backup› ls"
        );
        assert_eq!(rendered.cursor, Position::new(27, 0));
        assert_eq!(text(&rendered.buffer, FIRST_ROW).find("ls"), Some(25));
    }

    #[test]
    fn puts_the_cursor_where_it_is_in_the_query() {
        let mut finder = finder("ls\n");
        finder.left();

        assert_eq!(Render::new(&finder).run().cursor, Position::new(3, 0));
    }

    #[test]
    fn keeps_the_prompt_line_in_a_pane_of_one_row() {
        let Rendered { buffer, cursor, .. } = Render::new(&finder(HISTORY)).height(1).run();

        assert_eq!(text(&buffer, 0).trim_end(), "› ls");
        assert_eq!(cursor, Position::new(4, 0));
    }

    #[test]
    fn scrolls_a_query_too_long_for_the_line_to_the_cursor() {
        let finder = query_finder("0123456789abcdefghij", false);
        let Rendered { buffer, cursor, .. } = Render::new(&finder).width(20).run();

        assert_eq!(text(&buffer, 0), "› …456789abcdefghij ");
        assert_eq!(cursor, Position::new(19, 0));
    }

    #[test]
    fn keeps_the_start_of_a_long_query_while_the_cursor_fits() {
        let finder = query_finder("0123456789abcdefghij", true);
        let Rendered { buffer, cursor, .. } = Render::new(&finder).width(20).run();

        assert_eq!(text(&buffer, 0), "› 0123456789abcdefgh");
        assert_eq!(cursor, Position::new(2, 0));
    }

    #[test]
    fn scrolls_a_long_query_after_the_shell_prompt() {
        let finder = query_finder("0123456789abcdefghij", false);
        let Rendered { buffer, cursor, .. } = Render::new(&finder).prompt("dir$ ").width(20).run();

        assert_eq!(text(&buffer, 0), "dir$ …789abcdefghij ");
        assert_eq!(cursor, Position::new(19, 0));
    }

    #[test]
    fn scrolls_wide_characters_whole() {
        let finder = query_finder("日本語のコマンドを検索する", false);
        let Rendered { buffer, cursor, .. } = Render::new(&finder).width(20).run();

        assert_eq!(text(&buffer, 0), "› …マ ン ド を 検 索 す る  ");
        assert_eq!(cursor, Position::new(19, 0));
    }

    #[test]
    fn shows_the_count_in_the_divider_under_the_prompt_line() {
        let buffer = Render::new(&finder(HISTORY)).run().buffer;

        assert_eq!(text(&buffer, 0).trim_end(), "› ls");
        assert_eq!(
            text(&buffer, DIVIDER_ROW),
            format!("{} 2/2 ──", DASH.repeat(33))
        );
        assert!(
            (0..34)
                .chain(37..40)
                .all(|x| buffer[(x, DIVIDER_ROW)].fg == Color::DarkGray)
        );
    }

    #[test]
    fn keeps_the_count_in_its_color_after_a_long_query() {
        let finder = query_finder("0123456789abcdefghij", false);
        let buffer = Render::new(&finder).width(20).run().buffer;
        let theme = test_util::theme();

        assert_eq!(
            text(&buffer, DIVIDER_ROW),
            format!("{} 0/1 ──", DASH.repeat(13))
        );
        // Nothing matches the query, so the count is in the error color
        for x in 14..17 {
            assert_eq!(buffer[(x, DIVIDER_ROW)].fg, theme.error.fg.unwrap());
        }
    }

    #[test]
    fn cuts_the_status_to_the_divider() {
        let finder = query_finder("0123456789", false);
        let status = Status::Deleted("deleted git commit".to_string());
        let Rendered { buffer, cursor, .. } = Render::new(&finder).width(20).status(status).run();

        assert_eq!(text(&buffer, 0).trim_end(), "› 0123456789");
        assert_eq!(text(&buffer, DIVIDER_ROW), " deleted git com… ──");
        assert_eq!(cursor, Position::new(12, 0));
    }

    #[test]
    fn asks_before_deleting_from_backups_in_the_error_color() {
        let finder = query_finder("0123456789", false);
        let status = Status::Confirm("delete from every backup? y: ls".to_string());
        let buffer = Render::new(&finder).width(40).status(status).run().buffer;

        assert_eq!(
            text(&buffer, DIVIDER_ROW),
            format!("{} delete from every backup? y: ls ──", DASH.repeat(5))
        );
        assert_eq!(
            buffer[(6, DIVIDER_ROW)].fg,
            test_util::theme().error.fg.unwrap()
        );
    }

    #[test]
    fn shows_only_dashes_in_a_divider_too_narrow_for_the_count() {
        let finder = finder(HISTORY);

        for width in [4, 6] {
            let buffer = Render::new(&finder).width(width).run().buffer;

            assert_eq!(
                text(&buffer, DIVIDER_ROW),
                DASH.repeat(width.into()),
                "{width}"
            );
        }

        let buffer = Render::new(&finder).width(7).run().buffer;
        assert_eq!(text(&buffer, DIVIDER_ROW), " 2/2 ──");
    }

    #[test]
    fn shows_the_shell_prompt_in_its_colors_before_the_query() {
        let prompt = "\x01\x1b[35m\x02dir\x01\x1b[0m\x02$ ";
        let buffer = Render::new(&finder("ls\n")).prompt(prompt).run().buffer;

        assert_eq!(text(&buffer, 0).trim_end(), "dir$ ls");
        assert_eq!(buffer[(0, 0)].fg, Color::Magenta);
        assert_eq!(buffer[(3, 0)].fg, Color::Reset);
    }

    #[test]
    fn shows_the_arrow_for_a_prompt_with_nothing_to_show() {
        let prompt = "\x01\x1b]0;title\x07\x02";
        let buffer = Render::new(&finder("ls\n")).prompt(prompt).run().buffer;

        assert!(text(&buffer, 0).starts_with("› ls "));
        assert_eq!(first_command_column(&buffer), 2);
    }

    #[test]
    fn lines_the_commands_up_under_the_query_with_the_times_before_them() {
        let prompt = "[user@pc ~/src/utils (main)]$ ";
        let buffer = Render::new(&finder(HISTORY))
            .prompt(prompt)
            .width(120)
            .run()
            .buffer;

        assert_eq!(first_command_column(&buffer), prompt.len());
        assert_eq!(
            text(&buffer, FIRST_ROW).find("20").unwrap(),
            prompt.len() - 2 - TIME_WIDTH
        );
        assert_eq!(
            text(&buffer, FIRST_ROW + 1).find("ls").unwrap(),
            prompt.len()
        );
        assert!((0..120).all(|x| buffer[(x, FIRST_ROW)].bg == Color::DarkGray));
    }

    #[test]
    fn starts_the_commands_under_the_query_after_a_shell_prompt() {
        assert_eq!(command_column(Some(30), TIME_COLUMN), 30);
        assert_eq!(command_column(Some(18), TIME_COLUMN), 18);
        // Without times the gutter is blank
        assert_eq!(command_column(Some(13), 0), 13);
    }

    #[test]
    fn keeps_the_indent_without_a_shell_prompt() {
        assert_eq!(command_column(None, TIME_COLUMN), INDENT + TIME_COLUMN);
        assert_eq!(command_column(None, 0), INDENT);
    }

    #[test]
    fn keeps_the_indent_for_a_prompt_too_narrow_for_the_times() {
        assert_eq!(command_column(Some(17), TIME_COLUMN), INDENT + TIME_COLUMN);
    }

    #[test]
    fn shows_the_arrow_for_a_prompt_wider_than_60_percent_of_the_pane() {
        let finder = finder("ls\n");
        let (kept_prompt, wide_prompt) = (
            format!("{}$ ", "d".repeat(22)),
            format!("{}$ ", "d".repeat(23)),
        );
        let kept = Render::new(&finder).prompt(&kept_prompt).run().buffer;
        let wide = Render::new(&finder).prompt(&wide_prompt).run().buffer;

        assert!(text(&kept, 0).starts_with(&format!("{kept_prompt}ls ")));
        assert_eq!(first_command_column(&kept), 24);
        assert!(text(&wide, 0).starts_with("› ls "));
        assert_eq!(first_command_column(&wide), INDENT);
    }

    #[test]
    fn hides_the_times_with_a_prompt_wider_than_60_percent_of_the_pane() {
        let finder = finder(HISTORY);
        let wide_prompt = format!("{}$ ", "d".repeat(23));
        let wide = Render::new(&finder).prompt(&wide_prompt).run().buffer;
        let empty = Render::new(&finder).prompt("").run().buffer;

        assert_eq!(text(&wide, FIRST_ROW).trim_end(), "  ls");
        assert_eq!(text(&wide, FIRST_ROW + 1).trim_end(), "  ls -la");
        // Not for a prompt with nothing to show
        assert_eq!(first_command_column(&empty), INDENT + TIME_COLUMN);
    }

    #[test]
    fn hides_the_times_when_turned_off() {
        let prompt = "[user@pc ~/src/utils (main)]$ ";
        let finder = finder(HISTORY);
        let plain = Render::new(&finder).no_times().run().buffer;
        let after_prompt = Render::new(&finder)
            .prompt(prompt)
            .width(120)
            .no_times()
            .run()
            .buffer;

        assert_eq!(text(&plain, FIRST_ROW).trim_end(), "  ls");
        assert_eq!(text(&plain, FIRST_ROW + 1).trim_end(), "  ls -la");
        // Still under the query
        assert_eq!(first_command_column(&after_prompt), prompt.len());
        assert_eq!(text(&after_prompt, FIRST_ROW).trim(), "ls");
    }

    /// ls ran three times, ls -la once after it: lighter, but more recent
    const RUNS: &str = "ls\nls\nls\nls -la\n";

    /// A weight bar of the test config's steps
    fn bars(steps: usize) -> String {
        String::from(test_util::config().bar).repeat(steps)
    }

    #[test]
    fn shows_weight_bars_in_place_of_the_times_without_the_runs() {
        let finder = test_util::weights_finder(&history::parse(RUNS.as_bytes()), "ls", false);
        let buffer = Render::new(&finder).run().buffer;

        assert_eq!(
            text(&buffer, FIRST_ROW).trim_end(),
            format!("  {}  ls", bars(16))
        );
        // ln 2 / ln 4 of the heaviest, half the squares
        assert_eq!(
            text(&buffer, FIRST_ROW + 1).trim_end(),
            format!("  {}{}  ls -la", " ".repeat(8), bars(8))
        );
        // Dimmed off the selected row
        assert!((2..18).all(|x| buffer[(x, FIRST_ROW)].fg == Color::Reset));
        assert!((10..18).all(|x| buffer[(x, FIRST_ROW + 1)].fg == Color::DarkGray));
    }

    #[test]
    fn draws_weight_bars_with_the_bar_of_the_config() {
        let finder = test_util::weights_finder(&history::parse(RUNS.as_bytes()), "ls", false);
        let buffer = Render::new(&finder).bar('#').run().buffer;

        assert_eq!(
            text(&buffer, FIRST_ROW).trim_end(),
            format!("  {}  ls", "#".repeat(16))
        );
        assert_eq!(
            text(&buffer, FIRST_ROW + 1).trim_end(),
            format!("  {}{}  ls -la", " ".repeat(8), "#".repeat(8))
        );
    }

    #[test]
    fn shows_weight_bars_whatever_the_times() {
        let finder = test_util::weights_finder(
            &history::parse(format!("#1700000000\n{RUNS}").as_bytes()),
            "ls",
            false,
        );
        let buffer = Render::new(&finder).no_times().run().buffer;

        assert!(text(&buffer, FIRST_ROW).starts_with(&format!("  {}  ls", bars(16))));
    }

    #[test]
    fn shows_the_runs_for_weight_bars_hidden_by_a_prompt_wider_than_60_percent_of_the_pane() {
        let finder = test_util::weights_finder(&history::parse(RUNS.as_bytes()), "ls", false);
        let wide_prompt = format!("{}$ ", "d".repeat(23));
        let buffer = Render::new(&finder).prompt(&wide_prompt).run().buffer;

        assert_eq!(text(&buffer, FIRST_ROW).trim_end(), "  ls (3)");
        assert_eq!(text(&buffer, FIRST_ROW + 1).trim_end(), "  ls -la");
    }

    #[test]
    fn rounds_weight_bars_to_squares_with_at_least_one() {
        assert_eq!(weight_steps(0.0), 1);
        assert_eq!(weight_steps(0.46), 7);
        assert_eq!(weight_steps(0.47), 8);
        assert_eq!(weight_steps(1.0), 16);
    }

    /// RUNS with times, ls -la an hour after
    const TIMED_RUNS: &str =
        "#1700000000\nls\n#1700000000\nls\n#1700000000\nls\n#1700003600\nls -la\n";

    /// The time of a command as the finder shows it, in the local time zone
    fn time_of(finder: &Finder, text: &str) -> String {
        test_util::command(finder, text).time.clone().unwrap()
    }

    #[test]
    fn shows_the_times_at_the_right_edge_with_weight_bars_dimmed_off_the_selected_row() {
        let finder = test_util::weights_finder(&history::parse(TIMED_RUNS.as_bytes()), "ls", false);
        let buffer = Render::new(&finder).width(60).run().buffer;

        assert_eq!(
            text(&buffer, FIRST_ROW),
            format!(
                "{:<44}{}",
                format!("  {}  ls", bars(16)),
                time_of(&finder, "ls")
            )
        );
        assert_eq!(
            text(&buffer, FIRST_ROW + 1),
            format!(
                "{:<44}{}",
                format!("          {}  ls -la", bars(8)),
                time_of(&finder, "ls -la")
            )
        );
        assert!((44..60).all(|x| buffer[(x, FIRST_ROW)].fg == Color::Reset));
        assert!((44..60).all(|x| buffer[(x, FIRST_ROW + 1)].fg == Color::DarkGray));
    }

    #[test]
    fn leaves_the_times_out_of_the_weights_layout_when_turned_off() {
        let finder = test_util::weights_finder(&history::parse(TIMED_RUNS.as_bytes()), "ls", false);
        let buffer = Render::new(&finder).width(60).no_times().run().buffer;

        assert_eq!(
            text(&buffer, FIRST_ROW).trim_end(),
            format!("  {}  ls", bars(16))
        );
    }

    #[test]
    fn leaves_the_times_out_of_a_pane_too_narrow_for_the_command_next_to_them() {
        let finder = test_util::weights_finder(&history::parse(TIMED_RUNS.as_bytes()), "ls", false);
        // 20 columns left for the command, 18 short of the time and its gap
        let buffer = Render::new(&finder).width(40).run().buffer;

        assert_eq!(
            text(&buffer, FIRST_ROW).trim_end(),
            format!("  {}  ls", bars(16))
        );
    }

    /// A command from a backup on mac, with its time, in the weights layout
    fn timed_backup_finder() -> Finder {
        let entry = Entry::new(
            "ls".to_string(),
            Some(1_700_000_000),
            vec!["mac".to_string()],
        );

        test_util::weights_finder(&[entry], "", false)
    }

    #[test]
    fn shows_the_hosts_before_the_time_at_the_right_edge() {
        let finder = timed_backup_finder();
        let buffer = Render::new(&finder).width(80).run().buffer;

        assert_eq!(
            text(&buffer, FIRST_ROW),
            format!(
                "{:<59}mac  {}",
                format!("  {}  ls", bars(16)),
                time_of(&finder, "ls")
            )
        );
    }

    #[test]
    fn leaves_the_hosts_out_before_the_time() {
        let finder = timed_backup_finder();
        // 22 columns left for the command after the time, 3 short of the hosts and their gap
        let buffer = Render::new(&finder).width(60).run().buffer;

        assert_eq!(
            text(&buffer, FIRST_ROW),
            format!(
                "{:<44}{}",
                format!("  {}  ls", bars(16)),
                time_of(&finder, "ls")
            )
        );
    }

    /// Commands from backups with their hosts, oldest first, without a query
    fn backup_finder(entries: &[(&str, &[&str])]) -> Finder {
        let entries: Vec<Entry> = entries
            .iter()
            .map(|(command, hosts)| {
                let hosts = hosts.iter().map(|host| (*host).to_string()).collect();
                Entry::new((*command).to_string(), None, hosts)
            })
            .collect();

        test_util::finder(&entries, "")
    }

    #[test]
    fn shows_the_hosts_at_the_right_edge_dimmed_off_the_selected_row() {
        let finder = backup_finder(&[("ls -la", &["mac", "air"]), ("ls", &["mac"])]);
        let buffer = Render::new(&finder).run().buffer;

        assert_eq!(
            text(&buffer, FIRST_ROW),
            format!("  ls{}mac", " ".repeat(33))
        );
        assert_eq!(
            text(&buffer, FIRST_ROW + 1),
            format!("  ls -la{}air,mac", " ".repeat(25))
        );
        assert!((37..40).all(|x| buffer[(x, FIRST_ROW)].fg == Color::Reset));
        assert!((33..40).all(|x| buffer[(x, FIRST_ROW + 1)].fg == Color::DarkGray));
    }

    #[test]
    fn cuts_hosts_wider_than_their_column() {
        let finder = backup_finder(&[("ls", &["mac", "a-very-long-host-name"])]);
        let buffer = Render::new(&finder).width(50).run().buffer;

        assert!(text(&buffer, FIRST_ROW).ends_with(" a-very-long-host-na…"));
    }

    #[test]
    fn cuts_the_command_before_the_hosts() {
        let command = "echo one two three four five six seven eight nine";
        let finder = backup_finder(&[(command, &["mac"])]);
        let buffer = Render::new(&finder).run().buffer;

        assert_eq!(
            text(&buffer, FIRST_ROW),
            format!("  {}…  mac", &command[..32])
        );
    }

    #[test]
    fn leaves_the_hosts_out_of_a_pane_too_narrow_for_the_command() {
        let finder = backup_finder(&[("ls", &["mac"])]);
        let narrow = Render::new(&finder).width(26).run().buffer;
        let wide_enough = Render::new(&finder).width(27).run().buffer;

        assert_eq!(text(&narrow, FIRST_ROW).trim_end(), "  ls");
        assert!(text(&wide_enough, FIRST_ROW).ends_with("  mac"));
    }

    /// Most recent first, a command of 54 columns run twice, matched by ls at its end
    const LONG: &str = "ls -la\n\
        echo one two three four five six seven eight nine; ls\n\
        echo one two three four five six seven eight nine; ls\n";

    fn shown(text: &str, fitted: &Fitted) -> String {
        let start = if fitted.cut_start { ELLIPSIS } else { "" };
        let end = if fitted.cut_end { ELLIPSIS } else { "" };

        format!("{start}{}{end}", &text[fitted.shown.clone()])
    }

    #[test]
    fn fits_text_that_fits_as_it_is() {
        let fitted = fit("0123456789", &(3..4), 10);

        assert_eq!(shown("0123456789", &fitted), "0123456789");
        assert!(!fitted.is_cut());
    }

    #[test]
    fn cuts_the_end_when_the_match_is_early() {
        assert_eq!(
            shown("0123456789", &fit("0123456789", &(1..2), 6)),
            "01234…"
        );
    }

    #[test]
    fn shows_the_end_when_the_match_is_in_it() {
        assert_eq!(
            shown("0123456789", &fit("0123456789", &(8..9), 6)),
            "…56789"
        );
    }

    #[test]
    fn puts_a_match_in_the_middle_a_third_of_the_way_in() {
        let text = "0123456789abcdefghij";

        assert_eq!(shown(text, &fit(text, &(10..11), 8)), "…89abcd…");
    }

    #[test]
    fn fits_wide_characters_without_splitting_them() {
        let text = "日本語のテキスト";

        for width in 3..16 {
            for range in [0..3, 12..15, 21..24] {
                let fitted = fit(text, &range, width);

                assert!(shown(text, &fitted).width() <= width, "{width} {range:?}");
                assert!(fitted.shown.start <= range.start, "{width} {range:?}");
            }
        }
    }

    /// The text of each span, in capitals where it is highlighted
    fn highlighted(text: &str, ranges: &[Range<usize>], shown: Range<usize>) -> Vec<String> {
        let matched = Style::new().fg(Color::Yellow);

        highlight(text, ranges, shown, matched)
            .iter()
            .map(|span| {
                if span.style == matched {
                    span.content.to_uppercase()
                } else {
                    span.content.to_string()
                }
            })
            .collect()
    }

    #[test]
    fn highlights_every_match() {
        assert_eq!(
            highlighted("git -C api push", &[0..3, 11..15], 0..15),
            ["", "GIT", " -C api ", "PUSH", ""]
        );
    }

    #[test]
    fn highlights_what_is_shown_of_the_matches() {
        assert_eq!(
            highlighted("git -C api push", &[0..3, 11..15], 5..13),
            ["C api ", "PU", ""]
        );
        assert_eq!(highlighted("ls", &[], 0..2), ["ls"]);
    }

    #[test]
    fn fits_the_text_around_every_match() {
        assert_eq!(around(&[2..4, 8..9]), 2..9);
        assert_eq!(around(&[]), 0..0);
    }

    #[test]
    fn highlights_the_words_of_the_query_in_a_row() {
        let finder = test_util::finder(&history::parse(b"git -C api push\n"), "push git");
        let buffer = Render::new(&finder).run().buffer;
        let cell = |x| &buffer[(x, FIRST_ROW)];

        assert_eq!(text(&buffer, FIRST_ROW).trim_end(), "  git -C api push");
        assert!((2..5).chain(13..17).all(|x| cell(x).fg == Color::Yellow));
        assert!((5..13).all(|x| cell(x).fg == Color::Reset));
    }

    #[test]
    fn wraps_after_spaces() {
        assert_eq!(wrap("aa bb cc", 5), [0..6, 6..8]);
        assert_eq!(wrap("aa bbb cc", 5), [0..3, 3..7, 7..9]);
    }

    #[test]
    fn wraps_words_longer_than_the_line_where_they_fill_it() {
        assert_eq!(wrap("abcdefgh", 3), [0..3, 3..6, 6..8]);
        assert_eq!(wrap("日本", 1), [0..3, 3..6]);
    }

    #[test]
    fn shows_the_match_of_a_long_row_with_the_number_of_runs() {
        let buffer = Render::new(&finder(LONG)).run().buffer;
        let row = text(&buffer, FIRST_ROW);

        assert_eq!(row, "  …our five six seven eight nine; ls (2)");
        assert_eq!(
            buffer[(
                u16::try_from(row.find("ls").unwrap()).unwrap() - 2,
                FIRST_ROW
            )]
                .fg,
            Color::Yellow
        );
        assert!((0..40).all(|x| buffer[(x, FIRST_ROW)].bg == Color::DarkGray));
    }

    #[test]
    fn previews_the_selected_command_when_its_row_is_cut() {
        let buffer = Render::new(&finder(LONG)).preview().height(8).run().buffer;
        // Under three rows of the list
        let divider = FIRST_ROW + 3;

        assert_eq!(text(&buffer, divider), DASH.repeat(40));
        assert_eq!(
            text(&buffer, divider + 1).trim_end(),
            "echo one two three four five six seven"
        );
        assert_eq!(text(&buffer, divider + 2).trim_end(), "eight nine; ls");
        assert_eq!(buffer[(12, divider + 2)].fg, Color::Yellow);
    }

    #[test]
    fn previews_nothing_for_a_row_that_fits() {
        let mut finder = finder(LONG);
        finder.down();
        let buffer = Render::new(&finder).preview().height(8).run().buffer;

        assert!(!(FIRST_ROW..8).any(|y| text(&buffer, y).starts_with(DASH)));
    }

    #[test]
    fn previews_nothing_without_room_for_the_list() {
        let buffer = Render::new(&finder(LONG)).preview().height(6).run().buffer;

        assert!(!(FIRST_ROW..6).any(|y| text(&buffer, y).starts_with(DASH)));
    }

    #[test]
    fn previews_nothing_when_the_preview_is_off() {
        let rendered = Render::new(&finder(LONG)).height(8).run();

        assert!(!(FIRST_ROW..8).any(|y| text(&rendered.buffer, y).starts_with(DASH)));
        assert_eq!(rendered.page, 6);
    }

    #[test]
    fn gives_the_list_rows_as_the_page_without_the_preview() {
        let mut finder = finder(LONG);

        // The prompt line, the two dividers and two lines of preview leave 3 rows
        assert_eq!(Render::new(&finder).preview().height(8).run().page, 3);

        finder.down();
        assert_eq!(Render::new(&finder).preview().height(8).run().page, 6);
    }

    /// Rows of a pane of 5 with the list above
    const ABOVE_DIVIDER_ROW: u16 = 3;
    const ABOVE_PROMPT_ROW: u16 = 4;

    /// The commands on the rows of the list above the prompt line, from the divider up
    fn listed_above(finder: &Finder, offset: &mut usize) -> Vec<String> {
        let rendered = Render::new(finder).above().height(5).offset(*offset).run();
        *offset = rendered.offset;

        (0..ABOVE_DIVIDER_ROW)
            .rev()
            .map(|y| text(&rendered.buffer, y).trim().to_string())
            .collect()
    }

    #[test]
    fn lists_the_matches_up_from_the_divider_above_the_prompt_line() {
        let Rendered { buffer, cursor, .. } = Render::new(&finder(HISTORY)).above().height(5).run();

        assert_eq!(text(&buffer, ABOVE_PROMPT_ROW).trim_end(), "› ls");
        assert_eq!(cursor, Position::new(4, ABOVE_PROMPT_ROW));
        assert!(text(&buffer, ABOVE_DIVIDER_ROW).ends_with(" 2/2 ──"));
        // The most recent command is selected, next to the divider
        assert!(text(&buffer, 2).trim_end().ends_with("  ls"));
        assert!((0..40).all(|x| buffer[(x, 2)].bg == Color::DarkGray));
        assert!(text(&buffer, 1).trim_end().ends_with("  ls -la"));
        assert_eq!(text(&buffer, 0).trim(), "");
    }

    #[test]
    fn scrolls_the_list_above_the_prompt_line_to_keep_the_selection_in_view() {
        let mut finder = finder("ls 1\nls 2\nls 3\nls 4\nls 5\nls 6\n");
        let commands: Vec<String> = finder
            .matches()
            .iter()
            .map(|found| finder.commands()[found.command_index].text.clone())
            .collect();
        let offset = &mut 0;

        assert_eq!(listed_above(&finder, offset), commands[0..3]);

        finder.select(4);
        assert_eq!(listed_above(&finder, offset), commands[2..5]);

        finder.select(1);
        assert_eq!(listed_above(&finder, offset), commands[1..4]);
    }

    #[test]
    fn says_so_next_to_the_divider_when_nothing_matches_above_the_prompt_line() {
        let finder = test_util::finder(&history::parse(HISTORY.as_bytes()), "pwd");
        let buffer = Render::new(&finder).above().height(5).run().buffer;

        assert_eq!(
            text(&buffer, 2).trim_end(),
            format!("{}no matches", " ".repeat(20))
        );
        assert_eq!(text(&buffer, 0).trim(), "");
        assert_eq!(text(&buffer, 1).trim(), "");
    }

    #[test]
    fn previews_the_selected_command_over_the_list_above_the_prompt_line() {
        let buffer = Render::new(&finder(LONG))
            .above()
            .preview()
            .height(8)
            .run()
            .buffer;

        assert_eq!(
            text(&buffer, 0).trim_end(),
            "echo one two three four five six seven"
        );
        assert_eq!(text(&buffer, 1).trim_end(), "eight nine; ls");
        assert_eq!(text(&buffer, 2), DASH.repeat(40));
        // Three rows of the list, the selected command next to the divider
        assert!(text(&buffer, 5).ends_with("ls (2)"));
        assert!(text(&buffer, 6).ends_with(" 2/2 ──"));
        assert_eq!(text(&buffer, 7).trim_end(), "› ls");
    }

    #[test]
    fn keeps_the_prompt_line_in_a_pane_of_one_row_above_the_list() {
        let Rendered { buffer, cursor, .. } = Render::new(&finder(HISTORY)).above().height(1).run();

        assert_eq!(text(&buffer, 0).trim_end(), "› ls");
        assert_eq!(cursor, Position::new(4, 0));
    }

    #[test]
    fn keeps_the_prompt_line_and_the_divider_in_a_pane_of_two_rows_above_the_list() {
        let buffer = Render::new(&finder(HISTORY)).above().height(2).run().buffer;

        assert!(text(&buffer, 0).ends_with(" 2/2 ──"));
        assert_eq!(text(&buffer, 1).trim_end(), "› ls");
    }

    #[test]
    fn leaves_only_the_shell_prompt_on_the_first_row_to_close() {
        let prompt = prompt::parse("\x01\x1b[35m\x02dir\x01\x1b[0m\x02$ ");
        let mut terminal = Terminal::new(TestBackend::new(20, 3)).unwrap();
        terminal
            .draw(|frame| draw_shell_prompt(frame, Some(&prompt)))
            .unwrap();
        let buffer = terminal.backend().buffer();

        assert_eq!(text(buffer, 0), format!("dir$ {}", " ".repeat(15)));
        assert_eq!(buffer[(0, 0)].fg, Color::Magenta);
        assert_eq!(text(buffer, 1).trim(), "");
        assert_eq!(text(buffer, 2).trim(), "");
    }

    #[test]
    fn draws_the_border_on_the_last_row_below_the_list() {
        let rendered = Render::new(&finder(HISTORY)).border().height(6).run();
        let buffer = &rendered.buffer;

        assert_eq!(text(buffer, 5), DASH.repeat(40));
        assert!((0..40).all(|x| buffer[(x, 5)].fg == Color::DarkGray));
        // Both commands, then a blank row over the border
        assert!(text(buffer, FIRST_ROW + 1).trim_end().ends_with("  ls -la"));
        assert_eq!(text(buffer, 4).trim(), "");
        assert_eq!(rendered.page, 3);
    }

    #[test]
    fn draws_the_border_on_the_first_row_above_the_list() {
        let rendered = Render::new(&finder(HISTORY))
            .above()
            .border()
            .height(6)
            .run();
        let buffer = &rendered.buffer;

        assert_eq!(text(buffer, 0), DASH.repeat(40));
        assert_eq!(text(buffer, 1).trim(), "");
        assert!(text(buffer, 2).trim_end().ends_with("  ls -la"));
        assert!(text(buffer, 3).trim_end().ends_with("  ls"));
        assert!(text(buffer, 4).ends_with(" 2/2 ──"));
        assert_eq!(text(buffer, 5).trim_end(), "› ls");
        assert_eq!(rendered.page, 3);
    }

    #[test]
    fn draws_the_border_past_the_preview() {
        let below = Render::new(&finder(LONG))
            .border()
            .preview()
            .height(9)
            .run()
            .buffer;

        assert_eq!(text(&below, 5), DASH.repeat(40));
        assert_eq!(text(&below, 7).trim_end(), "eight nine; ls");
        assert_eq!(text(&below, 8), DASH.repeat(40));

        let above = Render::new(&finder(LONG))
            .above()
            .border()
            .preview()
            .height(9)
            .run()
            .buffer;

        assert_eq!(text(&above, 0), DASH.repeat(40));
        assert_eq!(
            text(&above, 1).trim_end(),
            "echo one two three four five six seven"
        );
        assert_eq!(text(&above, 3), DASH.repeat(40));
    }

    #[test]
    fn leaves_the_border_out_without_a_row_for_the_list() {
        let finder = finder(HISTORY);
        let short = Render::new(&finder).border().height(3).run().buffer;

        assert!(text(&short, FIRST_ROW).trim_end().ends_with("  ls"));

        let rendered = Render::new(&finder).border().height(4).run();
        assert!(
            text(&rendered.buffer, FIRST_ROW)
                .trim_end()
                .ends_with("  ls")
        );
        assert_eq!(text(&rendered.buffer, 3), DASH.repeat(40));
        assert_eq!(rendered.page, 1);
    }
}
