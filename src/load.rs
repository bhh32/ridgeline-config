use crate::{
    error::Error,
    panel::{Panel, validate_names},
};
use rg_paths::{config_dir, config_dirs};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{fs, io::ErrorKind, path::Path};
use toml::{Table, Value};

const GENERAL_FILE: &str = "ridgeline.toml";
const PANELS_FILE: &str = "panels.toml";
const GENERAL_TEMPLATE: &str = r##"# ridgeline
#
# Every key is optional. A missing key is just the default.

# Themes live in ~/.local/share/ridgeline/themes and /usr/share/ridgeline/themes.
# A user theme shadows a system one of the same name.
# theme = "ridge-dark"
"##;
const PANELS_TEMPLATE: &str = r##"# ridgeline panels
#
# Each [[panel]] is one bar. Three across a single edge is the normal case:
# give them different `align` values and they will not collide.
#
# Writing this file replaces the system panel list outright rather than adding
# to it, so list every panel you want.

# [[panel]]
# name = "top-left"               # required, unique across all panels
# edge = "top"                    # top, bottom, left, right
# align = "start"                 # start, center, end
# length = { fraction = 0.3 }     # "fill", { pixels = N }, { fraction = 0.0-1.0 }
# size = 36                       # required, thickness in pixels
# margin = { outer = 8, gap = 8 }
# visibility = "always"           # always, overlap, dodge, auto-hide, hidden
# layer = "top"                   # background, bottom, top, overlay (top)
# outputs = ["DP-1"]              # empty means every output
#
# [panel.modules]
# start = ["launcher"]
# center = []
# end = ["clock"]
"##;

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Config {
    pub general: General,
    pub panels: Vec<Panel>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct General {
    pub theme: String,
}

impl Default for General {
    fn default() -> Self {
        Self {
            theme: "ridge-dark".into(),
        }
    }
}

// panel.toml is an arry of tables, which needs a struct to hang it on
#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
struct PanelFile {
    panel: Vec<Panel>,
}

impl Config {
    pub fn load() -> Result<Self, Error> {
        let general = read(GENERAL_FILE)?.unwrap_or_default();
        let panels: PanelFile = read(PANELS_FILE)?.unwrap_or_default();

        validate_names(&panels.panel)?;
        Ok(Self {
            general,
            panels: panels.panel,
        })
    }
}

pub fn write_templates_if_missing() -> Result<(), Error> {
    let dir = config_dir();

    for (name, template) in [
        (GENERAL_FILE, GENERAL_TEMPLATE),
        (PANELS_FILE, PANELS_TEMPLATE),
    ] {
        let path = dir.join(name);
        if path.exists() {
            continue;
        }

        fs::create_dir_all(&dir).map_err(|source| Error::Write {
            path: dir.clone(),
            source,
        })?;
        fs::write(&path, template).map_err(|source| Error::Write { path, source })?;
    }

    Ok(())
}

fn read<T: DeserializeOwned>(name: &str) -> Result<Option<T>, Error> {
    let mut merged = Table::new();
    let mut last = None;

    for dir in config_dirs() {
        let path = dir.join(name);
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == ErrorKind::NotFound => continue,
            Err(source) => return Err(Error::Read { path, source }),
        };

        merge(&mut merged, parse(&text, &path)?);
        last = Some(path);
    }

    let Some(path) = last else {
        return Ok(None);
    };

    merged
        .try_into()
        .map(Some)
        .map_err(|source| Error::Parse { path, source })
}

fn merge(base: &mut Table, overlay: Table) {
    for (key, value) in overlay {
        match (base.get_mut(&key), value) {
            (Some(Value::Table(nested)), Value::Table(path)) => merge(nested, path),
            (_, value) => {
                base.insert(key, value);
            }
        }
    }
}

fn parse<T: DeserializeOwned>(text: &str, path: &Path) -> Result<T, Error> {
    toml::from_str(text).map_err(|source| Error::Parse {
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use toml::Table;

    use super::{
        GENERAL_FILE, GENERAL_TEMPLATE, General, PANELS_FILE, PANELS_TEMPLATE, PanelFile, merge,
        parse,
    };
    use crate::{
        error::Error,
        geometry::{Align, Edge, Length},
    };
    use std::path::Path;

    fn panels(text: &str) -> PanelFile {
        parse(text, Path::new(PANELS_FILE)).expect("panels parse")
    }

    fn table(text: &str) -> Table {
        parse(text, Path::new(GENERAL_FILE)).expect("table parses")
    }

    #[test]
    fn keys_merge() {
        let mut base = table("theme = \"ridge-light\"\nicons = \"papirus\"");
        merge(&mut base, table("theme = \"ridge-dark\""));

        assert_eq!(base["theme"].as_str(), Some("ridge-dark"));
        assert_eq!(base["icons"].as_str(), Some("papirus"));
    }

    #[test]
    fn nested_tables_merge() {
        let mut base = table("[font]\nfamily = \"Inter\"\nsize = 11");
        merge(&mut base, table("[font]\nsize = 12"));

        assert_eq!(base["font"]["family"].as_str(), Some("Inter"));
        assert_eq!(base["font"]["size"].as_integer(), Some(12));
    }

    #[test]
    fn arrays_replace() {
        let mut base = table("[[panel]]\nname = \"a\"\nedge = \"top\"\nsize = 36");
        merge(
            &mut base,
            table("[[panel]]\nname = \"b\"\nedge = \"top\"\nsize = 36"),
        );
        let list = base["panel"].as_array().expect("array");

        assert_eq!(list.len(), 1);
        assert_eq!(list[0]["name"].as_str(), Some("b"));
    }

    #[test]
    fn commented_file_keeps_lower_layers() {
        let mut base = table(r#"theme = "ridge-light""#);
        merge(&mut base, table(GENERAL_TEMPLATE));

        assert_eq!(base["theme"].as_str(), Some("ridge-light"));
    }

    #[test]
    fn panels_parse() {
        let file = panels(
            r#"
            [[panel]]
            name = "left"
            edge = "top"
            align = "start"
            length = { fraction = 0.3 }
            size = 36

            [[panel]]
            name = "clock"
            edge = "top"
            align = "center"
            length = { pixels = 240 }
            size = 36
            "#,
        );

        assert_eq!(file.panel.len(), 2);
        assert_eq!(file.panel[0].edge, Edge::Top);
        assert_eq!(file.panel[0].length, Length::Fraction(0.3));
        assert_eq!(file.panel[1].align, Align::Center);
        assert_eq!(file.panel[1].length, Length::Pixels(240));
    }

    #[test]
    fn empty_files_give_default() {
        assert!(panels("").panel.is_empty());
        assert_eq!(
            parse::<General>("", Path::new(GENERAL_FILE)).expect("parses"),
            General::default()
        );
    }

    #[test]
    fn general_rejects_unknown() {
        assert!(parse::<General>(r#"them = "x""#, Path::new(GENERAL_FILE)).is_err());
    }

    #[test]
    fn parse_error_names_path() {
        let error = parse::<General>("theme = ", Path::new(GENERAL_FILE)).expect_err("fails");
        assert!(matches!(error, Error::Parse { ref path, .. } if path.ends_with(GENERAL_FILE)));
        assert!(error.to_string().contains(GENERAL_FILE));
    }

    #[test]
    fn templates_are_valid() {
        assert_eq!(
            parse::<General>(GENERAL_TEMPLATE, Path::new(GENERAL_FILE)).expect("parses"),
            General::default()
        );
        assert!(panels(PANELS_TEMPLATE).panel.is_empty());
    }
}
