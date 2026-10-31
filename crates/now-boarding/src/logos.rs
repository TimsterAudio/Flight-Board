//! Airline logos the user adds themselves, for personal use: PNG pictures
//! in the logos folder, named by the airline's two-letter code (`QF.png`).
//! The app ships none and fetches none. An airline with a picture there has
//! it on its card on the board; one without gets its colour card.

use std::collections::HashMap;
use std::path::PathBuf;

use egui::{ColorImage, Context, TextureHandle, TextureId, TextureOptions, Vec2};

/// The logos folder: `now-boarding/logos` in the system's data folder.
pub fn folder() -> Option<PathBuf> {
    let base = if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support"))
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
    };
    base.map(|b| b.join("now-boarding").join("logos"))
}

/// The logos found, loaded the first time each is asked for.
#[derive(Default)]
pub struct Logos {
    /// By airline code: the picture, or `None` for one looked for and not
    /// there (or not readable).
    loaded: HashMap<String, Option<TextureHandle>>,
    /// The codes there are pictures for, from the folder's file names.
    found: Vec<String>,
    scanned: bool,
}

impl Logos {
    /// Looks in the folder again, for pictures added or taken away.
    pub fn rescan(&mut self) {
        self.loaded.clear();
        self.found = folder()
            .and_then(|d| std::fs::read_dir(d).ok())
            .map(|entries| {
                let mut codes: Vec<String> = entries
                    .flatten()
                    .filter_map(|e| {
                        let name = e.file_name().to_string_lossy().to_string();
                        let (stem, ext) = name.rsplit_once('.')?;
                        (ext.eq_ignore_ascii_case("png") && (2..=3).contains(&stem.len()))
                            .then(|| stem.to_uppercase())
                    })
                    .collect();
                codes.sort();
                codes
            })
            .unwrap_or_default();
        self.scanned = true;
    }

    /// The codes there are pictures for.
    pub fn found(&mut self) -> &[String] {
        if !self.scanned {
            self.rescan();
        }
        &self.found
    }

    /// The picture for airline `code`, if there is one.
    pub fn get(&mut self, ctx: &Context, code: &str) -> Option<(TextureId, Vec2)> {
        if !self.scanned {
            self.rescan();
        }
        let code = code.trim().to_uppercase();
        if !self.found.contains(&code) {
            return None;
        }
        self.loaded
            .entry(code.clone())
            .or_insert_with(|| load(ctx, &code))
            .as_ref()
            .map(|t| (t.id(), t.size_vec2()))
    }
}

/// `code`'s picture, as a texture. Big pictures are fine: they're scaled to
/// the card when drawn.
fn load(ctx: &Context, code: &str) -> Option<TextureHandle> {
    let dir = folder()?;
    let path = std::fs::read_dir(&dir).ok()?.flatten().find_map(|e| {
        let name = e.file_name().to_string_lossy().to_string();
        let (stem, ext) = name.rsplit_once('.')?;
        (stem.eq_ignore_ascii_case(code) && ext.eq_ignore_ascii_case("png")).then(|| e.path())
    })?;
    let img = image::open(path).ok()?.to_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    let pixels = ColorImage::from_rgba_unmultiplied(size, img.as_raw());
    Some(ctx.load_texture(format!("logo {code}"), pixels, TextureOptions::LINEAR))
}
