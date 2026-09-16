mod error;
mod geometry;
mod load;
mod panel;

// Re-export
pub use error::Error;
pub use geometry::{Align, Edge, Layer, Length, Margin, Visibility};
pub use load::{Config, General, write_templates_if_missing};
pub use panel::{Modules, Panel, validate_names, validate_panels};
