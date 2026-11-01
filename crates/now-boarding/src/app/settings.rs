//! The settings window: which board, how many rows and pages, the columns,
//! the look and the sound, the demo's clock, and a word about the data.

use egui::{vec2, RichText, Ui, ViewportBuilder, ViewportId};
use splitflap::lighting::{Failures, Mood};

use super::{widths_key, App, Layout};
use crate::aeroapi::AeroApi;
use crate::http::Net;
use crate::model::{Direction, Feed, Mode, Place};
use crate::provider::Provider;
use crate::ptv::{self, Ptv};
use crate::task::Task;
use crate::{airlines, airports, demo, keys, logos, normalise};

/// The page turns on offer, in seconds.
const PAGE_TURNS: [(u32, &str); 7] = [
    (10, "Every 10 seconds"),
    (20, "Every 20 seconds"),
    (30, "Every 30 seconds"),
    (60, "Every minute"),
    (120, "Every 2 minutes"),
    (300, "Every 5 minutes"),
    (600, "Every 10 minutes"),
];

const MOODS: [(Mood, &str); 4] = [
    (Mood::Off, "No lamps"),
    (Mood::Soft, "Soft"),
    (Mood::Warm, "Warm"),
    (Mood::Evening, "Evening"),
];

const PTV_CREDIT: &str = "Train times for Victoria come from Public Transport Victoria's \
Timetable API, licensed under Creative Commons Attribution 4.0. \
Now Boarding is not made or endorsed by PTV.";

const AEROAPI_CREDIT: &str = "Flight times come from FlightAware's AeroAPI, \
with your own key, under its terms: the Personal tier is for personal use only. \
Now Boarding is not made or endorsed by FlightAware.";

const AEROAPI_STEPS: &str =
    "1. Sign up for FlightAware AeroAPI (flightaware.com, Commercial, AeroAPI) \
and choose the Personal tier. It gives a little free credit each month.\n\
2. In AeroAPI's portal, make an API key.\n\
3. Paste it here and press Save. It goes into your computer's key store, nowhere else.\n\
4. Check the price of a page of results on AeroAPI's pricing page, and set it below.";

const LOGO_NOTE: &str = "For your own use only: add pictures you're happy to use. \
Now Boarding comes with no logos and fetches none. \
Name each one by the airline's two-letter code, as QF.png, and use PNG.";

/// What's typed on the AeroAPI page, and the jobs it has running.
#[derive(Default)]
pub struct AeroForm {
    key: String,
    said: Option<Result<String, String>>,
    test: Option<Task<Result<String, String>>>,
    airport_text: String,
    found: Vec<Place>,
    new_code: String,
}

const PTV_STEPS: &str = "1. Ask PTV for a Timetable API key: the way to ask is on \
ptv.vic.gov.au, under Data and reporting, Datasets, PTV Timetable API. It's free.\n\
2. PTV sends back a developer ID (a number) and a key (letters and numbers in groups).\n\
3. Paste both here and press Save. They go into your computer's key store, \
nowhere else.\n\
4. Press Test, then search for your station.";

/// What's typed on the PTV page, and the jobs it has running.
#[derive(Default)]
pub struct PtvForm {
    devid: String,
    key: String,
    /// What the last save, test or search said.
    said: Option<Result<String, String>>,
    test: Option<Task<Result<String, String>>>,
    search_text: String,
    search: Option<Task<Result<Vec<Place>, String>>>,
    found: Vec<Place>,
}

const ABOUT: &str = "Now Boarding shows departures and arrivals to look at, for personal use. \
Data can be late or wrong: always check with the airline or the operator. \
It is not for making travel decisions.";

fn heading(ui: &mut Ui, text: &str) {
    ui.add_space(10.0);
    ui.label(RichText::new(text).strong().size(15.0));
    ui.add_space(2.0);
}

impl App {
    pub(super) fn settings(&mut self, ctx: &egui::Context) {
        let mut builder = ViewportBuilder::default()
            .with_title("Now Boarding settings")
            .with_inner_size([520.0, 760.0])
            .with_min_inner_size([420.0, 400.0]);
        if let Some(at) = self.settings_at {
            builder = builder.with_position(at);
        }
        ctx.show_viewport_immediate(ViewportId::from_hash_of("settings"), builder, |ui, _| {
            if ui.input(|i| i.viewport().close_requested()) {
                self.settings_open = false;
            }
            egui::CentralPanel::default().show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    self.settings_page(ui);
                });
            });
        });
    }

    fn settings_page(&mut self, ui: &mut Ui) {
        let s = &mut self.saved;
        heading(ui, "Board");
        ui.horizontal(|ui| {
            ui.selectable_value(&mut s.mode, Mode::Flight, "Flights");
            ui.selectable_value(&mut s.mode, Mode::Train, "Trains");
        });
        ui.radio_value(&mut s.layout, Layout::One, "One board");
        if s.layout == Layout::One {
            ui.indent("direction", |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut s.direction, Direction::Departure, "Departures");
                    ui.selectable_value(&mut s.direction, Direction::Arrival, "Arrivals");
                });
            });
        }
        ui.radio_value(
            &mut s.layout,
            Layout::Stacked,
            "Two boards, one above the other, in one window",
        );
        ui.radio_value(
            &mut s.layout,
            Layout::TwoWindows,
            "Two boards, in a window each",
        );
        if s.layout != Layout::One {
            ui.indent("second", |ui| {
                ui.horizontal(|ui| {
                    ui.label("Departures, then");
                    ui.selectable_value(&mut s.second, Direction::Arrival, "arrivals");
                    ui.selectable_value(&mut s.second, Direction::Departure, "departures again")
                        .on_hover_text("Say for another terminal: set each board's below");
                });
            });
        }
        ui.add_space(4.0);
        match s.mode {
            Mode::Flight => {
                ui.horizontal(|ui| {
                    ui.label("Feed");
                    ui.selectable_value(
                        &mut s.flight_feed,
                        Feed::Demo,
                        "Demo: Gate 05 International",
                    );
                    ui.selectable_value(&mut s.flight_feed, Feed::AeroApi, "FlightAware AeroAPI");
                });
            }
            Mode::Train => {
                ui.horizontal(|ui| {
                    ui.label("Feed");
                    ui.selectable_value(&mut s.train_feed, Feed::Demo, "Demo: Harbour Street");
                    ui.selectable_value(&mut s.train_feed, Feed::Ptv, "PTV: Victoria's trains");
                });
            }
        }
        match s.feed() {
            Feed::Ptv => self.ptv_page(ui),
            Feed::AeroApi => self.aero_page(ui),
            Feed::Demo => {}
        }
        if self.saved.mode == Mode::Flight {
            self.terminals_page(ui);
            self.airline_cards(ui);
        }
        let s = &mut self.saved;

        heading(ui, "Rows and pages");
        ui.add(egui::Slider::new(&mut s.rows, 8..=24).text("rows on the board"));
        ui.horizontal(|ui| {
            ui.label("When there are more, turn the page");
            let current = PAGE_TURNS
                .iter()
                .find(|(secs, _)| *secs == s.page_secs)
                .map_or_else(
                    || format!("Every {} seconds", s.page_secs),
                    |(_, l)| l.to_string(),
                );
            egui::ComboBox::from_id_salt("page turn")
                .selected_text(current)
                .show_ui(ui, |ui| {
                    for (secs, label) in PAGE_TURNS {
                        ui.selectable_value(&mut s.page_secs, secs, label);
                    }
                });
        });

        ui.add(egui::Slider::new(&mut s.max_pages, 1..=6).text("pages at most"));

        heading(ui, "Columns");
        ui.label(RichText::new("Each column's width, in cards.").weak());
        let directions = match s.layout {
            Layout::One => vec![s.direction],
            _ => vec![Direction::Departure, Direction::Arrival],
        };
        for d in directions {
            let key = widths_key(s.mode, d);
            let mut specs = s.specs(s.mode, d);
            let preset = normalise::preset(s.mode, d);
            ui.label(RichText::new(d.title()).small().strong());
            let mut changed = false;
            ui.horizontal_wrapped(|ui| {
                for spec in &mut specs {
                    ui.label(spec.title);
                    changed |= ui
                        .add(egui::DragValue::new(&mut spec.cards).range(1..=40))
                        .changed();
                    ui.add_space(6.0);
                }
            });
            if changed {
                s.widths
                    .insert(key.clone(), specs.iter().map(|x| x.cards).collect());
            }
            if specs != preset && ui.small_button("Back to the preset").clicked() {
                s.widths.remove(&key);
            }
        }
        ui.horizontal(|ui| {
            ui.label("Second title");
            ui.add(
                egui::TextEdit::singleline(&mut s.second_title)
                    .hint_text("such as ABFLUG or PARTENZE")
                    .desired_width(200.0),
            );
        });

        heading(ui, "Look");
        ui.horizontal(|ui| {
            ui.label("Lamps");
            for (mood, label) in MOODS {
                ui.selectable_value(&mut s.mood, mood, label);
            }
        });
        if s.mood != Mood::Off {
            ui.add(egui::Slider::new(&mut s.dimmer, 0.25..=1.6).text("dimmer"));
            egui::ComboBox::from_label("Bulbs")
                .selected_text(s.bulbs.label())
                .show_ui(ui, |ui| {
                    for f in Failures::ALL {
                        ui.selectable_value(&mut s.bulbs, f, f.label());
                    }
                });
        }
        ui.add(
            egui::Slider::new(&mut s.flip_speed, 0.25..=4.0)
                .logarithmic(true)
                .text("flip speed"),
        );

        heading(ui, "Sound");
        ui.checkbox(&mut s.sound, "The clatter of the flaps");
        ui.add_enabled(
            s.sound,
            egui::Slider::new(&mut s.volume, 0.0..=1.0).text("volume"),
        );
        ui.checkbox(
            &mut s.chime,
            "The chime, for boarding and gate or platform changes",
        );
        ui.add_enabled(
            s.chime,
            egui::Slider::new(&mut s.chime_volume, 0.0..=1.0).text("chime volume"),
        );
        if let Some(e) = &self.sound_error {
            ui.label(RichText::new(format!("No sound: {e}")).weak());
        }

        let s = &mut self.saved;
        heading(ui, "Demo");
        ui.horizontal(|ui| {
            ui.label("The demo's day runs");
            ui.selectable_value(&mut s.demo_speed, 1.0, "in real time");
            ui.selectable_value(&mut s.demo_speed, 10.0, "10×");
            ui.selectable_value(&mut s.demo_speed, 60.0, "60×");
        });
        let mut restart = None;
        ui.horizontal(|ui| {
            ui.label("Start the day again at");
            for (h, m) in [(5, 0), (12, 0), (18, 0)] {
                if ui.button(format!("{h:02}:{m:02}")).clicked() {
                    restart = Some((h, m));
                }
            }
        });
        if let Some((h, m)) = restart {
            self.restart_at(h, m);
        }

        heading(ui, "About");
        ui.label(ABOUT);
        ui.add_space(4.0);
        ui.label(PTV_CREDIT);
        ui.add_space(4.0);
        ui.label(AEROAPI_CREDIT);
        ui.add_space(4.0);
        ui.label(
            RichText::new(format!("Now Boarding {}", env!("CARGO_PKG_VERSION")))
                .weak()
                .small(),
        );
        ui.add_space(vec2(0.0, 12.0).y);
    }
}

impl App {
    /// The PTV page: the key, a test, and the station.
    fn ptv_page(&mut self, ui: &mut Ui) {
        heading(ui, "PTV: your key");
        // Jobs that have finished.
        if let Some(r) = self.ptv_form.test.as_ref().and_then(Task::take) {
            self.ptv_form.said = Some(r);
            self.ptv_form.test = None;
        }
        if let Some(r) = self.ptv_form.search.as_ref().and_then(Task::take) {
            match r {
                Ok(found) => {
                    self.ptv_form.said = Some(Ok(match found.len() {
                        0 => "No stations found by that name.".into(),
                        1 => "Found one station.".into(),
                        n => format!("Found {n} stations."),
                    }));
                    self.ptv_form.found = found;
                }
                Err(e) => self.ptv_form.said = Some(Err(e)),
            }
            self.ptv_form.search = None;
        }

        match (&self.ptv_keys, &self.keys_error) {
            (Some(k), _) => {
                ui.label(format!("Saved in the key store: developer ID {}.", k.devid));
            }
            (None, Some(e)) => {
                ui.label(PTV_STEPS);
                ui.label(RichText::new(format!("The key store can't be read: {e}")).weak());
            }
            (None, None) => {
                ui.label(PTV_STEPS);
            }
        }
        let form = &mut self.ptv_form;
        egui::Grid::new("ptv key").num_columns(2).show(ui, |ui| {
            ui.label("Developer ID");
            ui.add(egui::TextEdit::singleline(&mut form.devid).desired_width(200.0));
            ui.end_row();
            ui.label("Key");
            ui.add(
                egui::TextEdit::singleline(&mut form.key)
                    .password(true)
                    .desired_width(320.0),
            );
            ui.end_row();
        });
        let typed =
            (!form.devid.trim().is_empty() && !form.key.trim().is_empty()).then(|| ptv::Keys {
                devid: form.devid.trim().to_string(),
                key: form.key.trim().to_string(),
            });
        // What a test or a search uses: what's typed, or else what's saved.
        let using = typed.clone().or_else(|| self.ptv_keys.clone());
        let busy = self.ptv_form.test.is_some() || self.ptv_form.search.is_some();
        ui.horizontal(|ui| {
            if ui
                .add_enabled(typed.is_some(), egui::Button::new("Save"))
                .on_hover_text("Into your computer's key store")
                .clicked()
            {
                if let Some(k) = &typed {
                    match keys::save_ptv(k) {
                        Ok(()) => {
                            self.ptv_keys = Some(k.clone());
                            self.keys_error = None;
                            self.keys_changed += 1;
                            self.ptv_form.devid.clear();
                            self.ptv_form.key.clear();
                            self.ptv_form.said = Some(Ok("Saved.".into()));
                        }
                        Err(e) => {
                            self.ptv_form.said =
                                Some(Err(format!("The key store wouldn't take it: {e}")));
                        }
                    }
                }
            }
            if ui
                .add_enabled(using.is_some() && !busy, egui::Button::new("Test"))
                .on_hover_text("Asks PTV for a station, to see the key works")
                .clicked()
            {
                if let Some(k) = using.clone() {
                    self.ptv_form.said = None;
                    self.ptv_form.test = Some(Task::start(ui.ctx(), move || {
                        let p = ptv_provider(k);
                        match p.search("Southern Cross") {
                            Ok(_) => Ok("The key works.".to_string()),
                            Err(e) => Err(format!("It didn't work: {e}")),
                        }
                    }));
                }
            }
            if self.ptv_keys.is_some()
                && ui
                    .button("Forget")
                    .on_hover_text("Takes the key out of the key store")
                    .clicked()
            {
                match keys::forget_ptv() {
                    Ok(()) => {
                        self.ptv_keys = None;
                        self.keys_changed += 1;
                        self.ptv_form.said = Some(Ok("Forgotten.".into()));
                    }
                    Err(e) => self.ptv_form.said = Some(Err(e)),
                }
            }
            if busy {
                ui.spinner();
            }
        });

        heading(ui, "PTV: your station");
        ui.label(match &self.saved.ptv_place {
            Some(p) => format!("The board shows departures from {}.", p.name),
            None => "Search for a station, then choose it.".into(),
        });
        let mut go = false;
        ui.horizontal(|ui| {
            let r = ui.add(
                egui::TextEdit::singleline(&mut self.ptv_form.search_text)
                    .hint_text("such as Southern Cross")
                    .desired_width(220.0),
            );
            go |= r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            go |= ui
                .add_enabled(using.is_some() && !busy, egui::Button::new("Search"))
                .clicked();
        });
        if go && !busy {
            if let Some(k) = using {
                let text = self.ptv_form.search_text.clone();
                self.ptv_form.said = None;
                self.ptv_form.search = Some(Task::start(ui.ctx(), move || {
                    ptv_provider(k).search(&text).map_err(|e| e.to_string())
                }));
            }
        }
        let mut chosen = None;
        for p in &self.ptv_form.found {
            let here = self.saved.ptv_place.as_ref().is_some_and(|c| c.id == p.id);
            let lines = ptv::stops(p)
                .iter()
                .map(|(t, _)| if *t == 3 { "V/Line" } else { "metro" })
                .collect::<Vec<_>>()
                .join(" and ");
            if ui
                .selectable_label(here, format!("{}  ({lines})", p.name))
                .clicked()
            {
                chosen = Some(p.clone());
            }
        }
        if let Some(p) = chosen {
            self.saved.ptv_place = Some(p);
        }
        match &self.ptv_form.said {
            Some(Ok(m)) => {
                ui.label(m.as_str());
            }
            Some(Err(e)) => {
                ui.colored_label(ui.visuals().warn_fg_color, e.as_str());
            }
            None => {}
        }
        ui.label(
            RichText::new(
                "PTV gives departures only, so the board shows departures. It asks once a minute.",
            )
            .weak(),
        );
    }
}

/// PTV, with keys `k`, for a Settings job; a stand-in server in a debug
/// build if one is set.
fn ptv_provider(k: ptv::Keys) -> Ptv {
    let p = Ptv::new(k, Box::new(Net::new()));
    #[cfg(debug_assertions)]
    if let Ok(base) = std::env::var("NOW_BOARDING_PTV_BASE") {
        return p.with_base(&base);
    }
    p
}

impl App {
    /// The AeroAPI page: the key, the airport and the budget.
    fn aero_page(&mut self, ui: &mut Ui) {
        heading(ui, "AeroAPI: your key");
        if let Some(r) = self.aero_form.test.as_ref().and_then(Task::take) {
            self.aero_form.said = Some(r);
            self.aero_form.test = None;
        }
        match (&self.aero_key, &self.aero_error) {
            (Some(_), _) => {
                ui.label("Your key is saved in the key store.");
            }
            (None, e) => {
                ui.label(AEROAPI_STEPS);
                if let Some(e) = e {
                    ui.label(RichText::new(format!("The key store can't be read: {e}")).weak());
                }
            }
        }
        ui.horizontal(|ui| {
            ui.label("Key");
            ui.add(
                egui::TextEdit::singleline(&mut self.aero_form.key)
                    .password(true)
                    .desired_width(320.0),
            );
        });
        let typed = Some(self.aero_form.key.trim().to_string()).filter(|k| !k.is_empty());
        let using = typed.clone().or_else(|| self.aero_key.clone());
        let busy = self.aero_form.test.is_some();
        ui.horizontal(|ui| {
            if ui
                .add_enabled(typed.is_some(), egui::Button::new("Save"))
                .on_hover_text("Into your computer's key store")
                .clicked()
            {
                if let Some(k) = &typed {
                    match keys::save_aeroapi(k) {
                        Ok(()) => {
                            self.aero_key = Some(k.clone());
                            self.aero_error = None;
                            self.keys_changed += 1;
                            self.aero_form.key.clear();
                            self.aero_form.said = Some(Ok("Saved.".into()));
                        }
                        Err(e) => {
                            self.aero_form.said =
                                Some(Err(format!("The key store wouldn't take it: {e}")))
                        }
                    }
                }
            }
            if ui
                .add_enabled(using.is_some() && !busy, egui::Button::new("Test"))
                .on_hover_text("Asks for one page of the airport's departures: it costs one page")
                .clicked()
            {
                if let Some(k) = using.clone() {
                    let place = self.saved.airport.clone();
                    let spent = self.spent.clone();
                    let budget = crate::aeroapi::Budget {
                        pages: 1,
                        ..self.saved.budget.clone()
                    };
                    self.aero_form.said = None;
                    self.aero_form.test = Some(Task::start(ui.ctx(), move || {
                        let p = aero_provider(k, budget, spent);
                        let q = crate::model::BoardQuery {
                            place,
                            mode: Mode::Flight,
                            direction: Direction::Departure,
                            now: chrono::Utc::now(),
                            terminal: String::new(),
                            want: 15,
                        };
                        match p.board(&q) {
                            Ok(a) => Ok(format!(
                                "The key works: {} departures in the first page.",
                                a.movements.len()
                            )),
                            Err(e) => Err(format!("It didn't work: {e}")),
                        }
                    }));
                }
            }
            if self.aero_key.is_some()
                && ui
                    .button("Forget")
                    .on_hover_text("Takes the key out of the key store")
                    .clicked()
            {
                match keys::forget_aeroapi() {
                    Ok(()) => {
                        self.aero_key = None;
                        self.keys_changed += 1;
                        self.aero_form.said = Some(Ok("Forgotten.".into()));
                    }
                    Err(e) => self.aero_form.said = Some(Err(e)),
                }
            }
            if busy {
                ui.spinner();
            }
        });
        match &self.aero_form.said {
            Some(Ok(m)) => {
                ui.label(m.as_str());
            }
            Some(Err(e)) => {
                ui.colored_label(ui.visuals().warn_fg_color, e.as_str());
            }
            None => {}
        }

        heading(ui, "AeroAPI: your airport");
        ui.label(format!(
            "The board shows {} ({}).",
            self.saved.airport.name, self.saved.airport.id
        ));
        ui.horizontal(|ui| {
            let r = ui.add(
                egui::TextEdit::singleline(&mut self.aero_form.airport_text)
                    .hint_text("MEL, YMML or Melbourne")
                    .desired_width(200.0),
            );
            if r.changed() {
                self.aero_form.found =
                    airports::search(&self.aero_form.airport_text, demo::local_tz().name());
            }
        });
        let mut chosen = None;
        for p in &self.aero_form.found {
            let here = self.saved.airport.id == p.id;
            if ui
                .selectable_label(here, format!("{}  ({}, {})", p.name, p.id, p.tz))
                .clicked()
            {
                chosen = Some(p.clone());
            }
        }
        if let Some(p) = chosen {
            self.saved.airport = p;
        }

        heading(ui, "AeroAPI: your budget");
        let b = &mut self.saved.budget;
        egui::Grid::new("budget").num_columns(2).show(ui, |ui| {
            ui.label("A month, at most");
            ui.add(
                egui::DragValue::new(&mut b.monthly)
                    .range(0.0..=500.0)
                    .speed(0.5)
                    .prefix("$"),
            );
            ui.end_row();
            ui.label("A page of results costs");
            ui.add(
                egui::DragValue::new(&mut b.per_page)
                    .range(0.0001..=1.0)
                    .speed(0.001)
                    .max_decimals(4)
                    .prefix("$"),
            );
            ui.end_row();
            ui.label("Pages each update, at most (15 flights a page)");
            ui.add(egui::DragValue::new(&mut b.pages).range(1..=5));
            ui.end_row();
            ui.label("Hours a day the board is open");
            ui.add(
                egui::DragValue::new(&mut b.hours)
                    .range(0.5..=24.0)
                    .speed(0.25),
            );
            ui.end_row();
        });
        let boards = self.saved.aero_boards().max(1);
        let b = &self.saved.budget;
        let every = b.interval(boards).as_secs_f64() / 60.0;
        let used = self
            .spent
            .lock()
            .map(|s| s.this_month(chrono::Utc::now()))
            .unwrap_or(0);
        ui.label(format!(
            "So the board updates about every {}, even when it uses every page. This month: {used} of {} pages, about ${:.2} of ${:.2}.",
            if every >= 90.0 {
                format!("{:.1} hours", every / 60.0)
            } else {
                format!("{every:.0} minutes")
            },
            b.pages_a_month(),
            used as f64 * b.per_page,
            b.monthly
        ));
        ui.label(
            RichText::new(
                "Once the month's budget is used, the board stops asking until the 1st. \
                 Check the price per page on AeroAPI's own pricing page.",
            )
            .weak(),
        );
    }

    /// Each board's terminal: all, or one, from those the flights name.
    fn terminals_page(&mut self, ui: &mut Ui) {
        heading(ui, "Terminals");
        let boards = self.views.len().max(1);
        let mut seen: Vec<String> = self.views.iter().flat_map(|v| v.terminals()).collect();
        seen.sort();
        seen.dedup();
        let s = &mut self.saved;
        if s.terminals.len() < boards {
            s.terminals.resize(boards, String::new());
        }
        for i in 0..boards {
            let which = match (boards, i, s.second) {
                (1, _, _) => "The board".to_string(),
                (_, 0, _) => "Departures".to_string(),
                (_, _, Direction::Arrival) => "Arrivals".to_string(),
                (_, _, Direction::Departure) => "The second departures".to_string(),
            };
            ui.horizontal_wrapped(|ui| {
                ui.label(format!("{which}: "));
                let t = &mut s.terminals[i];
                if ui.selectable_label(t.trim().is_empty(), "All").clicked() {
                    t.clear();
                }
                for k in &seen {
                    let here = normalise::terminal_key(t) == *k;
                    if ui.selectable_label(here, format!("T{k}")).clicked() {
                        *t = k.clone();
                    }
                }
                ui.add(
                    egui::TextEdit::singleline(t)
                        .hint_text("or type one")
                        .desired_width(70.0),
                );
            });
        }
        ui.label(
            RichText::new(
                "Terminals offered are the ones in the flights the board has. Flights with no \
                 terminal given show only on All. FlightAware sends the whole airport's \
                 flights, a page of 15 at a time, so a one-terminal board asks for pages until \
                 it's full, up to the pages each update set above: at a busy airport such as \
                 Heathrow, 4 or 5.",
            )
            .weak(),
        );
    }

    /// Airline colour cards, colours of your own, and logos you've added.
    fn airline_cards(&mut self, ui: &mut Ui) {
        heading(ui, "Airline cards");
        ui.checkbox(
            &mut self.saved.colour_cards,
            "Airline colour cards: the airline's code on a card in its colour",
        );
        let mut remove = None;
        for (i, (code, colour)) in self.saved.airline_colours.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.label(format!("{code:>3}"));
                egui::color_picker::color_edit_button_srgb(ui, colour);
                if ui.small_button("Remove").clicked() {
                    remove = Some(i);
                }
            });
        }
        if let Some(i) = remove {
            self.saved.airline_colours.remove(i);
        }
        ui.horizontal(|ui| {
            ui.label("Your own colour for");
            ui.add(
                egui::TextEdit::singleline(&mut self.aero_form.new_code)
                    .hint_text("QF")
                    .desired_width(40.0)
                    .char_limit(3),
            );
            let code = self.aero_form.new_code.trim().to_uppercase();
            if ui
                .add_enabled(code.len() >= 2, egui::Button::new("Add"))
                .clicked()
            {
                let start = airlines::colour(&code, &[]).unwrap_or([128, 128, 128]);
                self.saved.airline_colours.retain(|(c, _)| *c != code);
                self.saved.airline_colours.push((code, start));
                self.aero_form.new_code.clear();
            }
        });

        ui.add_space(6.0);
        ui.label(RichText::new("Logos").strong());
        ui.label(LOGO_NOTE);
        match logos::folder() {
            Some(dir) => {
                ui.horizontal(|ui| {
                    ui.label("Folder:");
                    ui.code(dir.display().to_string());
                });
                ui.horizontal(|ui| {
                    if ui.button("Open the folder").clicked() {
                        let _ = std::fs::create_dir_all(&dir);
                        open_folder(&dir);
                    }
                    if ui.button("Look again").clicked() {
                        self.logos.rescan();
                    }
                });
                let found = self.logos.found();
                ui.label(if found.is_empty() {
                    "No logos there yet.".to_string()
                } else {
                    format!("Logos for: {}", found.join(", "))
                });
            }
            None => {
                ui.label("There's no folder for logos on this computer.");
            }
        }
    }
}

/// Opens `dir` in the system's file manager.
fn open_folder(dir: &std::path::Path) {
    let program = if cfg!(windows) {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let _ = std::process::Command::new(program).arg(dir).spawn();
}

/// AeroAPI, with key `k`, for a Settings job; a stand-in server in a debug
/// build if one is set.
fn aero_provider(
    k: String,
    budget: crate::aeroapi::Budget,
    spent: std::sync::Arc<std::sync::Mutex<crate::aeroapi::Spent>>,
) -> AeroApi {
    let p = AeroApi::new(k, budget, spent, Box::new(Net::new()));
    #[cfg(debug_assertions)]
    if let Ok(base) = std::env::var("NOW_BOARDING_AEROAPI_BASE") {
        return p.with_base(&base);
    }
    p
}
