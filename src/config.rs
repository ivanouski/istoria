//! The config file

use std::env;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::str::FromStr;

use ratatui::style::{Color, Modifier, Style};
use serde::Deserialize;
use serde::de::{self, Deserializer, MapAccess, Visitor};

use crate::context::Context;

/// The defaults as a config file, for --help and --init-config
pub const DEFAULT: &str = r#"height  = 12      # rows of the pane, the terminal's at most
list    = "below" # or "above" the prompt line, like fzf
border  = false   # a line on the far edge of the pane
preview = false   # the selected command in full when its row is cut
[colors]
prompt   = { fg = "blue", bold = true }
backup   = "yellow"        # the prompt searching backups
count    = "dark-gray"     # matches/commands
deleted  = "green"         # the status after Ctrl-D
error    = "red"           # Ctrl-D failed, the count of no matches
match    = { fg = "yellow", bold = true }
dim      = "dark-gray"     # times, run counts, cuts, dividers, border
selected = { bg = "238" }  # the selected row, under the match
"#;

const DEFAULT_HEIGHT: u16 = 12;
/// The prompt line, the divider and a row of the list
const MIN_HEIGHT: u16 = 3;

#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Capped at the terminal's height
    #[serde(deserialize_with = "height")]
    pub height: u16,
    pub list: ListPosition,
    /// A line on the pane's edge away from the prompt line
    pub border: bool,
    /// The selected command in full when its row is cut
    pub preview: bool,
    pub colors: Theme,
}

/// Where the list goes from the prompt line
#[derive(Clone, Copy, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ListPosition {
    #[default]
    Below,
    Above,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            height: DEFAULT_HEIGHT,
            list: ListPosition::default(),
            border: false,
            preview: false,
            colors: Theme::default(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Theme {
    #[serde(deserialize_with = "style")]
    pub prompt: Style,
    /// The label and › searching backups
    #[serde(deserialize_with = "style")]
    pub backup: Style,
    /// The match count in the divider
    #[serde(deserialize_with = "style")]
    pub count: Style,
    /// The status after Ctrl-D deletes a command
    #[serde(deserialize_with = "style")]
    pub deleted: Style,
    #[serde(deserialize_with = "style")]
    pub error: Style,
    #[serde(rename = "match", deserialize_with = "style")]
    pub matched: Style,
    /// Times, run counts and ellipses off the selected row, the dividers and the border
    #[serde(deserialize_with = "style")]
    pub dim: Style,
    /// The selected row, under the match
    #[serde(deserialize_with = "style")]
    pub selected: Style,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            prompt: Style::new().fg(Color::Blue).bold(),
            backup: Style::new().fg(Color::Yellow),
            count: Style::new().fg(Color::DarkGray),
            deleted: Style::new().fg(Color::Green),
            error: Style::new().fg(Color::Red),
            matched: Style::new().fg(Color::Yellow).bold(),
            dim: Style::new().fg(Color::DarkGray),
            // 236 is too close to dark backgrounds, dark gray is the dim text
            selected: Style::new().bg(Color::Indexed(238)),
        }
    }
}

impl Config {
    /// The defaults when there is no file
    pub fn load() -> Result<Self, String> {
        let Some(path) = path() else {
            return Ok(Self::default());
        };

        match fs::read_to_string(&path) {
            Ok(text) => Self::parse(&text).context(path.display()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(format!("cannot read {}: {error}", path.display())),
        }
    }

    /// Writes the defaults, never over an existing file, returns its path
    pub fn init() -> Result<PathBuf, String> {
        let path = path().ok_or("XDG_CONFIG_HOME and HOME are not set")?;
        let cannot_create = |error: io::Error| format!("cannot create {}: {error}", path.display());

        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(cannot_create)?;
        }

        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|error| match error.kind() {
                io::ErrorKind::AlreadyExists => format!("{} already exists", path.display()),
                _ => cannot_create(error),
            })?;
        // Leaves no half-written config behind
        if let Err(error) = file.write_all(DEFAULT.as_bytes()) {
            let _ = fs::remove_file(&path);
            return Err(cannot_create(error));
        }

        Ok(path)
    }

    fn parse(text: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(text)
    }
}

/// A relative `XDG_CONFIG_HOME` is ignored, as the XDG spec says
fn path() -> Option<PathBuf> {
    env::var_os("XDG_CONFIG_HOME")
        .filter(|dir| Path::new(dir).is_absolute())
        .map(PathBuf::from)
        .or_else(|| env::home_dir().map(|home| home.join(".config")))
        .map(|dir| dir.join("istoria").join("config.toml"))
}

fn height<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u16, D::Error> {
    let height = u16::deserialize(deserializer)?;

    if height < MIN_HEIGHT {
        return Err(de::Error::custom(format!(
            "height must be at least {MIN_HEIGHT}"
        )));
    }

    Ok(height)
}

/// A style is a table of colors and modifiers, or just a color for the foreground
fn style<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Style, D::Error> {
    struct StyleVisitor;

    impl<'de> Visitor<'de> for StyleVisitor {
        type Value = Style;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a color or a table of fg, bg, bold, italic, underlined, reversed")
        }

        fn visit_str<E: de::Error>(self, value: &str) -> Result<Style, E> {
            parse_color(value).map(|color| Style::new().fg(color))
        }

        fn visit_map<M: MapAccess<'de>>(self, map: M) -> Result<Style, M::Error> {
            StyleTable::deserialize(de::value::MapAccessDeserializer::new(map)).map(Style::from)
        }
    }

    deserializer.deserialize_any(StyleVisitor)
}

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct StyleTable {
    #[serde(deserialize_with = "color")]
    fg: Option<Color>,
    #[serde(deserialize_with = "color")]
    bg: Option<Color>,
    bold: bool,
    italic: bool,
    underlined: bool,
    reversed: bool,
}

impl From<StyleTable> for Style {
    fn from(table: StyleTable) -> Self {
        let mut style = Style::new();
        style.fg = table.fg;
        style.bg = table.bg;

        for (on, modifier) in [
            (table.bold, Modifier::BOLD),
            (table.italic, Modifier::ITALIC),
            (table.underlined, Modifier::UNDERLINED),
            (table.reversed, Modifier::REVERSED),
        ] {
            if on {
                style = style.add_modifier(modifier);
            }
        }

        style
    }
}

fn color<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<Color>, D::Error> {
    let name = String::deserialize(deserializer)?;
    parse_color(&name).map(Some)
}

/// Names like yellow and light-blue, 0 to 255, or #rrggbb
fn parse_color<E: de::Error>(name: &str) -> Result<Color, E> {
    Color::from_str(name).map_err(|_| E::custom(format!("unknown color \"{name}\"")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn theme(text: &str) -> Theme {
        Config::parse(text).unwrap().colors
    }

    fn error(text: &str) -> String {
        Config::parse(text).err().unwrap().to_string()
    }

    #[test]
    fn defaults_without_a_config() {
        assert_eq!(theme(""), Theme::default());
        assert_eq!(theme("[colors]\n"), Theme::default());
    }

    #[test]
    fn writes_the_defaults_as_a_config() {
        let config = Config::parse(DEFAULT).unwrap();

        assert_eq!(config.height, Config::default().height);
        assert_eq!(config.list, Config::default().list);
        assert_eq!(config.border, Config::default().border);
        assert_eq!(config.preview, Config::default().preview);
        assert_eq!(config.colors, Theme::default());
    }

    #[test]
    fn reads_the_height() {
        assert_eq!(Config::parse("").unwrap().height, DEFAULT_HEIGHT);
        assert_eq!(Config::parse("height = 20\n").unwrap().height, 20);
        assert_eq!(Config::parse("height = 3\n").unwrap().height, 3);
    }

    #[test]
    fn reads_where_the_list_goes() {
        assert_eq!(Config::parse("").unwrap().list, ListPosition::Below);
        assert_eq!(
            Config::parse("list = \"below\"\n").unwrap().list,
            ListPosition::Below
        );
        assert_eq!(
            Config::parse("list = \"above\"\n").unwrap().list,
            ListPosition::Above
        );
    }

    #[test]
    fn reads_the_border() {
        assert!(!Config::parse("").unwrap().border);
        assert!(Config::parse("border = true\n").unwrap().border);
        assert!(error("border = \"yes\"\n").contains("expected a boolean"));
    }

    #[test]
    fn reads_the_preview() {
        assert!(!Config::parse("").unwrap().preview);
        assert!(Config::parse("preview = true\n").unwrap().preview);
        assert!(error("preview = \"yes\"\n").contains("expected a boolean"));
    }

    #[test]
    fn rejects_other_list_positions() {
        assert!(error("list = \"top\"\n").contains("unknown variant `top`"));
        assert!(error("list = \"Above\"\n").contains("expected `below` or `above`"));
    }

    #[test]
    fn rejects_heights_without_a_row_for_the_list() {
        assert!(error("height = 2\n").contains("height must be at least 3"));
        assert!(error("height = 0\n").contains("height must be at least 3"));
        assert!(error("height = -1\n").contains("invalid value"));
        assert!(error("height = \"12\"\n").contains("expected u16"));
    }

    #[test]
    fn keeps_the_defaults_of_keys_left_out() {
        let theme = theme("[colors]\nprompt = { fg = \"green\" }\n");

        assert_eq!(theme.prompt, Style::new().fg(Color::Green));
        assert_eq!(theme.matched, Theme::default().matched);
    }

    #[test]
    fn reads_a_table_of_colors_and_modifiers() {
        let theme = theme(
            "[colors]\nselected = { fg = \"black\", bg = \"white\", bold = true, italic = true, \
             underlined = true, reversed = true }\n",
        );

        assert_eq!(
            theme.selected,
            Style::new()
                .fg(Color::Black)
                .bg(Color::White)
                .bold()
                .italic()
                .underlined()
                .reversed()
        );
    }

    #[test]
    fn reads_a_color_alone_as_the_foreground() {
        assert_eq!(
            theme("[colors]\nmatch = \"cyan\"\n").matched,
            Style::new().fg(Color::Cyan)
        );
    }

    #[test]
    fn reads_names_indexes_and_hex_colors() {
        let theme = theme(
            "[colors]\nprompt = \"light-blue\"\ncount = \"dark-gray\"\nmatch = \"208\"\n\
             dim = \"#ff8800\"\n",
        );

        assert_eq!(theme.prompt.fg, Some(Color::LightBlue));
        assert_eq!(theme.count.fg, Some(Color::DarkGray));
        assert_eq!(theme.matched.fg, Some(Color::Indexed(208)));
        assert_eq!(theme.dim.fg, Some(Color::Rgb(255, 136, 0)));
    }

    #[test]
    fn rejects_unknown_colors() {
        assert!(error("[colors]\nmatch = \"yelow\"\n").contains("unknown color \"yelow\""));
        assert!(
            error("[colors]\nmatch = { bg = \"#ff88\" }\n").contains("unknown color \"#ff88\"")
        );
    }

    #[test]
    fn rejects_unknown_keys() {
        assert!(error("[colours]\n").contains("unknown field `colours`"));
        assert!(error("[colors]\nmatched = \"red\"\n").contains("unknown field `matched`"));
        assert!(error("[colors]\nmatch = { blink = true }\n").contains("unknown field `blink`"));
    }

    #[test]
    fn rejects_other_values() {
        assert!(error("[colors]\nmatch = 3\n").contains("expected a color or a table"));
        assert!(error("[colors]\nmatch = { bold = \"yes\" }\n").contains("expected a boolean"));
    }
}
