//! The app: its settings, its boards in one window or two, the lamps and
//! the sound.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::{DateTime, Utc};
use egui::{vec2, Color32, Rect, Sense, Ui, ViewportBuilder, ViewportCommand, ViewportId};
use serde::{Deserialize, Serialize};
use splitflap::board::{fit_in, Flip};
use splitflap::lighting::{Failures, Lighting, Mood, Replace};
use splitflap::rng::Rng;
use splitflap::sound::{Click, Crowd, Sound};
use splitflap::{Look, Style};

use crate::aeroapi::{AeroApi, Budget, Spent};
use crate::clock::Clock;
use crate::demo::{self, Demo};
use crate::http::Net;
use crate::logos::Logos;
use crate::model::{Direction, Feed, Mode, Place};
use crate::normalise::{self, Operators, Spec};
use crate::provider::{NoKey, Provider};
use crate::ptv::Ptv;
use crate::view::{Setup, View};
use crate::{airports, keys, ptv};

mod settings;

const BOARD: Color32 = Color32::from_rgb(10, 10, 10);
const CAPTION: Color32 = Color32::from_rgb(130, 128, 118);
const YELLOW: Color32 = Color32::from_rgb(244, 196, 48);
/// Room round the board, and between stacked boards, in board units.
const PAD: f32 = 26.0;
const STACK_GAP: f32 = 40.0;
/// More flaps than this falling in one frame are heard as a crowd.
const CROWD: usize = 10;
/// The arrivals' own window, in points.
const ARRIVALS_SIZE: [f32; 2] = [1100.0, 860.0];
/// The chime rings at most this often, in seconds.
const CHIME_EVERY: f64 = 4.0;

/// How the boards sit on the screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Layout {
    /// One board: departures or arrivals.
    #[default]
    One,
    /// Departures above arrivals, in one window.
    Stacked,
    /// Departures and arrivals, each in a window of its own.
    TwoWindows,
}

/// What the app remembers between runs. New fields need a default so older
/// saves still load.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Saved {
    pub mode: Mode,
    pub direction: Direction,
    pub layout: Layout,
    /// Rows on a board; more movements than this go onto further pages.
    pub rows: usize,
    /// Seconds each page shows.
    pub page_secs: u32,
    /// The most pages, so the board shows the next few hours, not all day.
    pub max_pages: usize,
    /// Column widths changed from the preset, by kind of board.
    pub widths: BTreeMap<String, Vec<usize>>,
    /// A second-language title on a small line under the name; empty for
    /// none.
    pub second_title: String,
    pub mood: Mood,
    pub dimmer: f32,
    pub bulbs: Failures,
    /// How fast flaps fall, 1 being the usual.
    pub flip_speed: f32,
    pub sound: bool,
    pub volume: f32,
    /// The airport chime, for a gate or platform change and boarding.
    pub chime: bool,
    pub chime_volume: f32,
    /// How fast the demo's day runs: 1 for real time.
    pub demo_speed: f64,
    /// Where train boards' movements come from, and PTV's station. The
    /// keys aren't here: they're in the system key store.
    pub train_feed: Feed,
    pub ptv_place: Option<Place>,
    /// Where flight boards' movements come from, the airport, and
    /// AeroAPI's budget and what's been spent of it this month.
    pub flight_feed: Feed,
    pub airport: Place,
    pub budget: Budget,
    pub spent: Spent,
    /// Airline colour cards, and colours set here, by airline code.
    pub colour_cards: bool,
    pub airline_colours: Vec<(String, [u8; 3])>,
}

impl Default for Saved {
    fn default() -> Self {
        Self {
            mode: Mode::Flight,
            direction: Direction::Departure,
            layout: Layout::One,
            rows: 15,
            page_secs: 30,
            max_pages: 3,
            widths: BTreeMap::new(),
            second_title: String::new(),
            mood: Mood::Warm,
            dimmer: 1.0,
            bulbs: Failures::Sometimes,
            flip_speed: 1.0,
            sound: false,
            volume: 0.6,
            chime: true,
            chime_volume: 0.7,
            demo_speed: 1.0,
            train_feed: Feed::Demo,
            ptv_place: None,
            flight_feed: Feed::Demo,
            airport: airports::melbourne(),
            budget: Budget::default(),
            spent: Spent::default(),
            colour_cards: true,
            airline_colours: Vec::new(),
        }
    }
}

/// The key a kind of board's column widths are saved under.
pub fn widths_key(mode: Mode, direction: Direction) -> String {
    format!("{mode:?}-{direction:?}").to_lowercase()
}

impl Saved {
    /// The columns for a kind of board: the preset, with any widths
    /// changed.
    pub fn specs(&self, mode: Mode, direction: Direction) -> Vec<Spec> {
        let mut specs = normalise::preset(mode, direction);
        if let Some(w) = self.widths.get(&widths_key(mode, direction)) {
            if w.len() == specs.len() {
                for (s, &w) in specs.iter_mut().zip(w) {
                    s.cards = w.clamp(1, 40);
                }
            }
        }
        specs
    }

    /// How many AeroAPI boards share the budget.
    pub fn aero_boards(&self) -> u32 {
        if self.feed() != Feed::AeroApi {
            return 0;
        }
        match self.layout {
            Layout::One => 1,
            _ => 2,
        }
    }

    /// Where this board's movements come from.
    pub fn feed(&self) -> Feed {
        match self.mode {
            Mode::Flight => self.flight_feed,
            Mode::Train => self.train_feed,
        }
    }

    /// The boards to show, in order: the main window's first.
    fn setups(&self) -> Vec<Setup> {
        let tz = demo::local_tz();
        let [airport, station] = Demo::places(tz.name());
        let feed = self.feed();
        let place = match (self.mode, feed) {
            (_, Feed::AeroApi) => self.airport.clone(),
            (_, Feed::Ptv) => self.ptv_place.clone().unwrap_or_else(|| Place {
                id: "ptv:".into(),
                name: "Choose a station".into(),
                tz: "Australia/Melbourne".into(),
            }),
            (Mode::Flight, _) => airport,
            (Mode::Train, _) => station,
        };
        let directions = match (feed, self.layout) {
            // PTV gives departures only.
            (Feed::Ptv, _) => vec![Direction::Departure],
            (_, Layout::One) => vec![self.direction],
            (_, Layout::Stacked | Layout::TwoWindows) => {
                vec![Direction::Departure, Direction::Arrival]
            }
        };
        directions
            .into_iter()
            .map(|direction| Setup {
                mode: self.mode,
                direction,
                feed,
                place: place.clone(),
                rows: self.rows.clamp(4, 30),
                page_secs: self.page_secs.max(5) as f64,
                max_pages: self.max_pages.clamp(1, 10),
                specs: self.specs(self.mode, direction),
                second_title: self.second_title.clone(),
            })
            .collect()
    }
}

/// What can be set from the command line, for trying things out and for
/// screenshots: it wins over what was saved.
#[derive(Clone, Debug, Default)]
pub struct Overrides {
    pub speed: Option<f64>,
    /// The demo's time at start, as hours and minutes, today.
    pub start: Option<(u32, u32)>,
    pub layout: Option<Layout>,
    pub mode: Option<Mode>,
    pub direction: Option<Direction>,
    pub rows: Option<usize>,
    pub page_secs: Option<u32>,
    pub mood: Option<Mood>,
    pub settings: bool,
    /// Trains from PTV, at a station given as `Name=0:1181,3:1544`.
    pub ptv: bool,
    pub ptv_place: Option<Place>,
    /// Flights from AeroAPI, at an airport found by its code.
    pub aeroapi: bool,
    pub airport: Option<String>,
}

pub struct App {
    saved: Saved,
    /// PTV's developer ID and key, from the key store, and what went wrong
    /// reading them, if anything; and a count that goes up when they change,
    /// so the boards start again with them.
    ptv_keys: Option<ptv::Keys>,
    keys_error: Option<String>,
    keys_changed: u64,
    keys_applied: u64,
    /// The PTV page of Settings: what's typed, what's going on.
    ptv_form: settings::PtvForm,
    /// The AeroAPI key, from the key store, and what went wrong reading it.
    aero_key: Option<String>,
    aero_error: Option<String>,
    /// AeroAPI pages paid for this month, shared with the boards' pollers,
    /// and the budget the boards were made with.
    spent: Arc<Mutex<Spent>>,
    budget_applied: Budget,
    aero_form: settings::AeroForm,
    /// Airline logos the user has added.
    logos: Logos,
    clock: Clock,
    views: Vec<View>,
    /// The lamps over each window.
    lights: [Lighting; 2],
    sound: Option<Sound>,
    sound_error: Option<String>,
    sound_wanted: bool,
    sound_retry: f64,
    next_chime: f64,
    settings_open: bool,
    /// Where the arrivals' and the settings' windows open: beside the
    /// board, worked out when they open.
    arrivals_at: Option<egui::Pos2>,
    settings_at: Option<egui::Pos2>,
}

/// A spot `across` points to the right of the main window, level with its
/// top, where the system says where the main window is.
fn beside(ctx: &egui::Context, across: f32) -> Option<egui::Pos2> {
    ctx.input(|i| i.viewport().outer_rect)
        .map(|r| egui::pos2(r.max.x + 12.0 + across, r.min.y))
}

/// What a live board needs to start: keys, and AeroAPI's budget.
struct Live {
    ptv: Option<ptv::Keys>,
    aero: Option<String>,
    budget: Budget,
    spent: Arc<Mutex<Spent>>,
    /// AeroAPI boards sharing the budget.
    aero_boards: u32,
}

/// A board for `setup`: the demo on the demo's `clock`, a live feed on the
/// real time with its keys; with no key, a board that says so.
fn make_view(setup: Setup, clock: Clock, ctx: &egui::Context, time: f64, live: &Live) -> View {
    let real = Clock::starting(Utc::now(), 1.0);
    let tz: chrono_tz::Tz = setup.place.tz.parse().unwrap_or(chrono_tz::Tz::UTC);
    let (provider, every, clock): (Box<dyn Provider>, Duration, Clock) = match setup.feed {
        Feed::Demo => (
            Box::new(Demo::new(tz)),
            // Once a board minute, and at least every few seconds.
            Duration::from_secs_f64((60.0 / clock.speed().max(0.01)).clamp(1.0, 5.0)),
            clock,
        ),
        Feed::Ptv => match &live.ptv {
            Some(k) => {
                let p = Ptv::new(k.clone(), Box::new(Net::new()));
                #[cfg(debug_assertions)]
                let p = match std::env::var("NOW_BOARDING_PTV_BASE") {
                    Ok(base) => p.with_base(&base),
                    Err(_) => p,
                };
                (Box::new(p), ptv::EVERY, real)
            }
            None => (
                Box::new(NoKey::new("PTV NEEDS YOUR KEY: SEE SETTINGS")),
                Duration::from_secs(60),
                real,
            ),
        },
        Feed::AeroApi => match &live.aero {
            Some(k) => {
                let p = AeroApi::new(
                    k.clone(),
                    live.budget.clone(),
                    live.spent.clone(),
                    Box::new(Net::new()),
                );
                #[cfg(debug_assertions)]
                let p = match std::env::var("NOW_BOARDING_AEROAPI_BASE") {
                    Ok(base) => p.with_base(&base),
                    Err(_) => p,
                };
                (Box::new(p), live.budget.interval(live.aero_boards), real)
            }
            None => (
                Box::new(NoKey::new("AEROAPI NEEDS YOUR KEY: SEE SETTINGS")),
                Duration::from_secs(60),
                real,
            ),
        },
    };
    View::new(setup, clock, ctx, time, provider, every)
}

/// Today at `h`:`m` on the demo's clock, in UTC.
fn today_at(h: u32, m: u32) -> DateTime<Utc> {
    use chrono::TimeZone;
    let tz = demo::local_tz();
    let date = Utc::now().with_timezone(&tz).date_naive();
    date.and_hms_opt(h.min(23), m.min(59), 0)
        .and_then(|t| tz.from_local_datetime(&t).earliest())
        .map(demo::utc)
        .unwrap_or_else(Utc::now)
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, o: Overrides) -> Self {
        splitflap::install_fonts(&cc.egui_ctx);
        let mut saved: Saved = cc
            .storage
            .and_then(|s| eframe::get_value(s, eframe::APP_KEY))
            .unwrap_or_default();
        if let Some(v) = o.speed {
            saved.demo_speed = v;
        }
        if let Some(v) = o.layout {
            saved.layout = v;
        }
        if let Some(v) = o.mode {
            saved.mode = v;
        }
        if let Some(v) = o.direction {
            saved.direction = v;
        }
        if let Some(v) = o.rows {
            saved.rows = v;
        }
        if let Some(v) = o.page_secs {
            saved.page_secs = v;
        }
        if let Some(v) = o.mood {
            saved.mood = v;
        }
        if o.ptv {
            saved.mode = Mode::Train;
            saved.train_feed = Feed::Ptv;
        }
        if let Some(p) = o.ptv_place {
            saved.ptv_place = Some(p);
        }
        if o.aeroapi {
            saved.mode = Mode::Flight;
            saved.flight_feed = Feed::AeroApi;
        }
        if let Some(code) = &o.airport {
            if let Some(p) = airports::search(code, demo::local_tz().name())
                .into_iter()
                .next()
            {
                saved.airport = p;
            }
        }
        let (ptv_keys, keys_error) = match keys::load_ptv() {
            Ok(k) => (k, None),
            Err(e) => (None, Some(e)),
        };
        let (aero_key, aero_error) = match keys::load_aeroapi() {
            Ok(k) => (k, None),
            Err(e) => (None, Some(e)),
        };
        let spent = Arc::new(Mutex::new(saved.spent.clone()));
        let live = Live {
            ptv: ptv_keys.clone(),
            aero: aero_key.clone(),
            budget: saved.budget.clone(),
            spent: spent.clone(),
            aero_boards: saved.aero_boards(),
        };
        let start = o.start.map_or_else(Utc::now, |(h, m)| today_at(h, m));
        let clock = Clock::starting(start, saved.demo_speed);
        let time = cc.egui_ctx.input(|i| i.time);
        let views = saved
            .setups()
            .into_iter()
            .map(|s| make_view(s, clock, &cc.egui_ctx, time, &live))
            .collect();
        Self {
            budget_applied: saved.budget.clone(),
            saved,
            aero_key,
            aero_error,
            spent,
            aero_form: settings::AeroForm::default(),
            logos: Logos::default(),
            ptv_keys,
            keys_error,
            keys_changed: 0,
            keys_applied: 0,
            ptv_form: settings::PtvForm::default(),
            clock,
            views,
            lights: [Lighting::new(Rng::seeded()), Lighting::new(Rng::seeded())],
            sound: None,
            sound_error: None,
            sound_wanted: false,
            sound_retry: 0.0,
            next_chime: 0.0,
            settings_open: o.settings,
            arrivals_at: None,
            settings_at: None,
        }
    }

    /// Makes the boards again when the settings say different boards.
    fn apply(&mut self, ctx: &egui::Context, time: f64) {
        if (self.clock.speed() - self.saved.demo_speed).abs() > 1e-9 {
            self.clock = self.clock.with_speed(self.saved.demo_speed);
            self.views.clear();
        }
        let setups = self.saved.setups();
        let same = setups.len() == self.views.len()
            && setups.iter().zip(&self.views).all(|(s, v)| *s == v.setup)
            && self.keys_applied == self.keys_changed
            && self.budget_applied == self.saved.budget;
        if !same {
            self.keys_applied = self.keys_changed;
            self.budget_applied = self.saved.budget.clone();
            let live = self.live();
            self.views = setups
                .into_iter()
                .map(|s| make_view(s, self.clock, ctx, time, &live))
                .collect();
        }
    }

    fn live(&self) -> Live {
        Live {
            ptv: self.ptv_keys.clone(),
            aero: self.aero_key.clone(),
            budget: self.saved.budget.clone(),
            spent: self.spent.clone(),
            aero_boards: self.saved.aero_boards(),
        }
    }

    /// Restarts the demo's day at `h`:`m`.
    fn restart_at(&mut self, h: u32, m: u32) {
        self.clock = Clock::starting(today_at(h, m), self.saved.demo_speed);
        self.views.clear();
    }

    fn apply_sound(&mut self, time: f64) {
        let s = &self.saved;
        let bulbs = s.mood != Mood::Off && s.bulbs != Failures::Never;
        let want = s.sound || s.chime || bulbs;
        if want != self.sound_wanted {
            self.sound_wanted = want;
            self.sound = None;
            self.sound_error = None;
            self.sound_retry = 0.0;
        }
        if want && self.sound.is_none() && time >= self.sound_retry {
            match Sound::open(self.saved.volume) {
                Ok(s) => self.sound = Some(s),
                Err(e) => {
                    self.sound_error = Some(e);
                    self.sound_retry = time + 15.0;
                }
            }
        }
        if let Some(s) = &self.sound {
            s.set_volume(self.saved.volume);
        }
    }

    /// The clatter of `flips`: a click each, or past a handful, a crowd.
    fn clatter(&mut self, flips: &[Flip]) {
        let (true, Some(sound)) = (self.saved.sound, &mut self.sound) else {
            return;
        };
        if flips.len() <= CROWD {
            for f in flips {
                sound.click(Click {
                    pan: f.pan,
                    size: f.size,
                    land: f.fall.land,
                });
            }
            return;
        }
        let n = flips.len() as f32;
        let pan = flips.iter().map(|f| f.pan).sum::<f32>() / n;
        let spread = (flips.iter().map(|f| (f.pan - pan).powi(2)).sum::<f32>() / n).sqrt();
        let size = flips.iter().map(|f| f.size).sum::<f32>() / n;
        sound.crowd(Crowd {
            count: flips.len() as u32,
            pan,
            spread,
            size,
        });
    }

    fn style(&self) -> Style {
        Style {
            look: Look {
                top: Color32::from_rgb(36, 36, 35),
                bottom: Color32::from_rgb(29, 29, 28),
                split: Color32::from_rgb(4, 4, 4),
                hinge: Color32::from_rgb(70, 70, 68),
                ink: YELLOW,
            },
            caption: CAPTION,
            shadow: self.saved.mood.shadow(),
        }
    }

    /// Draws the boards `which` into the window `ui`, lit by lamps
    /// `lamps`, and says which flaps fell.
    fn window(&mut self, ui: &mut Ui, which: &[usize], lamps: usize, time: f64) -> Vec<Flip> {
        let ctx = ui.ctx().clone();
        let full = ui.max_rect();
        let painter = ui.painter_at(full);
        let speed = self.saved.flip_speed.clamp(0.25, 4.0) as f64;
        let mut flips = Vec::new();
        let sizes: Vec<_> = which.iter().map(|&i| self.views[i].size()).collect();
        let size = vec2(
            sizes.iter().map(|s| s.x).fold(0.0, f32::max),
            sizes.iter().map(|s| s.y).sum::<f32>() + STACK_GAP * (sizes.len() as f32 - 1.0),
        );
        let (origin, scale) = fit_in(full, size, PAD);
        // The lamps light the boards as they're drawn.
        let mood = self.saved.mood;
        let light = if mood != Mood::Off {
            let n = ((size.x / 230.0).round() as usize).max(2);
            self.lights[lamps].update(n, time, self.saved.bulbs, Replace::Automatically);
            self.lights[lamps].map(full, mood, self.saved.dimmer)
        } else {
            None
        };
        match &light {
            Some(l) => {
                painter.add(l.backdrop(BOARD));
            }
            None => {
                painter.rect_filled(full, 0.0, BOARD);
            }
        }
        let style = self.style();
        let mut y = 0.0;
        for (&i, s) in which.iter().zip(&sizes) {
            let v = &mut self.views[i];
            flips.extend(v.tick(time, speed));
            v.draw(
                &ctx,
                &painter,
                origin + vec2(0.0, y) * scale,
                scale,
                &style,
                light.as_ref(),
            );
            y += s.y + STACK_GAP;
        }
        self.lights[lamps].draw_lamps_only(&painter, full, mood, self.saved.dimmer, time);
        let (pops, n) = self.lights[lamps].take_pops();
        if let Some(sound) = &mut self.sound {
            for i in pops {
                let pan = (i as f32 + 0.5) / n.max(1) as f32 * 2.0 - 1.0;
                sound.pop(pan, 0.5);
            }
        }
        self.menu(ui, full);
        flips
    }

    /// Right-click for the menu, double-click for the settings, and a
    /// settings button in the corner while the pointer is over the board.
    fn menu(&mut self, ui: &mut Ui, full: Rect) {
        let response = ui.interact(full, ui.id().with("board"), Sense::click());
        if response.double_clicked() {
            self.settings_open = true;
        }
        response.context_menu(|ui| {
            if ui.button("Settings…").clicked() {
                self.settings_open = true;
                ui.close();
            }
            if ui.checkbox(&mut self.saved.sound, "Clatter").changed() {
                ui.close();
            }
            if ui.checkbox(&mut self.saved.chime, "Chime").changed() {
                ui.close();
            }
            ui.separator();
            if ui.button("Quit").clicked() {
                ui.ctx()
                    .send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Close);
            }
        });
        if ui.rect_contains_pointer(full) && !self.settings_open {
            let b = Rect::from_min_size(full.right_top() + vec2(-30.0, 6.0), vec2(24.0, 24.0));
            let r = ui.put(
                b,
                egui::Button::new(egui::RichText::new("⚙").size(15.0)).frame(false),
            );
            if r.on_hover_text("Settings (or right-click the board)")
                .clicked()
            {
                self.settings_open = true;
            }
        }
    }

    fn title(&self) -> String {
        match self.saved.mode {
            Mode::Flight => "Now Boarding — Flight Board".into(),
            Mode::Train => "Now Boarding — Train Board".into(),
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let time = ctx.input(|i| i.time);
        self.apply(&ctx, time);
        self.apply_sound(time);
        ctx.send_viewport_cmd(ViewportCommand::Title(self.title()));

        // What the pollers have brought, and what to announce.
        let mut announce = Vec::new();
        for v in &mut self.views {
            announce.extend(v.take_updates(time));
        }
        if !announce.is_empty() && self.saved.chime && time >= self.next_chime {
            if let Some(s) = &mut self.sound {
                s.chime(self.saved.chime_volume);
                self.next_chime = time + CHIME_EVERY;
            }
        }
        let now = self.clock.now();
        let logos = &mut self.logos;
        let mut logo = |code: &str| logos.get(&ctx, code);
        let mut ops = Operators {
            colours: self.saved.colour_cards,
            own: &self.saved.airline_colours,
            logo: &mut logo,
        };
        for v in &mut self.views {
            v.show(time, now, Some(&mut ops));
        }

        let main: Vec<usize> = match self.saved.layout {
            Layout::Stacked => (0..self.views.len()).collect(),
            _ => vec![0],
        };
        let mut flips = self.window(ui, &main, 0, time);
        if self.saved.layout == Layout::TwoWindows && self.views.len() > 1 {
            if self.arrivals_at.is_none() {
                self.arrivals_at = beside(&ctx, 0.0);
            }
            let mut builder = ViewportBuilder::default()
                .with_title(format!("{} — Arrivals", self.title()))
                .with_inner_size(ARRIVALS_SIZE);
            if let Some(at) = self.arrivals_at {
                builder = builder.with_position(at);
            }
            let more = ctx.show_viewport_immediate(
                ViewportId::from_hash_of("arrivals"),
                builder,
                |ui, _| {
                    if ui.input(|i| i.viewport().close_requested()) {
                        self.saved.layout = Layout::One;
                    }
                    self.window(ui, &[1], 1, time)
                },
            );
            flips.extend(more);
        }
        self.clatter(&flips);

        if self.saved.layout != Layout::TwoWindows {
            self.arrivals_at = None;
        }
        if self.settings_open {
            if self.settings_at.is_none() {
                // Beyond the arrivals' window, if that's beside the board.
                let across = if self.saved.layout == Layout::TwoWindows {
                    ARRIVALS_SIZE[0] + 12.0
                } else {
                    0.0
                };
                self.settings_at = beside(&ctx, across);
            }
            self.settings(&ctx);
        } else {
            self.settings_at = None;
        }

        let busy = self.views.iter().any(|v| v.busy(time)) || !flips.is_empty();
        if busy {
            ctx.request_repaint();
        } else {
            // The clock's minute, the next page and the ticker come round
            // in their own time: a few looks a second is plenty.
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        if let Ok(spent) = self.spent.lock() {
            self.saved.spent = spent.clone();
        }
        eframe::set_value(storage, eframe::APP_KEY, &self.saved);
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        BOARD.to_normalized_gamma_f32()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_saves_still_load() {
        let s: Saved = ron::from_str("(rows: 12)").unwrap();
        assert_eq!(s.rows, 12);
        assert_eq!(s.page_secs, 30);
        assert_eq!(s.layout, Layout::One);
    }

    #[test]
    fn changed_widths_apply_to_their_board_only() {
        let mut s = Saved::default();
        s.widths.insert(
            widths_key(Mode::Flight, Direction::Departure),
            vec![5, 2, 5, 20, 4, 12],
        );
        assert_eq!(s.specs(Mode::Flight, Direction::Departure)[3].cards, 20);
        assert_eq!(s.specs(Mode::Flight, Direction::Arrival)[3].cards, 16);
        // Widths for a different number of columns are ignored.
        s.widths
            .insert(widths_key(Mode::Train, Direction::Arrival), vec![1, 2]);
        assert_eq!(s.specs(Mode::Train, Direction::Arrival)[1].cards, 18);
    }

    #[test]
    fn layouts_make_their_boards() {
        let mut s = Saved::default();
        assert_eq!(s.setups().len(), 1);
        s.layout = Layout::Stacked;
        let b = s.setups();
        assert_eq!(b.len(), 2);
        assert_eq!(b[0].direction, Direction::Departure);
        assert_eq!(b[1].direction, Direction::Arrival);
    }
}
