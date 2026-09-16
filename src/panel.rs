use crate::{
    error::Error,
    geometry::{Align, Edge, Layer, Length, Margin, Visibility},
};
use serde::{Deserialize, Serialize};
use std::ops::Range;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Panel {
    pub name: String,
    pub edge: Edge,
    #[serde(default)]
    pub align: Align,
    #[serde(default)]
    pub length: Length,
    // Thickness perpendicular to the edge
    pub size: u32,
    #[serde(default)]
    pub margin: Margin,
    #[serde(default)]
    pub visibility: Visibility,
    #[serde(default)]
    pub layer: Layer,
    // Empty means every output
    #[serde(default)]
    pub outputs: Vec<String>,
    #[serde(default)]
    pub modules: Modules,
}

impl Panel {
    // Where the panel sits along its edge
    pub fn span(&self, available: u32) -> Range<u32> {
        let outer = self.margin.outer.min(available / 2);
        let usable = available - outer * 2;
        let length = self.length.resolve(usable);
        let start = match self.align {
            Align::Start => outer,
            Align::Center => outer + (usable - length) / 2,
            Align::End => available - outer - length,
        };

        start..start + length
    }

    fn shares_output(&self, other: &Panel) -> bool {
        if self.outputs.is_empty() || other.outputs.is_empty() {
            return true;
        }
        self.outputs.iter().any(|name| other.outputs.contains(name))
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Modules {
    pub start: Vec<String>,
    pub center: Vec<String>,
    pub end: Vec<String>,
}

/// Names have to be unique
pub fn validate_names(panels: &[Panel]) -> Result<(), Error> {
    for (idx, panel) in panels.iter().enumerate() {
        if panels[..idx].iter().any(|other| other.name == panel.name) {
            return Err(Error::DuplicatePanelName(panel.name.clone()));
        }
    }

    Ok(())
}

pub fn validate_panels(panels: &[Panel], width: u32, height: u32) -> Result<(), Error> {
    validate_names(panels)?;

    for (idx, panel) in panels.iter().enumerate() {
        if panel.visibility == Visibility::Hidden {
            continue;
        }

        let available = if panel.edge.is_horizontal() {
            width
        } else {
            height
        };

        for other in &panels[idx + 1..] {
            if other.visibility == Visibility::Hidden
                || panel.edge != other.edge
                || !panel.shares_output(other)
            {
                continue;
            }
            if overlaps(panel, other, available) {
                return Err(Error::PanelOverlap {
                    first: panel.name.clone(),
                    second: other.name.clone(),
                    edge: panel.edge,
                });
            }
        }
    }

    Ok(())
}

// The larger of the two gaps wins.
// Example: One panel requests a gap and gets it, even when its neighbor asks for none.
fn overlaps(first: &Panel, second: &Panel, available: u32) -> bool {
    let gap = first.margin.gap.max(second.margin.gap);
    let left = first.span(available);
    let right = second.span(available);

    left.start < right.end.saturating_add(gap) && right.start < left.end.saturating_add(gap)
}

#[cfg(test)]
mod tests {
    use super::{Modules, Panel, validate_panels};
    use crate::{
        Visibility,
        geometry::{Align, Edge, Layer, Length, Margin},
    };

    const WIDTH: u32 = 1920;
    const HEIGHT: u32 = 1080;

    fn panel(name: &str, edge: Edge, align: Align, length: Length) -> Panel {
        Panel {
            name: name.into(),
            edge,
            align,
            length,
            size: 36,
            margin: Margin::default(),
            visibility: Visibility::Always,
            layer: Layer::default(),
            outputs: Vec::new(),
            modules: Modules::default(),
        }
    }

    fn third(name: &str, align: Align) -> Panel {
        panel(name, Edge::Top, align, Length::Fraction(0.3))
    }

    #[test]
    fn three_panels_fit() {
        let panels = [
            third("left", Align::Start),
            third("middle", Align::Center),
            third("right", Align::End),
        ];

        assert!(validate_panels(&panels, WIDTH, HEIGHT).is_ok());
    }

    #[test]
    fn same_align_overlaps() {
        let panels = [third("one", Align::Start), third("two", Align::Start)];
        assert!(validate_panels(&panels, WIDTH, HEIGHT).is_err());
    }

    #[test]
    fn wide_neighbors_overlap() {
        let panels = [
            panel("left", Edge::Top, Align::Start, Length::Fraction(0.5)),
            panel("middle", Edge::Top, Align::Center, Length::Fraction(0.6)),
        ];

        assert!(validate_panels(&panels, WIDTH, HEIGHT).is_err());
    }

    #[test]
    fn different_edges_ok() {
        let panels = [
            third("top", Align::Start),
            panel("bottom", Edge::Bottom, Align::Start, Length::Fraction(0.3)),
        ];

        assert!(validate_panels(&panels, WIDTH, HEIGHT).is_ok());
    }

    #[test]
    fn different_outputs_ok() {
        let mut first = third("one", Align::Start);
        let mut second = third("two", Align::Start);
        first.outputs = vec!["DP-1".to_string()];
        second.outputs = vec!["HDMI-A-1".to_string()];

        assert!(validate_panels(&[first, second], WIDTH, HEIGHT).is_ok());
    }

    #[test]
    fn unnamed_output_shares_every_output() {
        let mut first = third("one", Align::Start);
        first.outputs = vec!["DP-1".to_string()];

        assert!(validate_panels(&[first, third("two", Align::Start)], WIDTH, HEIGHT).is_err());
    }

    #[test]
    fn gap_forces_separation() {
        let halves = |gap| {
            let mut left = panel("left", Edge::Top, Align::Start, Length::Fraction(0.5));
            let mut right = panel("right", Edge::Top, Align::End, Length::Fraction(0.5));
            left.margin = Margin { outer: 0, gap };
            right.margin = Margin { outer: 0, gap };
            [left, right]
        };

        assert!(validate_panels(&halves(0), WIDTH, HEIGHT).is_ok());
        assert!(validate_panels(&halves(8), WIDTH, HEIGHT).is_err());
    }

    #[test]
    fn duplicate_names() {
        let panels = [third("bar", Align::Start), third("bar", Align::End)];

        assert!(matches!(
            validate_panels(&panels, WIDTH, HEIGHT),
            Err(crate::error::Error::DuplicatePanelName(_))
        ));
    }

    #[test]
    fn vertical_edge_uses_height() {
        let tall = panel("side", Edge::Left, Align::Start, Length::Fraction(1.0));

        assert_eq!(tall.span(HEIGHT), 0..HEIGHT);
        assert!(validate_panels(&[tall], WIDTH, HEIGHT).is_ok());
    }

    #[test]
    fn outer_margin_insets_the_span() {
        let mut bar = panel("bar", Edge::Top, Align::Start, Length::Fill);
        bar.margin = Margin { outer: 8, gap: 0 };

        assert_eq!(bar.span(WIDTH), 8..WIDTH - 8);
    }

    #[test]
    fn panel_defaults() {
        let bar: Panel = toml::from_str(
            r#"
            name = "bar"
            edge = "top"
            size = 36
            "#,
        )
        .expect("parses");

        assert_eq!(bar.align, Align::Start);
        assert_eq!(bar.length, Length::Fill);
        assert_eq!(bar.layer, Layer::Top);
        assert_eq!(bar.margin, Margin::default());
        assert!(bar.visibility == Visibility::default());
        assert!(bar.outputs.is_empty());
        assert_eq!(bar.modules, Modules::default());
    }

    #[test]
    fn panel_rejects_unknown() {
        let text = r#"
            name = "bar"
            edge = "top"
            size = 36
            position = "start"
        "#;

        assert!(toml::from_str::<Panel>(text).is_err());
    }

    #[test]
    fn hidden_never_collides() {
        let mut first = third("one", Align::Start);
        first.visibility = Visibility::Hidden;
        assert!(validate_panels(&[first, third("two", Align::Start)], WIDTH, HEIGHT).is_ok());
    }

    #[test]
    fn transient_panels_still_collide() {
        let mut first = third("one", Align::Start);
        let mut second = third("two", Align::Start);

        first.visibility = Visibility::AutoHide;
        second.visibility = Visibility::Dodge;
        assert!(validate_panels(&[first, second], WIDTH, HEIGHT).is_err());
    }

    #[test]
    fn hidden_still_needs_unique_name() {
        let mut first = third("bar", Align::Start);
        let mut second = third("bar", Align::End);

        first.visibility = Visibility::Hidden;
        second.visibility = Visibility::Hidden;
        assert!(validate_panels(&[first, second], WIDTH, HEIGHT).is_err());
    }
}
