use std::{io, path::PathBuf};

use crate::geometry::Edge;
use thiserror::Error;
use toml::de;

#[derive(Debug, Error)]
pub enum Error {
    #[error("fraction {0} is outside 0.0 - 1.0")]
    FractionOutOfRange(f32),
    #[error("panels {first} and {second} overlap on the {edge} edge")]
    PanelOverlap {
        first: String,
        second: String,
        edge: Edge,
    },
    #[error("more than one panel is named {0}")]
    DuplicatePanelName(String),
    #[error("could not read {path}: {source}")]
    Read { path: PathBuf, source: io::Error },
    #[error("could not parse {path}: {source}")]
    Parse { path: PathBuf, source: de::Error },
    #[error("could not write {path}: {source}")]
    Write { path: PathBuf, source: io::Error },
}
