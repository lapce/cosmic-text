// SPDX-License-Identifier: MIT OR Apache-2.0

#[cfg(not(feature = "std"))]
use alloc::collections::BTreeMap as Map;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;
use peniko::Color;
#[cfg(feature = "std")]
use std::collections::HashMap as Map;
use swash::scale::{image::Content, ScaleContext};
use swash::scale::{Render, Source, StrikeWith};
use swash::zeno::{Format, Vector};

use crate::{CacheKey, FontSystem, FONT_SYSTEM};

pub use swash::scale::image::{Content as SwashContent, Image as SwashImage};
pub use swash::zeno::{Command, Placement};

const IS_MACOS: bool = cfg!(target_os = "macos");

fn swash_image(context: &mut ScaleContext, cache_key: CacheKey) -> Option<SwashImage> {
    let font = match FONT_SYSTEM.get_font(cache_key.font_id) {
        Some(some) => some,
        None => {
            log::warn!("did not find font {:?}", cache_key.font_id);
            return None;
        }
    };

    // Build the scaler
    let mut scaler = context
        .builder(font.as_swash())
        .size(cache_key.font_size as f32)
        .hint(!IS_MACOS)
        .build();

    // Compute the fractional offset-- you'll likely want to quantize this
    // in a real renderer
    let offset = Vector::new(cache_key.x_bin.as_float(), cache_key.y_bin.as_float());

    let embolden = if IS_MACOS { 0.2 } else { 0. };
    // Select our source order
    Render::new(&[
        // Color outline with the first palette
        Source::ColorOutline(0),
        // Color bitmap with best fit selection mode
        Source::ColorBitmap(StrikeWith::BestFit),
        // Standard scalable outline
        Source::Outline,
    ])
    // Select a subpixel format
    .format(Format::Alpha)
    // Apply the fractional offset
    .offset(offset)
    .embolden(embolden)
    // Render the image
    .render(&mut scaler, cache_key.glyph_id)
}

fn swash_outline_commands(
    font_system: &mut FontSystem,
    context: &mut ScaleContext,
    cache_key: CacheKey,
) -> Option<Vec<swash::zeno::Command>> {
    use swash::zeno::PathData as _;

    let font = match font_system.get_font(cache_key.font_id) {
        Some(some) => some,
        None => {
            log::warn!("did not find font {:?}", cache_key.font_id);
            return None;
        }
    };

    // Build the scaler
    let mut scaler = context
        .builder(font.as_swash())
        .size(cache_key.font_size as f32)
        .build();

    // Scale the outline
    let outline = scaler
        .scale_outline(cache_key.glyph_id)
        .or_else(|| scaler.scale_color_outline(cache_key.glyph_id))?;

    // Get the path information of the outline
    let path = outline.path();

    // Return the commands
    Some(path.commands().collect())
}

/// Cache for rasterizing with the swash scaler
pub struct SwashCache {
    context: ScaleContext,
    pub image_cache: Map<CacheKey, Option<SwashImage>>,
    pub outline_command_cache: Map<CacheKey, Option<Vec<swash::zeno::Command>>>,
}

impl SwashCache {
    /// Create a new swash cache
    pub fn new() -> Self {
        Self {
            context: ScaleContext::new(),
            image_cache: Map::new(),
            outline_command_cache: Map::new(),
        }
    }

    /// Create a swash Image from a cache key, without caching results
    pub fn get_image_uncached(&mut self, cache_key: CacheKey) -> Option<SwashImage> {
        swash_image(&mut self.context, cache_key)
    }

    /// Create a swash Image from a cache key, caching results
    pub fn get_image(&mut self, cache_key: CacheKey) -> &Option<SwashImage> {
        self.image_cache
            .entry(cache_key)
            .or_insert_with(|| swash_image(&mut self.context, cache_key))
    }

    pub fn get_outline_commands(
        &mut self,
        font_system: &mut FontSystem,
        cache_key: CacheKey,
    ) -> Option<&[swash::zeno::Command]> {
        self.outline_command_cache
            .entry(cache_key)
            .or_insert_with(|| swash_outline_commands(font_system, &mut self.context, cache_key))
            .as_deref()
    }

    /// Enumerate pixels in an Image, use `with_image` for better performance
    pub fn with_pixels<F: FnMut(i32, i32, Color)>(
        &mut self,
        cache_key: CacheKey,
        base: Color,
        mut f: F,
    ) {
        if let Some(image) = self.get_image(cache_key) {
            let x = image.placement.left;
            let y = -image.placement.top;

            match image.content {
                Content::Mask => {
                    let mut i = 0;
                    for off_y in 0..image.placement.height as i32 {
                        for off_x in 0..image.placement.width as i32 {
                            //TODO: blend base alpha?
                            f(x + off_x, y + off_y, Color::BLACK);
                            i += 1;
                        }
                    }
                }
                Content::Color => {
                    let mut i = 0;
                    for off_y in 0..image.placement.height as i32 {
                        for off_x in 0..image.placement.width as i32 {
                            //TODO: blend base alpha?
                            f(
                                x + off_x,
                                y + off_y,
                                Color::rgba8(
                                    image.data[i],
                                    image.data[i + 1],
                                    image.data[i + 2],
                                    image.data[i + 3],
                                ),
                            );
                            i += 4;
                        }
                    }
                }
                Content::SubpixelMask => {
                    log::warn!("TODO: SubpixelMask");
                }
            }
        }
    }
}
