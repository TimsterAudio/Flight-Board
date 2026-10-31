//! Airline colour cards: an airline's two-letter code in plain card letters
//! on a card in its main colour, as a board's operator flap. No artwork:
//! just a colour each, close to the airline's own, which Settings can
//! change or add to. Logos the user adds themselves go over these (see
//! `logos.rs`).

use egui::Color32;

/// Common airlines' main colours, by IATA code, as `0xRRGGBB`.
const COLOURS: &[(&str, u32)] = &[
    ("QF", 0xE0001B),
    ("JQ", 0xFF5F00),
    ("VA", 0xCC0000),
    ("ZL", 0x00205B),
    ("NZ", 0x111111),
    ("SQ", 0x13306A),
    ("TR", 0xFFD500),
    ("EK", 0xD71921),
    ("QR", 0x5C0632),
    ("EY", 0xBD8B13),
    ("CX", 0x006564),
    ("MH", 0x0A2A6B),
    ("TG", 0x4B286D),
    ("GA", 0x00A1B0),
    ("CZ", 0x0A68B4),
    ("MU", 0x1A3D8F),
    ("CA", 0xC8102E),
    ("NH", 0x13448F),
    ("JL", 0xCC0000),
    ("KE", 0x0077C8),
    ("OZ", 0x5E6A71),
    ("CI", 0x0B4F9C),
    ("BR", 0x00703C),
    ("VN", 0x00808A),
    ("PR", 0x003F87),
    ("AI", 0xB31B1B),
    ("UA", 0x005DAA),
    ("DL", 0x003A70),
    ("AA", 0x0078D2),
    ("AC", 0xF01428),
    ("HA", 0x5B2C83),
    ("BA", 0x1E3A6E),
    ("LH", 0x0A1D3D),
    ("AF", 0x002157),
    ("KL", 0x00A1DE),
    ("TK", 0xC70A0C),
    ("LA", 0x1B0088),
    ("FJ", 0x1C3D6E),
    ("PX", 0xC8102E),
    ("NF", 0x00843D),
];

/// The demo's made-up airlines, so the demo board shows the cards too.
const DEMO: &[(&str, u32)] = &[
    ("NB", 0x2A4B8D),
    ("GX", 0x0F7A5C),
    ("VR", 0xB0302A),
    ("KF", 0xD98C1A),
    ("TQ", 0x1F8BA8),
    ("MW", 0x8A1538),
    ("PJ", 0x3D3D3D),
];

fn rgb(c: u32) -> [u8; 3] {
    [(c >> 16) as u8, (c >> 8) as u8, c as u8]
}

/// An airline's card colour: one set in Settings first, then the built-in
/// table. `None` for an airline in neither, which gets the board's own card.
pub fn colour(code: &str, own: &[(String, [u8; 3])]) -> Option<[u8; 3]> {
    let code = code.trim().to_uppercase();
    own.iter()
        .find(|(c, _)| c.eq_ignore_ascii_case(&code))
        .map(|(_, c)| *c)
        .or_else(|| {
            COLOURS
                .iter()
                .chain(DEMO)
                .find(|(c, _)| *c == code)
                .map(|(_, c)| rgb(*c))
        })
}

/// Letters that read on a card of `card`: dark on light colours, white on
/// dark ones.
pub fn ink(card: [u8; 3]) -> Color32 {
    let lin = |v: u8| {
        let v = v as f32 / 255.0;
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    let l = 0.2126 * lin(card[0]) + 0.7152 * lin(card[1]) + 0.0722 * lin(card[2]);
    if l > 0.3 {
        Color32::from_rgb(24, 24, 22)
    } else {
        Color32::from_rgb(244, 242, 236)
    }
}

/// A flight number's airline code: `QF` from `QF 401` or `QF401`.
pub fn code_of(number: &str) -> String {
    let n = number.trim();
    match n.split_once(' ') {
        Some((code, _)) => code.to_string(),
        // IATA codes are two characters, letters or a digit and a letter.
        None => n.chars().take(2).collect(),
    }
}

/// A flight number without its airline code: `401` from `QF 401`.
pub fn number_of(number: &str) -> String {
    let n = number.trim();
    match n.split_once(' ') {
        Some((_, rest)) => rest.trim().to_string(),
        None => n.chars().skip(2).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn airlines_have_their_colours_and_settings_win() {
        assert_eq!(colour("QF", &[]), Some([0xE0, 0x00, 0x1B]));
        assert_eq!(colour("qf", &[]), Some([0xE0, 0x00, 0x1B]));
        assert_eq!(colour("ZZ", &[]), None);
        let own = vec![("QF".to_string(), [1, 2, 3])];
        assert_eq!(colour("QF", &own), Some([1, 2, 3]));
    }

    #[test]
    fn letters_read_on_their_card() {
        // Dark on Scoot's yellow, light on Qantas's red and on black.
        assert!(ink(rgb(0xFFD500)).r() < 100);
        assert!(ink(rgb(0xE0001B)).r() > 200);
        assert!(ink([0, 0, 0]).r() > 200);
    }

    #[test]
    fn flight_numbers_split_into_airline_and_number() {
        assert_eq!(
            (code_of("QF 401"), number_of("QF 401")),
            ("QF".into(), "401".into())
        );
        assert_eq!(
            (code_of("NZ124"), number_of("NZ124")),
            ("NZ".into(), "124".into())
        );
        assert_eq!(
            (code_of("3K 685"), number_of("3K 685")),
            ("3K".into(), "685".into())
        );
    }
}
