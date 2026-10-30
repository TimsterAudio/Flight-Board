//! The gallery: a made-up airport's departures on a full board, the top
//! flight leaving every few seconds and the rest moving up, to look at the
//! cards and to time them.
//!
//! cargo run --release -p splitflap --example gallery -- [options]
//!
//!   --rows N       rows on the board, 8 to 24 (default 24)
//!   --every S      seconds between departures (default 4)
//!   --first S      seconds to the first departure (default 3 more)
//!   --mood M       off, soft, warm or evening (default warm)
//!   --sound        play the clatter
//!   --no-shadow    no shadows under the cards
//!   --size WxH     the window's size, in points
//!   --bench S      run for S seconds as fast as it will go, print the
//!                  frame rate, and quit

use std::time::Instant;

use egui::{vec2, Align2, Color32, FontFamily, FontId, Vec2};
use splitflap::board::{fit_in, FONT};
use splitflap::flap::REMARKS;
use splitflap::lighting::{Failures, Lighting, Mood, Replace};
use splitflap::rng::Rng;
use splitflap::sound::{Click, Sound};
use splitflap::{Board, Column, Look, Row, Style};

const BOARD: Color32 = Color32::from_rgb(10, 10, 10);
const CAPTION: Color32 = Color32::from_rgb(130, 128, 118);
const YELLOW: Color32 = Color32::from_rgb(244, 196, 48);
const AMBER: Color32 = Color32::from_rgb(240, 132, 32);
const RED: Color32 = Color32::from_rgb(222, 56, 44);
const WHITE: Color32 = Color32::from_rgb(236, 234, 226);
/// The big cards of the title and the time.
const BIG: Vec2 = vec2(44.0, 64.0);
const PAD: f32 = 26.0;

/// A day of made-up flights: number, destination, gate, and how it's going.
const FLIGHTS: &[(&str, &str, &str, &str)] = &[
    ("NB 101", "London Heathrow", "5", ""),
    ("SK 22", "Zürich", "11", ""),
    ("TA 318", "Auckland", "3", "DELAYED"),
    ("NB 7", "Singapore", "9", ""),
    ("OC 450", "Wellington", "2", ""),
    ("RV 66", "São Paulo Guarulhos", "14", ""),
    ("NB 233", "Hobart", "6", "CANCELLED"),
    ("KJ 12", "Tokyo Haneda", "8", ""),
    ("SK 901", "Kuala Lumpur International", "12", ""),
    ("TA 77", "Christchurch", "4", ""),
    ("NB 418", "Perth", "1", "DELAYED"),
    ("LX 15", "Los Angeles", "10", ""),
    ("OC 3", "Nadi", "7", ""),
    ("RV 210", "Düsseldorf", "15", ""),
    ("NB 55", "Adelaide", "2", ""),
    ("KJ 640", "Hong Kong", "13", ""),
    ("SK 8", "Dubai", "9", ""),
    ("TA 19", "Queenstown", "5", ""),
    ("NB 312", "Brisbane", "3", ""),
    ("LX 400", "San Francisco", "11", "DELAYED"),
    ("OC 88", "Denpasar", "6", ""),
    ("RV 5", "Johannesburg O.R. Tambo", "16", ""),
    ("NB 700", "Sydney", "4", ""),
    ("KJ 21", "Seoul Incheon", "8", ""),
    ("SK 160", "Doha", "12", ""),
    ("TA 302", "Rarotonga", "1", "CANCELLED"),
    ("NB 9", "Darwin", "7", ""),
    ("LX 81", "Vancouver", "10", ""),
    ("OC 515", "Port Moresby", "2", ""),
    ("RV 44", "Malmö", "15", ""),
];

struct Options {
    rows: usize,
    every: f64,
    first: Option<f64>,
    mood: Mood,
    sound: bool,
    shadow: bool,
    bench: Option<f64>,
    size: Option<[f32; 2]>,
}

fn options() -> Options {
    let mut o = Options {
        rows: 24,
        every: 4.0,
        first: None,
        mood: Mood::Warm,
        sound: false,
        shadow: true,
        bench: None,
        size: None,
    };
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        let value = args.get(i + 1).map(String::as_str).unwrap_or("");
        match args[i].as_str() {
            "--rows" => o.rows = value.parse().unwrap_or(24).clamp(8, 24),
            "--every" => o.every = value.parse().unwrap_or(4.0f64).max(0.5),
            "--bench" => o.bench = value.parse().ok(),
            "--first" => o.first = value.parse().ok(),
            "--size" => {
                o.size = value
                    .split_once('x')
                    .and_then(|(w, h)| Some([w.parse().ok()?, h.parse().ok()?]))
            }
            "--mood" => {
                o.mood = match value {
                    "off" => Mood::Off,
                    "soft" => Mood::Soft,
                    "evening" => Mood::Evening,
                    _ => Mood::Warm,
                }
            }
            "--sound" => {
                o.sound = true;
                i += 1;
                continue;
            }
            "--no-shadow" => {
                o.shadow = false;
                i += 1;
                continue;
            }
            other => eprintln!("gallery: ignoring {other}"),
        }
        i += 2;
    }
    o
}

struct Gallery {
    o: Options,
    title: Board,
    place: Board,
    board: Board,
    lighting: Lighting,
    sound: Option<Sound>,
    /// The next flight to come onto the foot of the board, by its place in
    /// the day, and when the top one next leaves.
    next: usize,
    next_departure: f64,
    started: Instant,
    last_frame: Option<Instant>,
    frames: Vec<f32>,
    /// Each frame's own time, building the board's shapes, and all the
    /// work the frame took before it went to the screen.
    built: Vec<f32>,
    cpu: Vec<f32>,
    reported: bool,
    /// Frames drawn while cards were flipping.
    busy_frames: usize,
}

/// The departure time of the `n`th flight of the day.
fn time_of(n: usize) -> String {
    let m = 6 * 60 + 5 + n * 10;
    format!("{:02}:{:02}", (m / 60) % 24, m % 60)
}

impl Gallery {
    fn new(cc: &eframe::CreationContext<'_>, o: Options) -> Self {
        splitflap::install_fonts(&cc.egui_ctx);
        let now = 0.0;
        let title = Board::new(
            vec![Column::text("", 10), Column::time("", 5)],
            1,
            now,
            Rng::seeded(),
        )
        .with_card(BIG);
        let place = Board::new(vec![Column::text("", 24)], 1, now, Rng::seeded());
        let board = Board::new(
            vec![
                Column::time("TIME", 5),
                Column::text("FLIGHT", 7),
                Column::text("DESTINATION", 16),
                Column::text("GATE", 4),
                Column::words("REMARKS", 12, REMARKS),
            ],
            o.rows,
            now,
            Rng::seeded(),
        );
        let sound = if o.sound {
            Sound::open(0.6).map_err(|e| eprintln!("gallery: {e}")).ok()
        } else {
            None
        };
        let mut g = Self {
            title,
            place,
            board,
            lighting: Lighting::new(Rng::seeded()),
            sound,
            next: 0,
            next_departure: o.first.unwrap_or(o.every + 3.0),
            started: Instant::now(),
            last_frame: None,
            frames: Vec::new(),
            built: Vec::new(),
            cpu: Vec::new(),
            reported: false,
            busy_frames: 0,
            o,
        };
        g.show(now);
        g
    }

    /// The rows for the flights from `self.next` on: the first boarding,
    /// the second's gate open, and so on down.
    fn rows(&self) -> Vec<Row> {
        (0..self.o.rows)
            .map(|k| {
                let n = self.next + k;
                let (flight, to, gate, how) = FLIGHTS[n % FLIGHTS.len()];
                let remark = match (k, how) {
                    (_, "CANCELLED") => "CANCELLED",
                    (0, _) => "FINAL CALL",
                    (1, _) => "BOARDING",
                    (2, _) => "GATE OPEN",
                    (_, "") => "ON TIME",
                    (_, how) => how,
                };
                let tab = match remark {
                    "FINAL CALL" | "BOARDING" => Some(YELLOW),
                    "DELAYED" => Some(AMBER),
                    "CANCELLED" => Some(RED),
                    _ => None,
                };
                let gate = if remark == "CANCELLED" { "" } else { gate };
                let row = Row::new(&[&time_of(n), flight, to, gate, remark]);
                match tab {
                    Some(t) => row.tab(t),
                    None => row,
                }
            })
            .collect()
    }

    fn show(&mut self, now: f64) {
        let first = time_of(self.next);
        // The hall's clock: five minutes before the first departure.
        let clock = {
            let (h, m) = first.split_once(':').unwrap_or(("0", "0"));
            let m = (h.parse::<i32>().unwrap_or(0) * 60 + m.parse::<i32>().unwrap_or(0) - 5)
                .rem_euclid(24 * 60);
            format!("{:02}:{:02}", m / 60, m % 60)
        };
        self.title
            .set_rows(&[Row::new(&["DEPARTURES", clock.as_str()])], now);
        self.place
            .set_rows(&[Row::new(&["GATE 05 INTERNATIONAL"]).ink(WHITE)], now);
        let rows = self.rows();
        self.board.set_rows(&rows, now);
    }

    fn look() -> Look {
        Look {
            top: Color32::from_rgb(36, 36, 35),
            bottom: Color32::from_rgb(29, 29, 28),
            split: Color32::from_rgb(4, 4, 4),
            hinge: Color32::from_rgb(70, 70, 68),
            ink: YELLOW,
        }
    }

    fn time_frame(&mut self, busy: bool) {
        let now = Instant::now();
        if let Some(last) = self.last_frame {
            self.frames.push((now - last).as_secs_f32());
            if busy {
                self.busy_frames += 1;
            }
        }
        self.last_frame = Some(now);
    }

    fn report(&self) {
        let mut f = self.frames.clone();
        if f.is_empty() {
            return;
        }
        f.sort_by(|a, b| a.total_cmp(b));
        let total: f32 = f.iter().sum();
        let mean = f.len() as f32 / total;
        let p99 = f[((f.len() as f32 * 0.99) as usize).min(f.len() - 1)];
        let p50 = f[f.len() / 2];
        println!(
            "rows {} · frames {} · mean {:.1} fps · median {:.1} ms · 99th percentile {:.1} ms ({:.1} fps) · worst {:.1} ms · flipping in {:.0}% of frames",
            self.o.rows,
            f.len(),
            mean,
            p50 * 1000.0,
            p99 * 1000.0,
            1.0 / p99,
            f[f.len() - 1] * 1000.0,
            100.0 * self.busy_frames as f32 / f.len() as f32
        );
        let mean = |v: &[f32]| 1000.0 * v.iter().sum::<f32>() / v.len().max(1) as f32;
        println!(
            "  building the shapes {:.1} ms · the frame's work before the screen {:.1} ms",
            mean(&self.built),
            mean(&self.cpu)
        );
    }
}

impl eframe::App for Gallery {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let began = Instant::now();
        if let Some(cpu) = frame.info().cpu_usage {
            self.cpu.push(cpu);
        }
        let ctx = ui.ctx().clone();
        let time = ctx.input(|i| i.time);
        if time >= self.next_departure {
            self.next += 1;
            self.next_departure = time + self.o.every;
            self.show(time);
        }

        let mut flips = self.title.tick(time, 1.0);
        flips.extend(self.place.tick(time, 1.0));
        flips.extend(self.board.tick(time, 1.0));
        if let Some(sound) = &mut self.sound {
            for f in flips.iter().take(48) {
                sound.click(Click {
                    pan: f.pan,
                    size: f.size,
                    land: f.fall.land,
                });
            }
        }
        let busy = self.title.busy(time) || self.place.busy(time) || self.board.busy(time);

        // The title and the time over the place, over the board.
        let full = ui.max_rect();
        let painter = ui.painter_at(full);
        // The lamps light the board as it's drawn.
        let light = if self.o.mood != Mood::Off {
            let lamps = ((self.board.size().x / 230.0).round() as usize).max(2);
            self.lighting
                .update(lamps, time, Failures::Never, Replace::Automatically);
            self.lighting.map(full, self.o.mood, 1.0)
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
        let (ts, ps, bs) = (self.title.size(), self.place.size(), self.board.size());
        let size = vec2(
            bs.x.max(ts.x).max(ps.x),
            ts.y + 10.0 + ps.y + 18.0 + bs.y + 30.0,
        );
        let (origin, scale) = fit_in(full, size, PAD);
        let style = Style {
            look: Self::look(),
            caption: CAPTION,
            shadow: if self.o.shadow {
                self.o.mood.shadow()
            } else {
                0.0
            },
        };
        let mut y = 0.0;
        for (b, h) in [(&self.title, 10.0), (&self.place, 18.0), (&self.board, 0.0)] {
            let at = origin + vec2(0.0, y) * scale;
            b.draw_lit(&ctx, &painter, at, scale, &style, light.as_ref());
            y += b.size().y + h;
        }
        let small = FontId::new(11.0 * scale, FontFamily::Name(FONT.into()));
        let foot = origin + vec2(0.0, y + 16.0) * scale;
        painter.text(
            foot,
            Align2::LEFT_CENTER,
            "DEMO FEED · MADE-UP FLIGHTS · NOT FOR TRAVEL",
            small.clone(),
            light.as_ref().map_or(CAPTION, |l| l.light(foot, CAPTION)),
        );

        // The lamps over it all.
        self.lighting
            .draw_lamps_only(&painter, full, self.o.mood, 1.0, time);

        // The frame rate, in the corner.
        self.built.push(began.elapsed().as_secs_f32());
        self.time_frame(busy);
        let recent = &self.frames[self.frames.len().saturating_sub(60)..];
        if !recent.is_empty() {
            let fps = recent.len() as f32 / recent.iter().sum::<f32>();
            painter.text(
                full.right_bottom() + vec2(-8.0, -6.0),
                Align2::RIGHT_BOTTOM,
                format!("{fps:.0} fps · {} cards", count_cards(&self.board)),
                FontId::monospace(11.0),
                CAPTION,
            );
        }

        if let Some(secs) = self.o.bench {
            if self.started.elapsed().as_secs_f64() >= secs {
                if !std::mem::replace(&mut self.reported, true) {
                    self.report();
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            ctx.request_repaint();
        } else if busy {
            ctx.request_repaint();
        } else {
            let wait = (self.next_departure - time).max(0.0);
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(wait));
        }
    }
}

fn count_cards(b: &Board) -> usize {
    b.columns()
        .iter()
        .map(|c| match c.drum {
            splitflap::Drum::Words(_) => 1,
            _ => c.cards,
        })
        .sum::<usize>()
        * b.row_count()
}

fn main() -> eframe::Result {
    let o = options();
    let size = o.size.unwrap_or(if o.rows > 16 {
        [1180.0, 1080.0]
    } else {
        [1180.0, 720.0]
    });
    let native = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Split-flap gallery")
            .with_inner_size(size),
        ..Default::default()
    };
    eframe::run_native(
        "Split-flap gallery",
        native,
        Box::new(|cc| Ok(Box::new(Gallery::new(cc, o)))),
    )
}
