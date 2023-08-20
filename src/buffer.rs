// SPDX-License-Identifier: MIT OR Apache-2.0

#[cfg(not(feature = "std"))]
use alloc::{
    string::{String, ToString},
    vec::Vec,
};
use core::{cmp, fmt};
use peniko::kurbo::{Point, Size};
use unicode_segmentation::UnicodeSegmentation;

use crate::{Attrs, AttrsList, LayoutGlyph, LayoutLine, ShapeLine, TextLayoutLine, Wrap};
#[cfg(feature = "swash")]
use peniko::Color;

pub struct HitPoint {
    /// Text line the cursor is on
    pub line: usize,
    /// First-byte-index of glyph at cursor (will insert behind this glyph)
    pub index: usize,
    /// Whether or not the point was inside the bounds of the layout object.
    ///
    /// A click outside the layout object will still resolve to a position in the
    /// text; for instance a click to the right edge of a line will resolve to the
    /// end of that line, and a click below the last line will resolve to a
    /// position in that line.
    pub is_inside: bool,
}

pub struct HitPosition {
    /// Text line the cursor is on
    pub line: usize,
    /// Point of the cursor
    pub point: Point,
    /// ascent of glyph
    pub glyph_ascent: f64,
    /// descent of glyph
    pub glyph_descent: f64,
}

/// Current cursor location
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd)]
pub struct Cursor {
    /// Text line the cursor is on
    pub line: usize,
    /// First-byte-index of glyph at cursor (will insert behind this glyph)
    pub index: usize,
    /// Whether to associate the cursor with the run before it or the run after it if placed at the
    /// boundary between two runs
    pub affinity: Affinity,
}

impl Cursor {
    /// Create a new cursor
    pub const fn new(line: usize, index: usize) -> Self {
        Self::new_with_affinity(line, index, Affinity::Before)
    }

    /// Create a new cursor, specifying the affinity
    pub const fn new_with_affinity(line: usize, index: usize, affinity: Affinity) -> Self {
        Self {
            line,
            index,
            affinity,
        }
    }
}

/// Whether to associate cursors placed at a boundary between runs with the run before or after it.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Affinity {
    Before,
    After,
}

impl Affinity {
    pub fn before(&self) -> bool {
        *self == Self::Before
    }

    pub fn after(&self) -> bool {
        *self == Self::After
    }

    pub fn from_before(before: bool) -> Self {
        if before {
            Self::Before
        } else {
            Self::After
        }
    }

    pub fn from_after(after: bool) -> Self {
        if after {
            Self::After
        } else {
            Self::Before
        }
    }
}

impl Default for Affinity {
    fn default() -> Self {
        Affinity::Before
    }
}

/// The position of a cursor within a [`Buffer`].
pub struct LayoutCursor {
    pub line: usize,
    pub layout: usize,
    pub glyph: usize,
}

impl LayoutCursor {
    pub fn new(line: usize, layout: usize, glyph: usize) -> Self {
        Self {
            line,
            layout,
            glyph,
        }
    }
}

/// A line of visible text for rendering
pub struct LayoutRun<'a> {
    /// The index of the original text line
    pub line_i: usize,
    /// The original text line
    pub text: &'a str,
    /// True if the original paragraph direction is RTL
    pub rtl: bool,
    /// The array of layout glyphs to draw
    pub glyphs: &'a [LayoutGlyph],
    /// Y offset of line
    pub line_y: f32,
    /// width of line
    pub line_w: f32,
    /// height of this line
    pub line_height: f32,
    /// ascent of glyph
    pub glyph_ascent: f32,
    /// descent of glyph
    pub glyph_descent: f32,
}

impl<'a> LayoutRun<'a> {
    /// Return the pixel span `Some((x_left, x_width))` of the highlighted area between `cursor_start`
    /// and `cursor_end` within this run, or None if the cursor range does not intersect this run.
    /// This may return widths of zero if `cursor_start == cursor_end`, if the run is empty, or if the
    /// region's left start boundary is the same as the cursor's end boundary or vice versa.
    pub fn highlight(&self, cursor_start: Cursor, cursor_end: Cursor) -> Option<(f32, f32)> {
        let mut x_start = None;
        let mut x_end = None;
        let rtl_factor = if self.rtl { 1. } else { 0. };
        let ltr_factor = 1. - rtl_factor;
        for glyph in self.glyphs.iter() {
            let cursor = self.cursor_from_glyph_left(glyph);
            if cursor >= cursor_start && cursor <= cursor_end {
                if x_start.is_none() {
                    x_start = Some(glyph.x + glyph.w * rtl_factor);
                }
                x_end = Some(glyph.x + glyph.w * rtl_factor);
            }
            let cursor = self.cursor_from_glyph_right(glyph);
            if cursor >= cursor_start && cursor <= cursor_end {
                if x_start.is_none() {
                    x_start = Some(glyph.x + glyph.w * ltr_factor);
                }
                x_end = Some(glyph.x + glyph.w * ltr_factor);
            }
        }
        if let Some(x_start) = x_start {
            let x_end = x_end.expect("end of cursor not found");
            let (x_start, x_end) = if x_start < x_end {
                (x_start, x_end)
            } else {
                (x_end, x_start)
            };
            Some((x_start, x_end - x_start))
        } else {
            None
        }
    }

    fn cursor_from_glyph_left(&self, glyph: &LayoutGlyph) -> Cursor {
        if self.rtl {
            Cursor::new_with_affinity(self.line_i, glyph.end, Affinity::Before)
        } else {
            Cursor::new_with_affinity(self.line_i, glyph.start, Affinity::After)
        }
    }

    fn cursor_from_glyph_right(&self, glyph: &LayoutGlyph) -> Cursor {
        if self.rtl {
            Cursor::new_with_affinity(self.line_i, glyph.start, Affinity::After)
        } else {
            Cursor::new_with_affinity(self.line_i, glyph.end, Affinity::Before)
        }
    }
}

/// An iterator of visible text lines, see [`LayoutRun`]
pub struct LayoutRunIter<'b> {
    buffer: &'b TextLayout,
    line_i: usize,
    layout_i: usize,
    remaining_len: usize,
    line_y: f32,
    total_layout: i32,
}

impl<'b> LayoutRunIter<'b> {
    pub fn new(buffer: &'b TextLayout) -> Self {
        let total_layout_lines: usize = buffer
            .lines
            .iter()
            .map(|line| {
                line.layout_opt()
                    .as_ref()
                    .map(|layout| layout.len())
                    .unwrap_or_default()
            })
            .sum();
        let top_cropped_layout_lines =
            total_layout_lines.saturating_sub(buffer.scroll.try_into().unwrap_or_default());
        let maximum_lines = i32::MAX;
        let bottom_cropped_layout_lines =
            if top_cropped_layout_lines > maximum_lines.try_into().unwrap_or_default() {
                maximum_lines.try_into().unwrap_or_default()
            } else {
                top_cropped_layout_lines
            };

        Self {
            buffer,
            line_i: 0,
            layout_i: 0,
            remaining_len: bottom_cropped_layout_lines,
            line_y: 0.0,
            total_layout: 0,
        }
    }
}

impl<'b> Iterator for LayoutRunIter<'b> {
    type Item = LayoutRun<'b>;

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining_len, Some(self.remaining_len))
    }

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(line) = self.buffer.lines.get(self.line_i) {
            let shape = line.shape_opt().as_ref()?;
            let layout = line.layout_opt().as_ref()?;
            while let Some(layout_line) = layout.get(self.layout_i) {
                self.layout_i += 1;

                let scrolled = self.total_layout < self.buffer.scroll;
                self.total_layout += 1;
                if scrolled {
                    continue;
                }

                let line_height = layout_line.line_ascent + layout_line.line_descent;
                self.line_y += line_height;
                if self.line_y > self.buffer.height {
                    return None;
                }

                let offset =
                    (line_height - (layout_line.glyph_ascent + layout_line.glyph_descent)) / 2.0;

                self.remaining_len -= 1;
                return Some(LayoutRun {
                    line_i: self.line_i,
                    text: line.text(),
                    rtl: shape.rtl,
                    glyphs: &layout_line.glyphs,
                    line_y: self.line_y - offset - layout_line.glyph_descent,
                    line_w: layout_line.w,
                    glyph_ascent: layout_line.glyph_ascent,
                    glyph_descent: layout_line.glyph_descent,
                    line_height,
                });
            }
            self.line_i += 1;
            self.layout_i = 0;
        }

        None
    }
}

impl<'b> ExactSizeIterator for LayoutRunIter<'b> {}

/// Metrics of text
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Metrics {
    /// Font size in pixels
    pub font_size: f32,
    /// Line height in pixels
    pub line_height: f32,
}

impl Metrics {
    pub const fn new(font_size: f32, line_height: f32) -> Self {
        Self {
            font_size,
            line_height,
        }
    }

    pub fn scale(self, scale: f32) -> Self {
        Self {
            font_size: self.font_size * scale,
            line_height: self.line_height * scale,
        }
    }
}

impl fmt::Display for Metrics {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}px / {}px", self.font_size, self.line_height)
    }
}

/// A buffer of text that is shaped and laid out
#[derive(Clone)]
pub struct TextLayout {
    /// [BufferLine]s (or paragraphs) of text in the buffer
    pub lines: Vec<TextLayoutLine>,
    width: f32,
    height: f32,
    scroll: i32,
    /// True if a redraw is requires. Set to false after processing
    redraw: bool,
    wrap: Wrap,
}

impl TextLayout {
    /// Create a new [`Buffer`] with the provided [`FontSystem`] and [`Metrics`]
    ///
    /// # Panics
    ///
    /// Will panic if `metrics.line_height` is zero.
    pub fn new() -> Self {
        let mut buffer = Self {
            lines: Vec::new(),
            width: f32::MAX,
            height: f32::MAX,
            scroll: 0,
            redraw: false,
            wrap: Wrap::Word,
        };
        buffer.set_text("", AttrsList::new(Attrs::new()));
        buffer
    }

    fn relayout(&mut self) {
        #[cfg(all(feature = "std", not(target_arch = "wasm32")))]
        let instant = std::time::Instant::now();

        for line in &mut self.lines {
            if line.shape_opt().is_some() {
                line.reset_layout();
                line.layout(self.width, self.wrap);
            }
        }

        self.redraw = true;

        #[cfg(all(feature = "std", not(target_arch = "wasm32")))]
        log::debug!("relayout: {:?}", instant.elapsed());
    }

    /// Pre-shape lines in the buffer, up to `lines`, return actual number of layout lines
    pub fn shape_until(&mut self, lines: i32) -> i32 {
        #[cfg(all(feature = "std", not(target_arch = "wasm32")))]
        let instant = std::time::Instant::now();

        let mut reshaped = 0;
        let mut total_layout = 0;
        for line in &mut self.lines {
            if total_layout >= lines {
                break;
            }

            if line.shape_opt().is_none() {
                reshaped += 1;
            }
            let layout = line.layout(self.width, self.wrap);
            total_layout += layout.len() as i32;
        }

        if reshaped > 0 {
            #[cfg(all(feature = "std", not(target_arch = "wasm32")))]
            log::debug!("shape_until {}: {:?}", reshaped, instant.elapsed());
            self.redraw = true;
        }

        total_layout
    }

    /// Shape lines until cursor, also scrolling to include cursor in view
    pub fn shape_until_cursor(&mut self, cursor: Cursor) {
        #[cfg(all(feature = "std", not(target_arch = "wasm32")))]
        let instant = std::time::Instant::now();

        let mut reshaped = 0;
        let mut layout_i = 0;
        for (line_i, line) in self.lines.iter_mut().enumerate() {
            if line_i > cursor.line {
                break;
            }

            if line.shape_opt().is_none() {
                reshaped += 1;
            }
            let layout = line.layout(self.width, self.wrap);
            if line_i == cursor.line {
                let layout_cursor = self.layout_cursor(&cursor);
                layout_i += layout_cursor.layout as i32;
                break;
            } else {
                layout_i += layout.len() as i32;
            }
        }

        if reshaped > 0 {
            #[cfg(all(feature = "std", not(target_arch = "wasm32")))]
            log::debug!("shape_until_cursor {}: {:?}", reshaped, instant.elapsed());
            self.redraw = true;
        }

        let lines = i32::MAX;
        if layout_i < self.scroll {
            self.scroll = layout_i;
        } else if layout_i >= self.scroll + lines {
            self.scroll = layout_i - (lines - 1);
        }

        self.shape_until_scroll();
    }

    /// Shape lines until scroll
    pub fn shape_until_scroll(&mut self) {
        let lines = i32::MAX;

        let scroll_end = self.scroll + lines;
        let total_layout = self.shape_until(scroll_end);

        self.scroll = cmp::max(0, cmp::min(total_layout - (lines - 1), self.scroll));
    }

    pub fn layout_cursor(&self, cursor: &Cursor) -> LayoutCursor {
        let line = &self.lines[cursor.line];

        //TODO: ensure layout is done?
        let layout = line.layout_opt().as_ref().expect("layout not found");
        for (layout_i, layout_line) in layout.iter().enumerate() {
            for (glyph_i, glyph) in layout_line.glyphs.iter().enumerate() {
                let cursor_end =
                    Cursor::new_with_affinity(cursor.line, glyph.end, Affinity::Before);
                let cursor_start =
                    Cursor::new_with_affinity(cursor.line, glyph.start, Affinity::After);
                let (cursor_left, cursor_right) = if glyph.level.is_ltr() {
                    (cursor_start, cursor_end)
                } else {
                    (cursor_end, cursor_start)
                };
                if *cursor == cursor_left {
                    return LayoutCursor::new(cursor.line, layout_i, glyph_i);
                }
                if *cursor == cursor_right {
                    return LayoutCursor::new(cursor.line, layout_i, glyph_i + 1);
                }
            }
        }

        // Fall back to start of line
        //TODO: should this be the end of the line?
        LayoutCursor::new(cursor.line, 0, 0)
    }

    /// Shape the provided line index and return the result
    pub fn line_shape(&mut self, line_i: usize) -> Option<&ShapeLine> {
        let line = self.lines.get_mut(line_i)?;
        Some(line.shape())
    }

    /// Lay out the provided line index and return the result
    pub fn line_layout(&mut self, line_i: usize) -> Option<&[LayoutLine]> {
        let line = self.lines.get_mut(line_i)?;
        Some(line.layout(self.width, self.wrap))
    }

    /// Get the current [`Wrap`]
    pub fn wrap(&self) -> Wrap {
        self.wrap
    }

    /// Set the current [`Wrap`]
    pub fn set_wrap(&mut self, wrap: Wrap) {
        if wrap != self.wrap {
            self.wrap = wrap;
            self.relayout();
            self.shape_until_scroll();
        }
    }

    pub fn size(&self) -> Size {
        self.layout_runs()
            .fold(Size::new(0.0, 0.0), |mut size, run| {
                let new_width = run.line_w as f64;
                if new_width > size.width {
                    size.width = new_width;
                }

                size.height += run.line_height as f64;

                size
            })
    }
    /// Set the current buffer dimensions
    pub fn set_size(&mut self, width: f32, height: f32) {
        let clamped_width = width.max(0.0);
        let clamped_height = height.max(0.0);

        if clamped_width != self.width || clamped_height != self.height {
            self.width = clamped_width;
            self.height = clamped_height;
            self.relayout();
            self.shape_until_scroll();
        }
    }

    /// Get the current scroll location
    pub fn scroll(&self) -> i32 {
        self.scroll
    }

    /// Set the current scroll location
    pub fn set_scroll(&mut self, scroll: i32) {
        if scroll != self.scroll {
            self.scroll = scroll;
            self.redraw = true;
        }
    }

    /// Set text of buffer, using provided attributes for each line by default
    pub fn set_text(&mut self, text: &str, attrs: AttrsList) {
        self.lines.clear();
        let mut attrs = attrs;
        let mut start_index = 0;
        for line in text.split_terminator('\n') {
            let l = line.len();
            let (line, had_r) = if l > 0 && line.as_bytes()[l - 1] == b'\r' {
                (&line[0..l - 1], true)
            } else {
                (line, false)
            };
            let new_attrs = attrs.split_off(line.len() + 1 + if had_r { 1 } else { 0 });
            self.lines.push(TextLayoutLine::new(
                line.to_string(),
                attrs.clone(),
                start_index,
            ));
            attrs = new_attrs;

            start_index += l + 1 + if had_r { 1 } else { 0 };
        }
        // Make sure there is always one line
        if self.lines.is_empty() {
            self.lines
                .push(TextLayoutLine::new(String::new(), attrs, 0));
        }

        self.scroll = 0;

        self.shape_until_scroll();
    }

    /// True if a redraw is needed
    pub fn redraw(&self) -> bool {
        self.redraw
    }

    /// Set redraw needed flag
    pub fn set_redraw(&mut self, redraw: bool) {
        self.redraw = redraw;
    }

    /// Get the visible layout runs for rendering and other tasks
    pub fn layout_runs(&self) -> LayoutRunIter {
        LayoutRunIter::new(self)
    }

    pub fn hit_point(&self, point: Point) -> HitPoint {
        let x = point.x as f32;
        let y = point.y as f32;
        let mut hit_point = HitPoint {
            index: 0,
            line: 0,
            is_inside: false,
        };

        let mut runs = self.layout_runs().peekable();
        let mut first_run = true;
        while let Some(run) = runs.next() {
            let line_y = run.line_y;

            if first_run && y < line_y - run.line_height {
                first_run = false;
                hit_point.line = run.line_i;
                hit_point.index = 0;
            } else if y >= line_y - run.line_height && y < line_y {
                let mut new_cursor_glyph = run.glyphs.len();
                let mut new_cursor_char = 0;

                let mut first_glyph = true;

                'hit: for (glyph_i, glyph) in run.glyphs.iter().enumerate() {
                    if first_glyph {
                        first_glyph = false;
                        if (run.rtl && x > glyph.x) || (!run.rtl && x < 0.0) {
                            new_cursor_glyph = 0;
                            new_cursor_char = 0;
                        }
                    }
                    if x >= glyph.x && x <= glyph.x + glyph.w {
                        new_cursor_glyph = glyph_i;

                        let cluster = &run.text[glyph.start..glyph.end];
                        let total = cluster.grapheme_indices(true).count();
                        let mut egc_x = glyph.x;
                        let egc_w = glyph.w / (total as f32);
                        for (egc_i, egc) in cluster.grapheme_indices(true) {
                            if x >= egc_x && x <= egc_x + egc_w {
                                new_cursor_char = egc_i;

                                let right_half = x >= egc_x + egc_w / 2.0;
                                if right_half != glyph.level.is_rtl() {
                                    // If clicking on last half of glyph, move cursor past glyph
                                    new_cursor_char += egc.len();
                                }
                                break 'hit;
                            }
                            egc_x += egc_w;
                        }

                        let right_half = x >= glyph.x + glyph.w / 2.0;
                        if right_half != glyph.level.is_rtl() {
                            // If clicking on last half of glyph, move cursor past glyph
                            new_cursor_char = cluster.len();
                        }
                        break 'hit;
                    }
                }

                hit_point.line = run.line_i;
                hit_point.index = 0;

                match run.glyphs.get(new_cursor_glyph) {
                    Some(glyph) => {
                        // Position at glyph
                        hit_point.index = glyph.start + new_cursor_char;
                        hit_point.is_inside = true;
                    }
                    None => {
                        if let Some(glyph) = run.glyphs.last() {
                            // Position at end of line
                            hit_point.index = glyph.end;
                        }
                    }
                }

                break;
            } else if runs.peek().is_none() && y > run.line_y {
                let mut new_cursor = Cursor::new(run.line_i, 0);
                if let Some(glyph) = run.glyphs.last() {
                    new_cursor = run.cursor_from_glyph_right(glyph);
                }
                hit_point.line = new_cursor.line;
                hit_point.index = new_cursor.index;
            }
        }

        hit_point
    }

    pub fn line_col_position(&self, line: usize, col: usize) -> HitPosition {
        let mut last_glyph: Option<&LayoutGlyph> = None;
        let mut last_line = 0;
        let mut last_line_y = 0.0;
        let mut last_glyph_ascent = 0.0;
        let mut last_glyph_descent = 0.0;
        for (current_line, run) in self.layout_runs().enumerate() {
            for glyph in run.glyphs {
                if line == run.line_i {
                    if glyph.start > col {
                        return HitPosition {
                            line: last_line,
                            point: Point::new(
                                last_glyph.map(|g| (g.x + g.w) as f64).unwrap_or(0.0),
                                last_line_y as f64,
                            ),
                            glyph_ascent: last_glyph_ascent as f64,
                            glyph_descent: last_glyph_descent as f64,
                        };
                    }
                    if (glyph.start..glyph.end).contains(&col) {
                        return HitPosition {
                            line: current_line,
                            point: Point::new(glyph.x as f64, run.line_y as f64),
                            glyph_ascent: run.glyph_ascent as f64,
                            glyph_descent: run.glyph_descent as f64,
                        };
                    }
                } else if run.line_i > line {
                    return HitPosition {
                        line: last_line,
                        point: Point::new(
                            last_glyph.map(|g| (g.x + g.w) as f64).unwrap_or(0.0),
                            last_line_y as f64,
                        ),
                        glyph_ascent: last_glyph_ascent as f64,
                        glyph_descent: last_glyph_descent as f64,
                    };
                }
                last_glyph = Some(glyph);
            }
            last_line = current_line;
            last_line_y = run.line_y;
            last_glyph_ascent = run.glyph_ascent;
            last_glyph_descent = run.glyph_descent;
        }

        HitPosition {
            line: last_line,
            point: Point::new(
                last_glyph.map(|g| (g.x + g.w) as f64).unwrap_or(0.0),
                last_line_y as f64,
            ),
            glyph_ascent: last_glyph_ascent as f64,
            glyph_descent: last_glyph_descent as f64,
        }
    }

    pub fn hit_position(&self, idx: usize) -> HitPosition {
        let mut last_line = 0;
        let mut last_end: usize = 0;
        let mut offset = 0;
        let mut last_glyph_width = 0.0;
        let mut last_position = HitPosition {
            line: 0,
            point: Point::ZERO,
            glyph_ascent: 0.0,
            glyph_descent: 0.0,
        };
        for (line, run) in self.layout_runs().enumerate() {
            if run.line_i > last_line {
                last_line = run.line_i;
                offset += last_end + 1;
            }
            for glyph in run.glyphs {
                if glyph.start + offset > idx {
                    last_position.point.x += last_glyph_width as f64;
                    return last_position;
                }
                last_end = glyph.end;
                last_glyph_width = glyph.w;
                last_position = HitPosition {
                    line,
                    point: Point::new(glyph.x as f64, run.line_y as f64),
                    glyph_ascent: run.glyph_ascent as f64,
                    glyph_descent: run.glyph_descent as f64,
                };
                if (glyph.start + offset..glyph.end + offset).contains(&idx) {
                    return last_position;
                }
            }
        }

        if idx > 0 {
            last_position.point.x += last_glyph_width as f64;
            return last_position;
        }

        HitPosition {
            line: 0,
            point: Point::ZERO,
            glyph_ascent: 0.0,
            glyph_descent: 0.0,
        }
    }

    /// Convert x, y position to Cursor (hit detection)
    pub fn hit(&self, x: f32, y: f32) -> Option<Cursor> {
        #[cfg(all(feature = "std", not(target_arch = "wasm32")))]
        let instant = std::time::Instant::now();

        let mut new_cursor_opt = None;

        let mut runs = self.layout_runs().peekable();
        let mut first_run = true;
        while let Some(run) = runs.next() {
            let line_y = run.line_y;

            if first_run && y < line_y - run.line_height {
                first_run = false;
                let new_cursor = Cursor::new(run.line_i, 0);
                new_cursor_opt = Some(new_cursor);
            } else if y >= line_y - run.line_height && y < line_y {
                let mut new_cursor_glyph = run.glyphs.len();
                let mut new_cursor_char = 0;
                let mut new_cursor_affinity = Affinity::After;

                let mut first_glyph = true;

                'hit: for (glyph_i, glyph) in run.glyphs.iter().enumerate() {
                    if first_glyph {
                        first_glyph = false;
                        if (run.rtl && x > glyph.x) || (!run.rtl && x < 0.0) {
                            new_cursor_glyph = 0;
                            new_cursor_char = 0;
                        }
                    }
                    if x >= glyph.x && x <= glyph.x + glyph.w {
                        new_cursor_glyph = glyph_i;

                        let cluster = &run.text[glyph.start..glyph.end];
                        let total = cluster.grapheme_indices(true).count();
                        let mut egc_x = glyph.x;
                        let egc_w = glyph.w / (total as f32);
                        for (egc_i, egc) in cluster.grapheme_indices(true) {
                            if x >= egc_x && x <= egc_x + egc_w {
                                new_cursor_char = egc_i;

                                let right_half = x >= egc_x + egc_w / 2.0;
                                if right_half != glyph.level.is_rtl() {
                                    // If clicking on last half of glyph, move cursor past glyph
                                    new_cursor_char += egc.len();
                                    new_cursor_affinity = Affinity::Before;
                                }
                                break 'hit;
                            }
                            egc_x += egc_w;
                        }

                        let right_half = x >= glyph.x + glyph.w / 2.0;
                        if right_half != glyph.level.is_rtl() {
                            // If clicking on last half of glyph, move cursor past glyph
                            new_cursor_char = cluster.len();
                            new_cursor_affinity = Affinity::Before;
                        }
                        break 'hit;
                    }
                }

                let mut new_cursor = Cursor::new(run.line_i, 0);

                match run.glyphs.get(new_cursor_glyph) {
                    Some(glyph) => {
                        // Position at glyph
                        new_cursor.index = glyph.start + new_cursor_char;
                        new_cursor.affinity = new_cursor_affinity;
                    }
                    None => {
                        if let Some(glyph) = run.glyphs.last() {
                            // Position at end of line
                            new_cursor.index = glyph.end;
                            new_cursor.affinity = Affinity::Before;
                        }
                    }
                }

                new_cursor_opt = Some(new_cursor);

                break;
            } else if runs.peek().is_none() && y > run.line_y {
                let mut new_cursor = Cursor::new(run.line_i, 0);
                if let Some(glyph) = run.glyphs.last() {
                    new_cursor = run.cursor_from_glyph_right(glyph);
                }
                new_cursor_opt = Some(new_cursor);
            }
        }

        #[cfg(all(feature = "std", not(target_arch = "wasm32")))]
        log::trace!("click({}, {}): {:?}", x, y, instant.elapsed());

        new_cursor_opt
    }

    /// Draw the buffer
    #[cfg(feature = "swash")]
    pub fn draw<F>(&self, cache: &mut crate::SwashCache, color: Color, mut f: F)
    where
        F: FnMut(i32, i32, u32, u32, Color),
    {
        for run in self.layout_runs() {
            for glyph in run.glyphs.iter() {
                let (cache_key, x_int, y_int) = (glyph.cache_key, glyph.x_int, glyph.y_int);

                let glyph_color = glyph.color;

                cache.with_pixels(cache_key, glyph_color, |x, y, color| {
                    f(x_int + x, run.line_y as i32 + y_int + y, 1, 1, color);
                });
            }
        }
    }
}
