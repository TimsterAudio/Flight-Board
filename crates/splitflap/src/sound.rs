//! The board's clatter: a click for every flap that falls, synthesised as it
//! plays, so there are no recordings to carry.
//!
//! A click is a very short burst of noise, the flap striking its stop, rung
//! through two resonances, the flap's own ring and the module's body. A
//! flap that lands adds a low knock. Every click is a little different in
//! pitch, loudness and timing, and bigger cards sound lower and louder, so
//! a whole board spinning sounds like a real one.
//!
//! A bulb that blows pops: a sharp crack with a thump, then the tinkle of
//! its glass landing a moment later.
//!
//! The alarm's chime is the airport kind: two soft bell notes, high then
//! low, "bing-bong", made the same way from a few sine partials.
//!
//! [`Clatter`] is plain Rust and allocates nothing once made; the audio
//! callback runs it, fed through a lock-free queue.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample, StreamConfig};

use crate::rng::Rng;

/// One flap falling.
#[derive(Clone, Copy, Debug)]
pub struct Click {
    /// Where the card is, from -1 (left of the board) to 1 (right).
    pub pan: f32,
    /// The card's height in board units: bigger cards sound bigger.
    pub size: f32,
    /// The last flip, onto the symbol the card was after.
    pub land: bool,
}

/// What the board asks the sound to play.
#[derive(Clone, Copy, Debug)]
pub enum Event {
    Click(Click),
    /// The alarm chime, at its own volume from 0 to 1.
    Chime(f32),
    /// A bulb blowing, from -1 (left of the board) to 1 (right), at its
    /// own volume from 0 to 1.
    Pop(f32, f32),
    /// Many flaps falling at once, more than there are voices for: a
    /// blended clatter as loud as that many would be.
    Crowd(Crowd),
}

/// Many flaps falling in the same moment.
#[derive(Clone, Copy, Debug)]
pub struct Crowd {
    /// How many fell.
    pub count: u32,
    /// Where they are on average, from -1 (left) to 1 (right), and how far
    /// apart: 0 for all in one place, 1 for right across the board.
    pub pan: f32,
    pub spread: f32,
    /// The cards' height in board units.
    pub size: f32,
}

const VOICES: usize = 64;
const TONES: usize = 24;
/// The chime's two notes, E5 then C5, and the time between them.
const CHIME: [f32; 2] = [659.25, 523.25];
const CHIME_GAP: f32 = 0.5;
/// Each note's partials, as multiples of its pitch, and their levels.
const PARTIALS: [(f32, f32); 3] = [(1.0, 1.0), (2.0, 0.18), (3.0, 0.06)];
/// A piece of glass rings at a pitch and two higher partials that aren't
/// harmonics of it, which is what makes it sound like glass.
const GLASS: [(f32, f32); 3] = [(1.0, 1.0), (2.32, 0.45), (4.25, 0.2)];
/// How long a click lasts, in seconds.
const LIFE: f32 = 0.06;

#[derive(Clone, Copy, Default)]
struct Voice {
    live: bool,
    /// Samples to wait before starting, so clicks from one frame spread out.
    wait: u32,
    age: u32,
    left: f32,
    right: f32,
    strike: f32,
    strike_decay: f32,
    ring: Resonator,
    body: Resonator,
    knock: f32,
    knock_phase: f32,
    knock_step: f32,
    knock_decay: f32,
    /// At its own volume, rather than the clatter's.
    own: bool,
}

/// One bell note of the chime.
#[derive(Clone, Copy, Default)]
struct Tone {
    live: bool,
    wait: u32,
    age: u32,
    life: u32,
    attack: u32,
    level: f32,
    env: f32,
    decay: f32,
    steps: [f32; 3],
    phases: [f32; 3],
    /// Each partial's level.
    amps: [f32; 3],
    /// A slow, gentle wobble in level, as a struck bar has.
    wobble: f32,
    wobble_step: f32,
}

impl Tone {
    fn sample(&mut self) -> f32 {
        if self.wait > 0 {
            self.wait -= 1;
            return 0.0;
        }
        let rise = (self.age as f32 / self.attack.max(1) as f32).min(1.0);
        let mut s = 0.0;
        for k in 0..3 {
            s += self.amps[k] * self.phases[k].sin();
            self.phases[k] = (self.phases[k] + self.steps[k]) % std::f32::consts::TAU;
        }
        let wobble = 1.0 + 0.08 * self.wobble.sin();
        self.wobble += self.wobble_step;
        let out = s * self.level * self.env * rise * wobble;
        self.env *= self.decay;
        self.age += 1;
        if self.age > self.life {
            self.live = false;
        }
        out
    }
}

/// A two-pole resonator, about unity gain at its centre.
#[derive(Clone, Copy, Default)]
struct Resonator {
    a1: f32,
    a2: f32,
    gain: f32,
    y1: f32,
    y2: f32,
}

impl Resonator {
    fn new(freq: f32, width: f32, rate: f32) -> Self {
        let r = (-std::f32::consts::PI * width / rate).exp();
        let w = std::f32::consts::TAU * freq / rate;
        Self {
            a1: 2.0 * r * w.cos(),
            a2: -r * r,
            gain: 1.0 - r,
            y1: 0.0,
            y2: 0.0,
        }
    }

    fn run(&mut self, x: f32) -> f32 {
        let y = self.gain * x + self.a1 * self.y1 + self.a2 * self.y2;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

pub struct Clatter {
    rate: f32,
    voices: [Voice; VOICES],
    tones: [Tone; TONES],
    rng: Rng,
}

impl Clatter {
    pub fn new(rate: f32, rng: Rng) -> Self {
        Self {
            rate,
            voices: [Voice::default(); VOICES],
            tones: [Tone::default(); TONES],
            rng,
        }
    }

    fn rand(&mut self, lo: f32, hi: f32) -> f32 {
        self.rng.between(lo as f64, hi as f64) as f32
    }

    pub fn trigger(&mut self, c: Click) {
        let v = self.click_voice(c);
        self.take_voice(v);
    }

    /// The voice of one click.
    fn click_voice(&mut self, c: Click) -> Voice {
        let rate = self.rate;
        // Bigger flaps ring lower and hit harder.
        let size = (c.size / 32.0).clamp(0.5, 4.0);
        let tone = size.powf(-0.35) * self.rand(0.88, 1.12) * if c.land { 0.85 } else { 1.0 };
        let gain = size.sqrt() * self.rand(0.7, 1.1) * if c.land { 1.5 } else { 1.0 };
        let angle = (c.pan.clamp(-1.0, 1.0) * 0.7 + 1.0) * std::f32::consts::FRAC_PI_4;
        Voice {
            live: true,
            wait: (self.rand(0.0, 0.016) * rate) as u32,
            age: 0,
            left: gain * angle.cos(),
            right: gain * angle.sin(),
            strike: 1.0,
            strike_decay: (-1.0 / (0.0009 * rate)).exp(),
            ring: Resonator::new(4200.0 * tone, 600.0, rate),
            body: Resonator::new(1300.0 * tone, 300.0, rate),
            knock: if c.land { 0.2 } else { 0.0 },
            knock_phase: 0.0,
            knock_step: std::f32::consts::TAU * 240.0 * tone / rate,
            knock_decay: (-1.0 / (0.012 * rate)).exp(),
            own: false,
        }
    }

    pub fn play(&mut self, e: Event) {
        match e {
            Event::Click(c) => self.trigger(c),
            Event::Chime(gain) => self.chime(gain),
            Event::Pop(pan, gain) => self.pop(pan, gain),
            Event::Crowd(c) => self.crowd(c),
        }
    }

    /// A crowd of flaps: a few clicks spread over the moment and across the
    /// board, each standing for several, so it sounds as busy as that many
    /// would without taking every voice.
    pub fn crowd(&mut self, c: Crowd) {
        const STAND_INS: u32 = 8;
        let n = c.count.min(STAND_INS);
        if n == 0 {
            return;
        }
        // Clicks that land at random add up as the square root of how many.
        let each = (c.count as f32 / n as f32).sqrt();
        for _ in 0..n {
            let pan = c.pan + c.spread * self.rand(-1.0, 1.0);
            let land = self.rand(0.0, 1.0) < 0.2;
            let mut v = self.click_voice(Click {
                pan,
                size: c.size,
                land,
            });
            // Louder for the clicks it stands for, and spread a little
            // further in time.
            v.left *= each;
            v.right *= each;
            v.wait += (self.rand(0.0, 0.03) * self.rate) as u32;
            self.take_voice(v);
        }
    }

    /// A bulb blowing at `pan`: the crack of the glass and a dull thump,
    /// then its pieces tinkling down a moment later.
    pub fn pop(&mut self, pan: f32, gain: f32) {
        let rate = self.rate;
        let level = gain.clamp(0.0, 1.0);
        let angle = (pan.clamp(-1.0, 1.0) * 0.7 + 1.0) * std::f32::consts::FRAC_PI_4;
        let crack = Voice {
            live: true,
            wait: 0,
            age: 0,
            left: 1.6 * level * angle.cos(),
            right: 1.6 * level * angle.sin(),
            strike: 1.0,
            strike_decay: (-1.0 / (0.0025 * rate)).exp(),
            ring: Resonator::new(3100.0, 1400.0, rate),
            body: Resonator::new(850.0, 500.0, rate),
            knock: 0.45,
            knock_phase: 0.0,
            knock_step: std::f32::consts::TAU * 110.0 / rate,
            knock_decay: (-1.0 / (0.02 * rate)).exp(),
            own: true,
        };
        self.take_voice(crack);
        // The glass landing, piece by piece: sharp, brittle ticks, each
        // with a smaller one as it skitters, and the barest ring.
        for k in 0..11 {
            let wait = 0.42 + k as f32 * 0.03 + self.rand(0.0, 0.1);
            let gain = self.rand(0.8, 1.5) * level;
            for (bounce, (after, size)) in [(0.0, 1.0), (self.rand(0.03, 0.08), 0.4)]
                .into_iter()
                .enumerate()
            {
                let g = gain * size;
                let tick = Voice {
                    live: true,
                    wait: ((wait + after) * rate) as u32,
                    age: 0,
                    left: g * angle.cos(),
                    right: g * angle.sin(),
                    strike: 1.0,
                    strike_decay: (-1.0 / (0.00025 * rate)).exp(),
                    ring: Resonator::new(self.rand(7000.0, 11000.0), 420.0, rate),
                    body: Resonator::new(self.rand(3800.0, 5600.0), 500.0, rate),
                    knock: 0.0,
                    knock_phase: 0.0,
                    knock_step: 0.0,
                    knock_decay: 0.0,
                    own: true,
                };
                self.take_voice(tick);
                if bounce > 0 || k % 2 == 1 {
                    continue;
                }
                // Every other piece rings, bright and over in a moment.
                let freq = self.rand(4200.0, 7200.0);
                let ring = Tone {
                    live: true,
                    wait: (wait * rate) as u32,
                    age: 0,
                    life: (0.15 * rate) as u32,
                    attack: (0.0005 * rate) as u32,
                    level: gain * 0.1,
                    env: 1.0,
                    decay: (-1.0 / (self.rand(0.02, 0.045) * rate)).exp(),
                    steps: GLASS.map(|(m, _)| std::f32::consts::TAU * freq * m / rate),
                    phases: [0.0; 3],
                    amps: GLASS.map(|(_, a)| a),
                    wobble: 0.0,
                    wobble_step: 0.0,
                };
                self.take_tone(ring);
            }
        }
    }

    /// Takes a free tone for `t`, or the oldest one.
    fn take_tone(&mut self, t: Tone) {
        let slot = (0..TONES)
            .find(|&i| !self.tones[i].live)
            .unwrap_or_else(|| (0..TONES).max_by_key(|&i| self.tones[i].age).unwrap_or(0));
        self.tones[slot] = t;
    }

    /// Takes a free voice for `v`, or the oldest one.
    fn take_voice(&mut self, v: Voice) {
        let slot = (0..VOICES)
            .find(|&i| !self.voices[i].live)
            .unwrap_or_else(|| (0..VOICES).max_by_key(|&i| self.voices[i].age).unwrap_or(0));
        self.voices[slot] = v;
    }

    /// Starts the two notes of the alarm chime.
    pub fn chime(&mut self, gain: f32) {
        let rate = self.rate;
        for (n, freq) in CHIME.into_iter().enumerate() {
            let tone = Tone {
                live: true,
                wait: (n as f32 * CHIME_GAP * rate) as u32,
                age: 0,
                life: (3.5 * rate) as u32,
                attack: (0.004 * rate) as u32,
                level: 0.8 * gain.clamp(0.0, 1.0),
                env: 1.0,
                decay: (-1.0 / (1.1 * rate)).exp(),
                steps: PARTIALS.map(|(m, _)| std::f32::consts::TAU * freq * m / rate),
                phases: [0.0; 3],
                amps: PARTIALS.map(|(_, a)| a),
                wobble: 0.0,
                wobble_step: std::f32::consts::TAU * 4.5 / rate,
            };
            self.take_tone(tone);
        }
    }

    /// Adds the clicks under way to `out`, interleaved stereo, the clicks at
    /// `volume` and the chime at its own.
    pub fn render(&mut self, out: &mut [f32], volume: f32) {
        let life = (LIFE * self.rate) as u32;
        for frame in out.as_chunks_mut::<2>().0 {
            let (mut l, mut r) = (0.0, 0.0);
            let (mut own_l, mut own_r) = (0.0, 0.0);
            for i in 0..VOICES {
                if !self.voices[i].live {
                    continue;
                }
                if self.voices[i].wait > 0 {
                    self.voices[i].wait -= 1;
                    continue;
                }
                let noise = self.rand(-1.0, 1.0);
                let v = &mut self.voices[i];
                let x = noise * v.strike;
                v.strike *= v.strike_decay;
                let knock = v.knock * v.knock_phase.sin();
                v.knock *= v.knock_decay;
                v.knock_phase += v.knock_step;
                let s = 0.6 * x + 7.0 * v.ring.run(x) + 2.0 * v.body.run(x) + knock;
                if v.own {
                    own_l += s * v.left;
                    own_r += s * v.right;
                } else {
                    l += s * v.left;
                    r += s * v.right;
                }
                v.age += 1;
                if v.age > life {
                    v.live = false;
                }
            }
            let mut bell = 0.0;
            for t in self.tones.iter_mut().filter(|t| t.live) {
                bell += t.sample();
            }
            // A soft limit, so a whole board at once doesn't clip.
            frame[0] += ((l * volume + own_l + bell) * 0.5).tanh();
            frame[1] += ((r * volume + own_r + bell) * 0.5).tanh();
        }
    }
}

/// The sound output: the queue clicks go in by, and the stream that plays
/// them. Dropping it closes the device.
pub struct Sound {
    events: rtrb::Producer<Event>,
    volume: Arc<AtomicU32>,
    _stream: cpal::Stream,
}

impl Sound {
    pub fn open(volume: f32) -> Result<Self, String> {
        let host = cpal::default_host();
        let dev = host
            .default_output_device()
            .ok_or("There's no sound output to play on")?;
        let supported = dev.default_output_config().map_err(|e| e.to_string())?;
        let format = supported.sample_format();
        let config: StreamConfig = supported.config();
        let (tx, rx) = rtrb::RingBuffer::new(2048);
        let level = Arc::new(AtomicU32::new(gain(volume).to_bits()));
        let stream = match format {
            SampleFormat::F32 => build::<f32>(&dev, &config, rx, level.clone()),
            SampleFormat::I16 => build::<i16>(&dev, &config, rx, level.clone()),
            SampleFormat::I32 => build::<i32>(&dev, &config, rx, level.clone()),
            SampleFormat::U16 => build::<u16>(&dev, &config, rx, level.clone()),
            other => Err(format!(
                "The sound output wants {other} samples, which can't be made here"
            )),
        }?;
        stream.play().map_err(|e| e.to_string())?;
        Ok(Self {
            events: tx,
            volume: level,
            _stream: stream,
        })
    }

    pub fn click(&mut self, c: Click) {
        // A full queue just drops the click; there are plenty more.
        let _ = self.events.push(Event::Click(c));
    }

    /// A bulb blowing, at `pan` from -1 (left) to 1 (right), at `volume`
    /// from 0 to 1.
    pub fn pop(&mut self, pan: f32, volume: f32) {
        let _ = self.events.push(Event::Pop(pan, gain(volume)));
    }

    /// Many clicks at once; see [`Event::Crowd`].
    pub fn crowd(&mut self, c: Crowd) {
        let _ = self.events.push(Event::Crowd(c));
    }

    /// Rings the alarm chime once, at `volume` from 0 to 1.
    pub fn chime(&mut self, volume: f32) {
        let _ = self.events.push(Event::Chime(gain(volume)));
    }

    pub fn set_volume(&self, v: f32) {
        self.volume.store(gain(v).to_bits(), Ordering::Relaxed);
    }
}

/// The volume slider's 0 to 1 as a gain, closer to how loud it sounds.
fn gain(v: f32) -> f32 {
    let v = v.clamp(0.0, 1.0);
    v * v
}

fn build<T>(
    dev: &cpal::Device,
    config: &StreamConfig,
    mut events: rtrb::Consumer<Event>,
    volume: Arc<AtomicU32>,
) -> Result<cpal::Stream, String>
where
    T: SizedSample + FromSample<f32> + Send + 'static,
{
    let channels = config.channels as usize;
    let mut clatter = Clatter::new(config.sample_rate as f32, Rng::seeded());
    // Made here, not in the callback, which must never allocate.
    let mut scratch = vec![0.0f32; 2 * 1024];
    dev.build_output_stream(
        *config,
        move |data: &mut [T], _| {
            while let Ok(e) = events.pop() {
                clatter.play(e);
            }
            let vol = f32::from_bits(volume.load(Ordering::Relaxed));
            // In pieces the size of the scratch buffer, however much the
            // system asks for at once.
            for chunk in data.chunks_mut(channels * (scratch.len() / 2)) {
                let frames = chunk.len() / channels;
                let buf = &mut scratch[..frames * 2];
                buf.fill(0.0);
                clatter.render(buf, vol);
                for (f, out) in chunk.chunks_exact_mut(channels).enumerate() {
                    let (l, r) = (buf[2 * f], buf[2 * f + 1]);
                    for (c, s) in out.iter_mut().enumerate() {
                        let v = match (channels, c) {
                            (1, _) => 0.5 * (l + r),
                            (_, 0) => l,
                            (_, 1) => r,
                            _ => 0.0,
                        };
                        *s = T::from_sample(v);
                    }
                }
            }
        },
        // Nothing to be done from here if the device goes away.
        |_| {},
        None,
    )
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn click(size: f32, land: bool) -> Click {
        Click {
            pan: 0.0,
            size,
            land,
        }
    }

    #[test]
    fn a_click_rings_and_dies_away() {
        let mut c = Clatter::new(48_000.0, Rng::with_seed(7));
        c.trigger(click(32.0, false));
        let mut out = vec![0.0; 2 * 48_000 / 5];
        c.render(&mut out, 1.0);
        let peak = out.iter().fold(0.0f32, |m, x| m.max(x.abs()));
        assert!(peak > 0.05 && peak < 1.0, "peak {peak}");
        assert!(out.iter().all(|x| x.is_finite()));
        // Silent once every voice is done.
        assert!(out[out.len() - 200..].iter().all(|&x| x == 0.0));
    }

    #[test]
    fn a_whole_board_at_once_stays_in_range() {
        let mut c = Clatter::new(44_100.0, Rng::with_seed(3));
        for i in 0..500 {
            c.trigger(click(if i % 5 == 0 { 100.0 } else { 32.0 }, i % 7 == 0));
        }
        let mut out = vec![0.0; 2 * 4410];
        c.render(&mut out, 1.0);
        assert!(out.iter().all(|x| x.is_finite() && x.abs() <= 1.0));
    }

    #[test]
    fn a_crowd_is_louder_than_one_click_and_stays_in_range() {
        let level = |count: u32| {
            let mut c = Clatter::new(48_000.0, Rng::with_seed(4));
            c.crowd(Crowd {
                count,
                pan: 0.0,
                spread: 0.8,
                size: 32.0,
            });
            let mut out = vec![0.0; 2 * 48_000 / 5];
            c.render(&mut out, 1.0);
            assert!(out.iter().all(|x| x.is_finite() && x.abs() <= 1.0));
            out.iter().map(|x| x * x).sum::<f32>()
        };
        assert!(level(400) > level(8) * 3.0);
        assert!(level(8) > 0.0);
        assert_eq!(level(0), 0.0);
    }

    #[test]
    fn the_chime_rings_two_notes_and_fades() {
        let rate = 48_000.0;
        let mut c = Clatter::new(rate, Rng::with_seed(1));
        c.chime(0.64);
        let mut out = vec![0.0; 2 * 48_000 * 5];
        c.render(&mut out, 1.0);
        let peak = |a: f32, b: f32| {
            out[(a * rate) as usize * 2..(b * rate) as usize * 2]
                .iter()
                .fold(0.0f32, |m, x| m.max(x.abs()))
        };
        // The second note starts half a second in, louder than the first
        // has become by then; then it all dies away.
        assert!(peak(0.0, 0.1) > 0.1);
        assert!(peak(0.5, 0.6) > peak(0.4, 0.5));
        assert!(peak(4.5, 5.0) == 0.0);
        assert!(out.iter().all(|x| x.is_finite() && x.abs() <= 1.0));
    }

    #[test]
    fn a_bulb_pops_then_its_glass_tinkles() {
        let rate = 48_000.0;
        let mut c = Clatter::new(rate, Rng::with_seed(9));
        c.pop(0.5, 0.5);
        let mut out = vec![0.0; 2 * 2 * 48_000];
        c.render(&mut out, 1.0);
        let peak = |a: f32, b: f32| {
            out[(a * rate) as usize * 2..(b * rate) as usize * 2]
                .iter()
                .fold(0.0f32, |m, x| m.max(x.abs()))
        };
        // The crack, a quiet moment, the glass landing in a brittle
        // scatter, then silence.
        assert!(peak(0.0, 0.05) > 0.2);
        assert!(peak(0.2, 0.4) < 0.01);
        assert!(peak(0.4, 0.75) > 0.02);
        assert!(peak(1.2, 2.0) == 0.0);
        assert!(out.iter().all(|x| x.is_finite() && x.abs() <= 1.0));
        // To the right of the board, louder on the right.
        let (l, r) = out.chunks(2).fold((0.0f32, 0.0f32), |(l, r), f| {
            (l + f[0].abs(), r + f[1].abs())
        });
        assert!(r > l);
    }

    /// Renders a bulb blowing to the WAV file named by `POP_WAV`.
    #[test]
    #[ignore]
    fn render_a_pop() {
        let path = std::env::var("POP_WAV").expect("set POP_WAV to a path");
        let mut c = Clatter::new(48_000.0, Rng::with_seed(9));
        c.pop(0.0, 0.5);
        let mut out = vec![0.0; 2 * 2 * 48_000];
        c.render(&mut out, 0.6);
        write_wav(&path, &out);
    }

    /// A board like the default one starting up, with the chime rung at
    /// each of `chimes` seconds, as interleaved stereo at 48 kHz.
    fn render_board(seconds: f64, chimes: &[f64]) -> Vec<f32> {
        use crate::flap::{Drum, Flap};
        let rate = 48_000u32;
        let mut rng = Rng::with_seed(11);
        let mut clatter = Clatter::new(rate as f32, Rng::with_seed(5));
        // Four big digits, the date and three zones of small cards.
        let mut cards: Vec<(Flap, String, f32, f32)> = Vec::new();
        for (k, d) in ["1", "0", "4", "2"].iter().enumerate() {
            let (delay, pace) = rng.scatter();
            cards.push((
                Flap::new(Drum::Digit, 0.0, delay, pace),
                d.to_string(),
                100.0,
                k as f32 / 2.0 - 0.75,
            ));
        }
        let text = "TUE 27 OCT 2026LONDON      TOKYO       NEW YORK    ";
        for (k, ch) in text.chars().enumerate() {
            let (delay, pace) = rng.scatter();
            let pan = (k % 15) as f32 / 7.0 - 1.0;
            cards.push((
                Flap::new(Drum::Alpha, 0.0, delay, pace),
                ch.to_string(),
                32.0,
                pan,
            ));
        }
        let mut out = Vec::new();
        let frame = 1.0 / 60.0;
        let per_frame = (rate as f64 * frame) as usize;
        let mut rung = 0;
        for n in 0..(seconds / frame) as usize {
            let t = n as f64 * frame;
            while rung < chimes.len() && chimes[rung] <= t {
                clatter.chime(gain(0.8));
                rung += 1;
            }
            for (flap, want, size, pan) in &mut cards {
                flap.set(want);
                if let Some(fall) = flap.tick(t, 1.0) {
                    clatter.trigger(Click {
                        pan: *pan,
                        size: *size,
                        land: fall.land,
                    });
                }
            }
            let mut buf = vec![0.0; per_frame * 2];
            clatter.render(&mut buf, 0.6);
            out.extend(buf);
        }
        out
    }

    fn write_wav(path: &str, out: &[f32]) {
        let rate = 48_000u32;
        let mut wav = Vec::new();
        let data_len = (out.len() * 2) as u32;
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + data_len).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&[1, 0, 2, 0]);
        wav.extend_from_slice(&rate.to_le_bytes());
        wav.extend_from_slice(&(rate * 4).to_le_bytes());
        wav.extend_from_slice(&[4, 0, 16, 0]);
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data_len.to_le_bytes());
        for s in out {
            wav.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
        }
        std::fs::write(path, wav).unwrap();
    }

    /// Renders a start-up of a board like the default one to the WAV file
    /// named by `CLATTER_WAV`, to hear what it sounds like.
    #[test]
    #[ignore]
    fn render_a_start_up() {
        let path = std::env::var("CLATTER_WAV").expect("set CLATTER_WAV to a path");
        write_wav(&path, &render_board(6.0, &[]));
    }

    /// Renders an alarm going off, the board's clatter and three chimes, to
    /// the WAV file named by `ALARM_WAV`.
    #[test]
    #[ignore]
    fn render_an_alarm() {
        let path = std::env::var("ALARM_WAV").expect("set ALARM_WAV to a path");
        write_wav(&path, &render_board(12.5, &[0.3, 4.3, 8.3]));
    }
}
