//! The settings window: which board, how many rows and pages, the columns,
//! the look and the sound, the demo's clock, and a word about the data.

use egui::{vec2, RichText, Ui, ViewportBuilder, ViewportId};
use splitflap::lighting::{Failures, Mood};

use super::{widths_key, App, Layout};
use crate::model::{Direction, Mode};
use crate::normalise;

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
        ui.label(match s.mode {
            Mode::Flight => "Feed: the demo airport, Gate 05 International.",
            Mode::Train => "Feed: the demo station, Harbour Street.",
        });
        ui.label(RichText::new("Live feeds, with your own key, come next.").weak());

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
        ui.label(
            RichText::new(format!("Now Boarding {}", env!("CARGO_PKG_VERSION")))
                .weak()
                .small(),
        );
        ui.add_space(vec2(0.0, 12.0).y);
    }
}
