// SPDX-License-Identifier: MIT OR Apache-2.0

use std::{collections::HashMap, sync::Arc};

use fontdb::{Family, Query, Stretch, Style, Weight};
use once_cell::sync::Lazy;
use parking_lot::RwLock;

use crate::{Attrs, FamilyOwned, Font, FontAttrs};

pub static FONT_SYSTEM: Lazy<FontSystem> = Lazy::new(FontSystem::new);

#[allow(clippy::missing_errors_doc)]
pub fn load_font_file<P: AsRef<std::path::Path>>(path: P) -> Result<(), std::io::Error> {
    FONT_SYSTEM.db.write().load_font_file(path)
}
pub fn load_fonts_dir<P: AsRef<std::path::Path>>(path: P) {
    FONT_SYSTEM.db.write().load_fonts_dir(path);
}
pub fn load_font_data(data: Vec<u8>) {
    FONT_SYSTEM.db.write().load_font_data(data);
}

/// Access system fonts
pub struct FontSystem {
    locale: String,
    db: RwLock<fontdb::Database>,
    font_cache: RwLock<HashMap<fontdb::ID, Option<Arc<Font>>>>,
    quey_cache: RwLock<HashMap<FontAttrs, Option<fontdb::ID>>>,
    monospace_cache: RwLock<HashMap<(Style, Weight, Stretch), Option<fontdb::ID>>>,
}

impl FontSystem {
    /// Create a new [`FontSystem`], that allows access to any installed system fonts
    ///
    /// # Timing
    ///
    /// This function takes some time to run. On the release build, it can take up to a second,
    /// while debug builds can take up to ten times longer. For this reason, it should only be
    /// called once, and the resulting [`FontSystem`] should be shared.
    pub fn new() -> Self {
        Self::new_with_fonts(std::iter::empty())
    }

    pub fn new_with_fonts(fonts: impl Iterator<Item = fontdb::Source>) -> Self {
        let locale = sys_locale::get_locale().unwrap_or_else(|| {
            log::warn!("failed to get system locale, falling back to en-US");
            String::from("en-US")
        });
        log::debug!("Locale: {}", locale);

        let mut db = fontdb::Database::new();
        {
            db.set_monospace_family("Fira Mono");
            db.set_sans_serif_family("Fira Sans");
            db.set_serif_family("DejaVu Serif");

            #[cfg(not(target_arch = "wasm32"))]
            let now = std::time::Instant::now();

            #[cfg(target_os = "redox")]
            db.load_fonts_dir("/ui/fonts");

            db.load_system_fonts();

            for source in fonts {
                db.load_font_source(source);
            }

            #[cfg(not(target_arch = "wasm32"))]
            log::info!(
                "Parsed {} font faces in {}ms.",
                db.len(),
                now.elapsed().as_millis()
            );
        }

        Self::new_with_locale_and_db(locale, db)
    }

    /// Create a new [`FontSystem`], manually specifying the current locale and font database.
    pub fn new_with_locale_and_db(locale: String, db: fontdb::Database) -> Self {
        Self {
            locale,
            db: RwLock::new(db),
            font_cache: RwLock::new(HashMap::new()),
            quey_cache: RwLock::new(HashMap::new()),
            monospace_cache: RwLock::new(HashMap::new()),
        }
    }

    pub fn locale(&self) -> &str {
        &self.locale
    }

    pub fn get_font(&self, id: fontdb::ID) -> Option<Arc<Font>> {
        if let Some(f) = self.font_cache.read().get(&id) {
            return f.clone();
        }
        let mut font_cache = self.font_cache.write();
        font_cache
            .entry(id)
            .or_insert_with(|| {
                unsafe {
                    self.db.write().make_shared_face_data(id);
                }
                let db = self.db.read();
                let face = db.face(id)?;
                match Font::new(face) {
                    Some(font) => Some(Arc::new(font)),
                    None => {
                        log::warn!("failed to load font '{}'", face.post_script_name);
                        None
                    }
                }
            })
            .clone()
    }

    pub fn query(&self, family: &[FamilyOwned], attrs: Attrs) -> Option<fontdb::ID> {
        let font_attrs = FontAttrs {
            family: family.to_vec(),
            monospaced: attrs.monospaced,
            stretch: attrs.stretch,
            style: attrs.style,
            weight: attrs.weight,
        };
        if let Some(f) = self.quey_cache.read().get(&font_attrs) {
            return *f;
        }

        *self
            .quey_cache
            .write()
            .entry(font_attrs)
            .or_insert_with(|| {
                let family: Vec<Family> = family.iter().map(|f| f.as_family()).collect();
                self.db.read().query(&Query {
                    families: &family,
                    style: attrs.style,
                    weight: attrs.weight,
                    stretch: attrs.stretch,
                })
            })
    }

    pub fn query_monospace(&self, attrs: &Attrs) -> Option<fontdb::ID> {
        let key = (attrs.style, attrs.weight, attrs.stretch);
        if let Some(f) = self.monospace_cache.read().get(&key) {
            return *f;
        }

        for face in self.db.read().faces() {
            if face.monospaced
                && face.weight == attrs.weight
                && face.stretch == attrs.stretch
                && face.style == attrs.style
            {
                self.monospace_cache.write().insert(key, Some(face.id));
                return Some(face.id);
            }
        }
        self.monospace_cache.write().insert(key, None);
        None
    }

    pub fn face_name(&self, id: fontdb::ID) -> String {
        if let Some(face) = self.db.read().face(id) {
            if let Some((name, _)) = face.families.first() {
                name.clone()
            } else {
                face.post_script_name.clone()
            }
        } else {
            "invalid font id".to_string()
        }
    }
}
