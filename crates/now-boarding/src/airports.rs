//! Airports a flight board can show, found without asking the network: an
//! airport search on AeroAPI costs a query like any other. Any other
//! airport can be given by its ICAO code in Settings.

use crate::model::Place;

/// ICAO code, IATA code, the name the board shows, and its time zone.
const AIRPORTS: &[(&str, &str, &str, &str)] = &[
    ("YMML", "MEL", "Melbourne", "Australia/Melbourne"),
    ("YMAV", "AVV", "Avalon", "Australia/Melbourne"),
    ("YSSY", "SYD", "Sydney", "Australia/Sydney"),
    ("YBBN", "BNE", "Brisbane", "Australia/Brisbane"),
    ("YPAD", "ADL", "Adelaide", "Australia/Adelaide"),
    ("YPPH", "PER", "Perth", "Australia/Perth"),
    ("YSCB", "CBR", "Canberra", "Australia/Sydney"),
    ("YMHB", "HBA", "Hobart", "Australia/Hobart"),
    ("YBCG", "OOL", "Gold Coast", "Australia/Brisbane"),
    ("YBCS", "CNS", "Cairns", "Australia/Brisbane"),
    ("YPDN", "DRW", "Darwin", "Australia/Darwin"),
    ("NZAA", "AKL", "Auckland", "Pacific/Auckland"),
    ("NZCH", "CHC", "Christchurch", "Pacific/Auckland"),
    ("NZWN", "WLG", "Wellington", "Pacific/Auckland"),
    ("WSSS", "SIN", "Singapore Changi", "Asia/Singapore"),
    ("VHHH", "HKG", "Hong Kong", "Asia/Hong_Kong"),
    ("RJTT", "HND", "Tokyo Haneda", "Asia/Tokyo"),
    ("OMDB", "DXB", "Dubai", "Asia/Dubai"),
    ("EGLL", "LHR", "London Heathrow", "Europe/London"),
    ("LFPG", "CDG", "Paris Charles de Gaulle", "Europe/Paris"),
    ("EDDF", "FRA", "Frankfurt", "Europe/Berlin"),
    ("EHAM", "AMS", "Amsterdam Schiphol", "Europe/Amsterdam"),
    ("KJFK", "JFK", "New York JFK", "America/New_York"),
    ("KLAX", "LAX", "Los Angeles", "America/Los_Angeles"),
    ("KSFO", "SFO", "San Francisco", "America/Los_Angeles"),
    ("CYVR", "YVR", "Vancouver", "America/Vancouver"),
];

fn place(a: &(&str, &str, &str, &str)) -> Place {
    Place {
        id: a.0.into(),
        name: a.2.into(),
        tz: a.3.into(),
    }
}

/// Melbourne, the first board.
pub fn melbourne() -> Place {
    place(&AIRPORTS[0])
}

/// Airports whose code (ICAO or IATA) or name matches `text`. A four-letter
/// code not in the list is taken as an ICAO code, in the computer's own
/// time zone, named by its code.
pub fn search(text: &str, local_tz: &str) -> Vec<Place> {
    let t = text.trim();
    if t.is_empty() {
        return Vec::new();
    }
    let lower = t.to_lowercase();
    let mut out: Vec<Place> = AIRPORTS
        .iter()
        .filter(|a| {
            a.0.eq_ignore_ascii_case(t)
                || a.1.eq_ignore_ascii_case(t)
                || a.2.to_lowercase().contains(&lower)
        })
        .map(place)
        .collect();
    if out.is_empty() && t.len() == 4 && t.chars().all(|c| c.is_ascii_alphanumeric()) {
        out.push(Place {
            id: t.to_uppercase(),
            name: t.to_uppercase(),
            tz: local_tz.into(),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn airports_are_found_by_code_or_name() {
        assert_eq!(search("MEL", "UTC")[0].id, "YMML");
        assert_eq!(search("ymml", "UTC")[0].name, "Melbourne");
        assert_eq!(search("heathrow", "UTC")[0].tz, "Europe/London");
        // An ICAO code not in the list still works, named by its code.
        let p = &search("YBAS", "Australia/Darwin")[0];
        assert_eq!((p.id.as_str(), p.tz.as_str()), ("YBAS", "Australia/Darwin"));
        assert!(search("nowhere at all", "UTC").is_empty());
    }
}
