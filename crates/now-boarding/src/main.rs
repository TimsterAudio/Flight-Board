// No console window on Windows.
#![cfg_attr(windows, windows_subsystem = "windows")]

mod app;
mod clock;
mod demo;
mod model;
mod normalise;
mod poller;
mod provider;
mod view;

use app::{Layout, Overrides};
use model::{Direction, Mode};
use splitflap::lighting::Mood;

/// The command line, for trying things out:
///
///   --speed N       the demo's day runs N times as fast
///   --start HH:MM   the demo's day starts at this time today
///   --layout one|stacked|two
///   --trains, --flights, --arrivals, --departures
///   --rows N, --page SECONDS
///   --mood off|soft|warm|evening
///   --settings      open the settings at start
fn overrides() -> Overrides {
    let mut o = Overrides::default();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        let value = args.get(i + 1).cloned().unwrap_or_default();
        let mut takes = true;
        match args[i].as_str() {
            "--speed" => o.speed = value.parse().ok(),
            "--start" => {
                o.start = value
                    .split_once(':')
                    .and_then(|(h, m)| Some((h.parse().ok()?, m.parse().ok()?)))
            }
            "--layout" => {
                o.layout = match value.as_str() {
                    "stacked" => Some(Layout::Stacked),
                    "two" => Some(Layout::TwoWindows),
                    _ => Some(Layout::One),
                }
            }
            "--rows" => o.rows = value.parse().ok(),
            "--page" => o.page_secs = value.parse().ok(),
            "--mood" => {
                o.mood = match value.as_str() {
                    "off" => Some(Mood::Off),
                    "soft" => Some(Mood::Soft),
                    "evening" => Some(Mood::Evening),
                    _ => Some(Mood::Warm),
                }
            }
            flag => {
                takes = false;
                match flag {
                    "--trains" => o.mode = Some(Mode::Train),
                    "--flights" => o.mode = Some(Mode::Flight),
                    "--arrivals" => o.direction = Some(Direction::Arrival),
                    "--departures" => o.direction = Some(Direction::Departure),
                    "--settings" => o.settings = true,
                    other => eprintln!("now-boarding: ignoring {other}"),
                }
            }
        }
        i += if takes { 2 } else { 1 };
    }
    o
}

fn main() -> eframe::Result {
    let o = overrides();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Now Boarding")
            .with_app_id("now-boarding")
            .with_inner_size([1180.0, 940.0])
            .with_min_inner_size([320.0, 240.0]),
        event_loop_builder: event_loop_builder(),
        ..Default::default()
    };
    eframe::run_native(
        "Now Boarding",
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc, o)))),
    )
}

/// On Linux the board runs as an X11 window wherever X11 is there, XWayland
/// included, as the clock does.
#[cfg(target_os = "linux")]
fn event_loop_builder() -> Option<eframe::EventLoopBuilderHook> {
    use winit::platform::x11::EventLoopBuilderExtX11;
    let x11 = std::env::var_os("DISPLAY").is_some_and(|d| !d.is_empty());
    x11.then(|| {
        Box::new(|b: &mut eframe::EventLoopBuilder<_>| {
            b.with_x11();
        }) as eframe::EventLoopBuilderHook
    })
}

#[cfg(not(target_os = "linux"))]
fn event_loop_builder() -> Option<eframe::EventLoopBuilderHook> {
    None
}
