use crate::error::Error;
use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::{self, MapAccess, Unexpected, Visitor},
    ser::SerializeMap,
};
use std::fmt::{self, Display, Formatter};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Edge {
    Top,
    Bottom,
    Left,
    Right,
}

impl Edge {
    // A panel on a horizontal edge spans the output's eidth, a vertical one its
    // height. Overlap between two panels is measured along that axis.
    pub fn is_horizontal(self) -> bool {
        matches!(self, Edge::Top | Edge::Bottom)
    }
}

impl Display for Edge {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let name = match self {
            Edge::Top => "top",
            Edge::Bottom => "bottom",
            Edge::Left => "left",
            Edge::Right => "right",
        };

        f.write_str(name)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Align {
    #[default]
    Start,
    Center,
    End,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Layer {
    Background,
    Bottom,
    #[default]
    Top,
    Overlay,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Visibility {
    // Window laid out around the panel
    #[default]
    Always,
    // Allow panels to overlap windows
    Overlap,
    // Hides when a window would cover it
    Dodge,
    // Off screen until edge is hovered
    AutoHide,
    // Configured; never shown
    Hidden,
}

impl Visibility {
    // Only a panel that is always on screen and not overlapping holds an
    // exclusive zone.
    pub fn reserves_space(self) -> bool {
        matches!(self, Visibility::Always)
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Margin {
    // Distance from the screen edges
    pub outer: u32,
    // Distance from the next panel on the same edge
    pub gap: u32,
}

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub enum Length {
    #[default]
    Fill,
    Pixels(u32),
    Fraction(f32),
}

impl Length {
    pub fn fraction(value: f32) -> Result<Self, Error> {
        if !(0.0..=1.0).contains(&value) {
            return Err(Error::FractionOutOfRange(value));
        }
        Ok(Length::Fraction(value))
    }

    // How many pixels along the edge this occupies on an output of the given size.
    pub fn resolve(self, available: u32) -> u32 {
        match self {
            Length::Fill => available,
            Length::Pixels(pixels) => pixels.min(available),
            Length::Fraction(fraction) => (available as f32 * fraction).round() as u32,
        }
    }
}

impl Serialize for Length {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Length::Fill => serializer.serialize_str("fill"),
            Length::Pixels(pixels) => {
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("pixels", pixels)?;
                map.end()
            }
            Length::Fraction(fraction) => {
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("fraction", fraction)?;
                map.end()
            }
        }
    }
}

impl<'de> Deserialize<'de> for Length {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(LengthVisitor)
    }
}

struct LengthVisitor;

impl<'de> Visitor<'de> for LengthVisitor {
    type Value = Length;

    fn expecting(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(r#""fill", { pixels = <integer> }, or { fraction = <0.0 - 1.0 }"#)
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        match value {
            "fill" => Ok(Length::Fill),
            other => Err(E::invalid_value(Unexpected::Str(other), &self)),
        }
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let Some(key) = map.next_key::<String>()? else {
            return Err(de::Error::custom("length table is empty"));
        };
        let length = match key.as_str() {
            "pixels" => Length::Pixels(map.next_value()?),
            "fraction" => Length::fraction(map.next_value()?).map_err(de::Error::custom)?,
            other => return Err(de::Error::unknown_field(other, &["pixels", "fraction"])),
        };

        if let Some(extra) = map.next_key::<String>()? {
            return Err(de::Error::custom(format!(
                "length takes one of pixels or fraction, found both {key} and {extra}"
            )));
        }

        Ok(length)
    }
}

#[cfg(test)]
mod tests {
    use crate::geometry::Visibility;

    use super::{Align, Edge, Layer, Length, Margin};
    use serde::{Deserialize, Serialize};

    #[derive(Debug, PartialEq, Deserialize, Serialize)]
    struct Holder {
        length: Length,
    }

    fn parse(input: &str) -> Result<Holder, toml::de::Error> {
        toml::from_str(input)
    }

    fn roundtrip(length: Length) -> Length {
        let text = toml::to_string(&Holder { length }).expect("length serializes");
        parse(&text).expect("length reparses").length
    }

    #[test]
    fn fill_parses() {
        assert_eq!(
            parse(r#"length = "fill""#).expect("parses").length,
            Length::Fill
        );
        assert_eq!(roundtrip(Length::Fill), Length::Fill);
    }

    #[test]
    fn pixels_parse() {
        let length = parse("length = { pixels = 240 }").expect("parses").length;
        assert_eq!(length, Length::Pixels(240));
        assert_eq!(roundtrip(length), length);
    }

    #[test]
    fn fraction_parses() {
        let length = parse("length  = { fraction = 0.3 }")
            .expect("parses")
            .length;
        assert_eq!(length, Length::Fraction(0.3));
        assert_eq!(roundtrip(length), length);
    }

    #[test]
    fn fraction_bounds() {
        assert!(parse("length = { fraction = 1.5 }").is_err());
        assert!(parse("length = { fraction = -0.1 }").is_err());
        assert!(parse("length = { fraction = 0.0 }").is_ok());
        assert!(parse("length = { fraction = 1.0 }").is_ok());
    }

    #[test]
    fn length_rejects_two_keys() {
        assert!(parse("length = { pixels = 240, fraction = 0.3 }").is_err());
    }

    #[test]
    fn length_rejects_unknown() {
        assert!(parse(r#"length = "stretch""#).is_err());
        assert!(parse("length = { percent = 30 }").is_err());
        assert!(parse("length = {}").is_err());
    }

    #[test]
    fn length_resolves() {
        assert_eq!(Length::Fill.resolve(1920), 1920);
        assert_eq!(Length::Pixels(240).resolve(1920), 240);
        assert_eq!(Length::Pixels(4000).resolve(1920), 1920);
        assert_eq!(Length::Fraction(0.25).resolve(1920), 480);
    }

    #[test]
    fn edge_orientation() {
        assert!(Edge::Top.is_horizontal());
        assert!(Edge::Bottom.is_horizontal());
        assert!(!Edge::Left.is_horizontal());
        assert!(!Edge::Right.is_horizontal());
    }

    #[test]
    fn edge_spellings() {
        #[derive(Deserialize)]
        struct Holder {
            edge: Edge,
        }

        let holder: Holder = toml::from_str(r#"edge = "bottom""#).expect("parses");

        assert_eq!(holder.edge, Edge::Bottom);
        assert!(toml::from_str::<Holder>(r#"edge = "Top""#).is_err());
        assert!(toml::from_str::<Holder>(r#"edge = "north""#).is_err());
    }

    #[test]
    fn defaults() {
        assert_eq!(Align::default(), Align::Start);
        assert_eq!(Layer::default(), Layer::Top);
        assert_eq!(Margin::default(), Margin { outer: 0, gap: 0 });
        assert_eq!(Visibility::default(), Visibility::Always);
    }

    #[test]
    fn margin_rejects_unknown() {
        #[derive(Deserialize)]
        struct Holder {
            margin: Margin,
        }

        let holder: Holder = toml::from_str("margin = { outer = 8, gap = 4 }").expect("parses");
        assert_eq!(holder.margin, Margin { outer: 8, gap: 4 });
        assert!(toml::from_str::<Holder>("margin = { outer = 8, inner = 8 }").is_err());
    }
}
