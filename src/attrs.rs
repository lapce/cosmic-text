// SPDX-License-Identifier: MIT OR Apache-2.0

#[cfg(not(feature = "std"))]
use alloc::{
    string::{String, ToString},
    vec::Vec,
};
use core::ops::Range;
use peniko::Color;

pub use fontdb::{Family, Stretch, Style, Weight};
use rangemap::RangeMap;

static DEFAULT_FAMILY: [FamilyOwned; 1] = [FamilyOwned::SansSerif];

/// An owned version of [`Family`]
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum FamilyOwned {
    Name(String),
    Serif,
    SansSerif,
    Cursive,
    Fantasy,
    Monospace,
}

impl FamilyOwned {
    pub fn new(family: Family) -> Self {
        match family {
            Family::Name(name) => FamilyOwned::Name(name.to_string()),
            Family::Serif => FamilyOwned::Serif,
            Family::SansSerif => FamilyOwned::SansSerif,
            Family::Cursive => FamilyOwned::Cursive,
            Family::Fantasy => FamilyOwned::Fantasy,
            Family::Monospace => FamilyOwned::Monospace,
        }
    }

    pub fn as_family(&self) -> Family {
        match self {
            FamilyOwned::Name(name) => Family::Name(name),
            FamilyOwned::Serif => Family::Serif,
            FamilyOwned::SansSerif => Family::SansSerif,
            FamilyOwned::Cursive => Family::Cursive,
            FamilyOwned::Fantasy => Family::Fantasy,
            FamilyOwned::Monospace => Family::Monospace,
        }
    }

    pub fn parse_list<'a>(s: &'a str) -> impl Iterator<Item = FamilyOwned> + 'a + Clone {
        ParseList {
            source: s.as_bytes(),
            len: s.len(),
            pos: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LineHeightValue {
    Normal(f32),
    Px(f32),
}

/// Font attributes
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct FontAttrs {
    pub family: Vec<FamilyOwned>,
    pub monospaced: bool,
    pub stretch: Stretch,
    pub style: Style,
    pub weight: Weight,
}

/// Text attributes
#[derive(Clone, Copy, Debug)]
pub struct Attrs<'a> {
    pub color: Color,
    pub family: &'a [FamilyOwned],
    pub monospaced: bool,
    pub stretch: Stretch,
    pub style: Style,
    pub weight: Weight,
    pub font_size: f32,
    pub line_height: LineHeightValue,
    pub metadata: usize,
}

impl<'a> PartialEq for Attrs<'a> {
    fn eq(&self, other: &Self) -> bool {
        self.color == other.color
            && self.family == other.family
            && self.monospaced == other.monospaced
            && self.stretch == other.stretch
            && self.style == other.style
            && self.weight == other.weight
            && self.metadata == other.metadata
            && self.line_height == other.line_height
            && nearly_eq(self.font_size, other.font_size)
    }
}

impl<'a> Eq for Attrs<'a> {}

impl<'a> Attrs<'a> {
    /// Create a new set of attributes with sane defaults
    ///
    /// This defaults to a regular Sans-Serif font.
    pub fn new() -> Self {
        Self {
            color: Color::BLACK,
            family: &DEFAULT_FAMILY,
            monospaced: false,
            stretch: Stretch::Normal,
            style: Style::Normal,
            weight: Weight::NORMAL,
            font_size: 16.0,
            line_height: LineHeightValue::Normal(1.0),
            metadata: 0,
        }
    }

    /// Set [Color]
    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    /// Set [Family]
    pub fn family(mut self, family: &'a [FamilyOwned]) -> Self {
        self.family = family;
        self
    }

    /// Set monospaced
    pub fn monospaced(mut self, monospaced: bool) -> Self {
        self.monospaced = monospaced;
        self
    }

    /// Set [Stretch]
    pub fn stretch(mut self, stretch: Stretch) -> Self {
        self.stretch = stretch;
        self
    }

    /// Set [Style]
    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    /// Set [Weight]
    pub fn weight(mut self, weight: Weight) -> Self {
        self.weight = weight;
        self
    }

    /// Set Weight from u16 value
    pub fn raw_weight(mut self, weight: u16) -> Self {
        self.weight = Weight(weight);
        self
    }

    /// Set font size
    pub fn font_size(mut self, font_size: f32) -> Self {
        self.font_size = font_size;
        self
    }

    /// Set line height
    pub fn line_height(mut self, line_height: LineHeightValue) -> Self {
        self.line_height = line_height;
        self
    }

    /// Set metadata
    pub fn metadata(mut self, metadata: usize) -> Self {
        self.metadata = metadata;
        self
    }

    /// Check if font matches
    pub fn matches(&self, face: &fontdb::FaceInfo) -> bool {
        //TODO: smarter way of including emoji
        face.post_script_name.contains("Emoji")
            || (face.style == self.style
                && face.weight == self.weight
                && face.stretch == self.stretch
                && face.monospaced == self.monospaced)
    }

    /// Check if this set of attributes can be shaped with another
    pub fn compatible(&self, other: &Self) -> bool {
        self.family == other.family
            && self.monospaced == other.monospaced
            && self.stretch == other.stretch
            && self.style == other.style
            && self.weight == other.weight
    }
}

/// An owned version of [`Attrs`]
#[derive(Clone, Debug)]
pub struct AttrsOwned {
    pub color: Color,
    pub family_owned: Vec<FamilyOwned>,
    pub monospaced: bool,
    pub stretch: Stretch,
    pub style: Style,
    pub weight: Weight,
    pub metadata: usize,
    pub font_size: f32,
    pub line_height: LineHeightValue,
}

impl PartialEq for AttrsOwned {
    fn eq(&self, other: &Self) -> bool {
        self.color == other.color
            && self.family_owned == other.family_owned
            && self.monospaced == other.monospaced
            && self.stretch == other.stretch
            && self.style == other.style
            && self.weight == other.weight
            && self.metadata == other.metadata
            && nearly_eq(self.font_size, other.font_size)
            && self.line_height == other.line_height
    }
}

impl Eq for AttrsOwned {}

impl AttrsOwned {
    pub fn new(attrs: Attrs) -> Self {
        Self {
            color: attrs.color,
            family_owned: attrs.family.to_vec(),
            monospaced: attrs.monospaced,
            stretch: attrs.stretch,
            style: attrs.style,
            weight: attrs.weight,
            metadata: attrs.metadata,
            font_size: attrs.font_size,
            line_height: attrs.line_height,
        }
    }

    pub fn as_attrs(&self) -> Attrs {
        Attrs {
            color: self.color,
            family: &self.family_owned,
            monospaced: self.monospaced,
            stretch: self.stretch,
            style: self.style,
            weight: self.weight,
            metadata: self.metadata,
            font_size: self.font_size,
            line_height: self.line_height,
        }
    }
}

/// List of text attributes to apply to a line
//TODO: have this clean up the spans when changes are made
#[derive(PartialEq, Clone)]
pub struct AttrsList {
    defaults: AttrsOwned,
    spans: RangeMap<usize, AttrsOwned>,
}

impl AttrsList {
    /// Create a new attributes list with a set of default [Attrs]
    pub fn new(defaults: Attrs) -> Self {
        Self {
            defaults: AttrsOwned::new(defaults),
            spans: RangeMap::new(),
        }
    }

    /// Get the default [Attrs]
    pub fn defaults(&self) -> Attrs {
        self.defaults.as_attrs()
    }

    /// Get the current attribute spans
    pub fn spans(&self) -> Vec<(&Range<usize>, &AttrsOwned)> {
        self.spans.iter().collect()
    }

    /// Clear the current attribute spans
    pub fn clear_spans(&mut self) {
        self.spans.clear();
    }

    /// Add an attribute span, removes any previous matching parts of spans
    pub fn add_span(&mut self, range: Range<usize>, attrs: Attrs) {
        //do not support 1..1 even if by accident.
        if range.start == range.end {
            return;
        }

        self.spans.insert(range, AttrsOwned::new(attrs));
    }

    /// Get the attribute span for an index
    ///
    /// This returns a span that contains the index
    pub fn get_span(&self, index: usize) -> Attrs {
        self.spans
            .get(&index)
            .map(|v| v.as_attrs())
            .unwrap_or(self.defaults.as_attrs())
    }

    /// Split attributes list at an offset
    pub fn split_off(&mut self, index: usize) -> Self {
        let mut new = Self::new(self.defaults.as_attrs());
        let mut removes = Vec::new();

        //get the keys we need to remove or fix.
        for span in self.spans.iter() {
            if span.0.end <= index {
                continue;
            } else if span.0.start >= index {
                removes.push((span.0.clone(), false));
            } else {
                removes.push((span.0.clone(), true));
            }
        }

        for (key, resize) in removes {
            let (range, attrs) = self
                .spans
                .get_key_value(&key.start)
                .map(|v| (v.0.clone(), v.1.clone()))
                .expect("attrs span not found");
            self.spans.remove(key);

            if resize {
                new.spans.insert(0..range.end - index, attrs.clone());
                self.spans.insert(range.start..index, attrs);
            } else {
                new.spans
                    .insert(range.start - index..range.end - index, attrs);
            }
        }
        new
    }
}

pub fn nearly_eq(x: f32, y: f32) -> bool {
    (x - y).abs() < f32::EPSILON
}

#[derive(Clone)]
struct ParseList<'a> {
    source: &'a [u8],
    len: usize,
    pos: usize,
}

impl<'a> Iterator for ParseList<'a> {
    type Item = FamilyOwned;

    fn next(&mut self) -> Option<Self::Item> {
        let mut quote = None;
        let mut pos = self.pos;
        while pos < self.len && {
            let ch = self.source[pos];
            ch.is_ascii_whitespace() || ch == b','
        } {
            pos += 1;
        }
        self.pos = pos;
        if pos >= self.len {
            return None;
        }
        let first = self.source[pos];
        let mut start = pos;
        match first {
            b'"' | b'\'' => {
                quote = Some(first);
                pos += 1;
                start += 1;
            }
            _ => {}
        }
        if let Some(quote) = quote {
            while pos < self.len {
                if self.source[pos] == quote {
                    self.pos = pos + 1;
                    return Some(FamilyOwned::Name(
                        core::str::from_utf8(self.source.get(start..pos)?)
                            .ok()?
                            .trim()
                            .to_string(),
                    ));
                }
                pos += 1;
            }
            self.pos = pos;
            return Some(FamilyOwned::Name(
                core::str::from_utf8(self.source.get(start..pos)?)
                    .ok()?
                    .trim()
                    .to_string(),
            ));
        }
        let mut end = start;
        while pos < self.len {
            if self.source[pos] == b',' {
                pos += 1;
                break;
            }
            pos += 1;
            end += 1;
        }
        self.pos = pos;
        let name = core::str::from_utf8(self.source.get(start..end)?)
            .ok()?
            .trim();
        Some(match name.to_lowercase().as_str() {
            "serif" => FamilyOwned::Serif,
            "sans-serif" => FamilyOwned::SansSerif,
            "monospace" => FamilyOwned::Monospace,
            "cursive" => FamilyOwned::Cursive,
            "fantasy" => FamilyOwned::Fantasy,
            _ => FamilyOwned::Name(name.to_string()),
        })
    }
}
