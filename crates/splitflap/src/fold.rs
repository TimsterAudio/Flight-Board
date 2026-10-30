//! Text folded to what a letter drum can show: capitals, digits and a little
//! punctuation. Accents are dropped (ZÜRICH is ZURICH), lowercase is
//! upper-cased, and anything else becomes a space. A name too long for its
//! cards is shortened the way a board's sign-writer would, then cut.

use unicode_normalization::char::is_combining_mark;
use unicode_normalization::UnicodeNormalization;

use crate::flap::ALPHA;

/// Long words and their usual short forms, tried in this order until a
/// name fits.
pub const ABBREVIATIONS: &[(&str, &str)] = &[
    ("INTERNATIONAL", "INTL"),
    ("AIRPORT", "APT"),
    ("STATION", "STN"),
    ("TERMINAL", "TERM"),
    ("JUNCTION", "JCT"),
    ("CENTRAL", "CTRL"),
    ("CROSSING", "XING"),
    ("GARDENS", "GDNS"),
    ("HEIGHTS", "HTS"),
    ("REGIONAL", "REGL"),
    ("MUNICIPAL", "MUNI"),
    ("SAINT", "ST"),
    ("MOUNT", "MT"),
    ("FORT", "FT"),
    ("PORT", "PT"),
    ("NORTH", "NTH"),
    ("SOUTH", "STH"),
    ("UPPER", "UPR"),
    ("LOWER", "LWR"),
    ("STREET", "ST"),
    ("ROAD", "RD"),
    ("PARK", "PK"),
    ("AND", "&"),
];

/// `text` as a letter drum can show it, with runs of spaces closed up.
pub fn fold(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.nfd() {
        if is_combining_mark(c) {
            continue;
        }
        // Letters that are not a plain letter with an accent.
        let wide = match c {
            'ß' => "SS",
            'Æ' | 'æ' => "AE",
            'Œ' | 'œ' => "OE",
            'Ø' | 'ø' => "O",
            'Ł' | 'ł' => "L",
            'Đ' | 'đ' | 'Ð' | 'ð' => "D",
            'Þ' | 'þ' => "TH",
            'ı' => "I",
            '‘' | '’' | '`' | '´' => "'",
            '–' | '—' | '‐' => "-",
            _ => "",
        };
        if !wide.is_empty() {
            out.push_str(wide);
            continue;
        }
        for u in c.to_uppercase() {
            out.push(if u != ' ' && ALPHA.contains(u) {
                u
            } else {
                ' '
            });
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `text` folded and made to fit `cards` cards: shortened word by word from
/// [`ABBREVIATIONS`], then cut, never ending in a space.
pub fn fit(text: &str, cards: usize) -> String {
    let mut s = fold(text);
    for (long, short) in ABBREVIATIONS {
        if s.chars().count() <= cards {
            return s;
        }
        s = s
            .split(' ')
            .map(|w| if w == *long { *short } else { w })
            .collect::<Vec<_>>()
            .join(" ");
    }
    s.chars()
        .take(cards)
        .collect::<String>()
        .trim_end()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accents_drop_and_case_rises() {
        assert_eq!(fold("Zürich"), "ZURICH");
        assert_eq!(fold("São Paulo"), "SAO PAULO");
        assert_eq!(fold("Kraków Główny"), "KRAKOW GLOWNY");
        assert_eq!(fold("Malmö C"), "MALMO C");
        assert_eq!(fold("Düsseldorf Hbf"), "DUSSELDORF HBF");
        assert_eq!(fold("Straße"), "STRASSE");
        assert_eq!(fold("Reykjavík–Keflavík"), "REYKJAVIK-KEFLAVIK");
        assert_eq!(fold("Hawke’s Bay"), "HAWKE'S BAY");
    }

    #[test]
    fn what_the_drum_lacks_becomes_a_space() {
        assert_eq!(fold("Gate #5 ✈ now"), "GATE 5 NOW");
        assert_eq!(fold("東京 Tokyo"), "TOKYO");
        assert_eq!(fold("  a   b "), "A B");
        // Everything that comes out is on the drum.
        let s = fold("Ünïcødé; «quoted» [x] {y} 50% @home");
        assert!(s.chars().all(|c| ALPHA.contains(c)), "{s}");
    }

    #[test]
    fn long_names_shorten_then_cut() {
        assert_eq!(fit("Melbourne", 16), "MELBOURNE");
        assert_eq!(
            fit("Kuala Lumpur International Airport", 16),
            "KUALA LUMPUR INT"
        );
        assert_eq!(fit("Kuala Lumpur International", 18), "KUALA LUMPUR INTL");
        assert_eq!(fit("Southern Cross Station", 18), "SOUTHERN CROSS STN");
        assert_eq!(fit("Mount Waverley", 10), "MT WAVERLE");
        // Shortened words are whole words only.
        assert_eq!(fit("Portland Airport", 12), "PORTLAND APT");
        assert_eq!(fit("Abcdefgh ijk", 9), "ABCDEFGH");
    }
}
