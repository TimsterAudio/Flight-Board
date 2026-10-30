//! The split-flap look, shared by Split-Flap Clock and Now Boarding: cards
//! on drums that only turn forward ([`flap`]), text folded to what a drum
//! can show ([`fold`]), a grid of cards that ripples to new rows
//! ([`board`]), the clatter and chime ([`sound`]), and the lamps above the
//! board ([`lighting`]).
//!
//! Pure look and sound: nothing here touches the network.

pub mod board;
pub mod flap;
pub mod fold;
pub mod lighting;
mod mesh;
pub mod rng;
pub mod sound;

pub use board::{Board, Column, Row, Style};
pub use flap::{Drum, Fall, Flap, Look};

/// Barlow Condensed SemiBold, the cards' typeface, under the SIL Open Font
/// License (`assets/OFL.txt`).
pub const FONT_DATA: &[u8] = include_bytes!("../assets/BarlowCondensed-SemiBold.ttf");

/// Adds the cards' typeface to `ctx` as the font family [`board::FONT`],
/// alongside egui's own fonts.
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "barlow".into(),
        egui::FontData::from_static(FONT_DATA).into(),
    );
    fonts.families.insert(
        egui::FontFamily::Name(board::FONT.into()),
        vec!["barlow".into()],
    );
    ctx.set_fonts(fonts);
}
