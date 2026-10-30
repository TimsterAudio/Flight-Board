//! How long a 24-row board takes egui to lay out and tessellate, with no
//! window and no GPU: the board standing still, and every card flipping.
//!
//! cargo run --release -p splitflap --example cpu_bench [shadow, 0 to 1]

use std::time::Instant;

use egui::{pos2, vec2, Color32, Rect};
use splitflap::flap::REMARKS;
use splitflap::rng::Rng;
use splitflap::{Board, Column, Look, Row, Style};

fn main() {
    let ctx = egui::Context::default();
    splitflap::install_fonts(&ctx);
    let mut board = Board::new(
        vec![
            Column::time("TIME", 5),
            Column::text("FLIGHT", 7),
            Column::text("DESTINATION", 16),
            Column::text("GATE", 4),
            Column::words("REMARKS", 12, REMARKS),
        ],
        24,
        0.0,
        Rng::with_seed(1),
    );
    let rows = |k: usize| -> Vec<Row> {
        (0..24)
            .map(|i| {
                let n = i + k;
                Row::new(&[
                    format!("{:02}:{:02}", 6 + n / 6, (n % 6) * 10),
                    format!("NB {}", 100 + n * 7),
                    ["LONDON", "AUCKLAND", "SINGAPORE", "TOKYO HANEDA"][n % 4].to_string(),
                    format!("{}", n % 17),
                    ["ON TIME", "BOARDING", "DELAYED"][n % 3].to_string(),
                ])
            })
            .collect()
    };
    let style = Style {
        look: Look {
            top: Color32::from_rgb(36, 36, 35),
            bottom: Color32::from_rgb(29, 29, 28),
            split: Color32::from_rgb(4, 4, 4),
            hinge: Color32::from_rgb(70, 70, 68),
            ink: Color32::from_rgb(244, 196, 48),
        },
        caption: Color32::GRAY,
        shadow: std::env::args()
            .nth(1)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0.0),
    };
    let screen = Rect::from_min_size(pos2(0.0, 0.0), vec2(1180.0, 1080.0));
    let frame = |time: f64, board: &mut Board| -> (f64, f64, usize, usize) {
        let input = egui::RawInput {
            screen_rect: Some(screen),
            time: Some(time),
            ..Default::default()
        };
        let t0 = Instant::now();
        let out = ctx.run_ui(input, |ui| {
            board.tick(time, 1.0);
            let painter = ui.painter().clone();
            board.draw(ui.ctx(), &painter, pos2(20.0, 20.0), 0.95, &style);
        });
        let t1 = Instant::now();
        let prims = ctx.tessellate(out.shapes, 1.0);
        let t2 = Instant::now();
        let vertices: usize = prims
            .iter()
            .map(|p| match &p.primitive {
                egui::epaint::Primitive::Mesh(m) => m.vertices.len(),
                _ => 0,
            })
            .sum();
        (
            (t1 - t0).as_secs_f64() * 1000.0,
            (t2 - t1).as_secs_f64() * 1000.0,
            prims.len(),
            vertices,
        )
    };
    board.set_rows(&rows(0), 0.0);
    let mut t = 0.0;
    while board.busy(t) {
        frame(t, &mut board);
        t += 1.0 / 60.0;
    }
    let report = |name: &str, board: &mut Board, t: &mut f64, n: usize| {
        let (mut ui, mut tess, mut prims, mut verts) = (0.0, 0.0, 0, 0);
        for _ in 0..n {
            let (a, b, p, v) = frame(*t, board);
            ui += a;
            tess += b;
            prims += p;
            verts += v;
            *t += 1.0 / 60.0;
        }
        let n = n as f64;
        println!(
            "{name}: building {:.2} ms · tessellating {:.2} ms · {:.0} draw calls and {:.0} vertices a frame",
            ui / n,
            tess / n,
            prims as f64 / n,
            verts as f64 / n
        );
    };
    report("standing still", &mut board, &mut t, 120);
    board.set_rows(&rows(1), t);
    t += 0.3;
    report("rows moving up", &mut board, &mut t, 60);
    board.refresh(t);
    t += 0.5;
    report("every card flipping", &mut board, &mut t, 60);
}
