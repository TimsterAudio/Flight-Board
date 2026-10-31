//! The settings window: which board, how many rows and pages, the columns,
//! the look and the sound, the demo's clock, and a word about the data.

use egui::{vec2, RichText, Ui, ViewportBuilder, ViewportId};
use splitflap::lighting::{Failures, Mood};

use super::{widths_key, App, Layout};
use crate::http::Net;
use crate::model::{Direction, Feed, Mode, Place};
use crate::provider::Provider;
use crate::ptv::{self, Ptv};
use crate::task::Task;
use crate::{keys, normalise};

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
            "Departures above arrivals, in one window",
        );
        ui.radio_value(
            &mut s.layout,
            Layout::TwoWindows,
            "Departures and arrivals, in a window each",
        );
        ui.add_space(4.0);
        match s.mode {
            Mode::Flight => {
                ui.label("Feed: the demo airport, Gate 05 International.");
                ui.label(RichText::new("Live flights, with your own key, come next.").weak());
            }
            Mode::Train => {
                ui.horizontal(|ui| {
                    ui.label("Feed");
                    ui.selectable_value(&mut s.train_feed, Feed::Demo, "Demo: Harbour Street");
                    ui.selectable_value(&mut s.train_feed, Feed::Ptv, "PTV: Victoria's trains");
                });
            }
        }
        if s.feed() == Feed::Ptv {
            self.ptv_page(ui);
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
