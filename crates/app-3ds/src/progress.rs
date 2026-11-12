//! The screens shown while making a seed. The worker reports through atomics, and the screen thread
//! reads them.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering::Relaxed};
use std::sync::Mutex;

use ui_core::menu::{ROW_GAP, ROW_HEIGHT, ROW_LEFT, ROW_TOP, ROW_WIDTH};

use crate::art::Skin;
use crate::draw::{self, Font, CENTER, LEFT, RIGHT};
use crate::memory;
use crate::menu_screen::{self, Sizes};

/// One visible step, with its expected seconds on a New 3DS, from measured runs.
struct Step {
    label: &'static str,
    new_3ds_seconds: f32,
}

/// Phase names reported by the job itself.
pub mod job_phase {
    pub const ADDING_MODS: &str = "Adding models and voices";
}

const STEPS: [Step; 5] = [
    Step { label: "Placing items", new_3ds_seconds: 62.0 },
    Step { label: "Writing hints", new_3ds_seconds: 51.0 },
    Step { label: "Updating the world", new_3ds_seconds: 88.0 },
    Step { label: "Updating items and text", new_3ds_seconds: 26.0 },
    Step { label: "Compressing and saving", new_3ds_seconds: 204.0 },
];
/// Rough speed ratio from measured logs. The ETA corrects itself as real timings arrive.
const OLD_3DS_SLOWDOWN: f32 = 3.6;

fn step_of(phase: &str) -> Option<usize> {
    Some(match phase {
        "Checking reachability" | "Preplacing items" | "Placing items" | "Placing junk items" | "Calculating metrics" => 0,
        "Generating hints" => 1,
        "Patching regions" => 2,
        "Patching actors" | "Patching event flow" | "Patching messages" | "Patching dungeon prizes"
        | "Patching item tables" | "Patching stages" | "Building cutscenes" | "Patching item models"
        | "Patching code" => 3,
        "Compressing files" | "Writing files" | job_phase::ADDING_MODS => 4,
        _ => return None,
    })
}

static STEP: AtomicU32 = AtomicU32::new(0);
static DONE: AtomicU32 = AtomicU32::new(0);
static TOTAL: AtomicU32 = AtomicU32::new(0);
static STEP_STARTS: [AtomicU32; 5] = [const { AtomicU32::new(0) }; 5];
static STEP_ENDS: [AtomicU32; 5] = [const { AtomicU32::new(0) }; 5];
static FINISHED: AtomicBool = AtomicBool::new(false);
static FAILURE: Mutex<Option<String>> = Mutex::new(None);
static RESULT: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn now_ms() -> u32 {
    // SAFETY: Reads the system clock only. Wrapping 32-bit milliseconds cover about 49 days.
    unsafe { ctru_sys::osGetTime() as u32 }
}

/// Clears the last run's state. Call before starting the worker.
pub fn reset() {
    STEP.store(0, Relaxed);
    DONE.store(0, Relaxed);
    TOTAL.store(0, Relaxed);
    STEP_STARTS[0].store(now_ms(), Relaxed);
    FINISHED.store(false, Relaxed);
    *FAILURE.lock().unwrap() = None;
    RESULT.lock().unwrap().clear();
}

/// Called on the worker thread for each progress report.
pub fn report(phase: &str, done: u32, total: u32) {
    let Some(step) = step_of(phase) else { return };
    let current = STEP.load(Relaxed) as usize;
    if step != current {
        let now = now_ms();
        STEP_ENDS[current].store(now, Relaxed);
        STEP_STARTS[step].store(now, Relaxed);
        STEP.store(step as u32, Relaxed);
    }
    TOTAL.store(total, Relaxed);
    DONE.store(done, Relaxed);
}

/// Records why the run stopped. The first reason wins, because later ones are usually side effects.
pub fn fail(message: &str) {
    let mut failure = FAILURE.lock().unwrap();
    if failure.is_none() {
        *failure = Some(message.to_owned());
    }
}

/// Sets the lines shown when the seed is ready.
pub fn set_result(lines: Vec<String>) {
    *RESULT.lock().unwrap() = lines;
}

pub fn finish() {
    let step = STEP.load(Relaxed) as usize;
    STEP_ENDS[step].store(now_ms(), Relaxed);
    FINISHED.store(true, Relaxed);
}

pub fn finished() -> bool {
    FINISHED.load(Relaxed)
}

fn clock(seconds: f32) -> String {
    let seconds = seconds.max(0.0) as u32;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

fn eta_text(seconds: f32) -> String {
    if seconds >= 90.0 {
        format!("About {} min left", (seconds / 60.0).round() as u32)
    } else if seconds >= 10.0 {
        format!("About {} s left", ((seconds / 5.0).round() as u32) * 5)
    } else {
        "Almost done".to_owned()
    }
}

pub struct Screen {
    is_new: bool,
    heap_mb: f32,
    started: u32,
    tick: u32,
    eta: Option<f32>,
    /// Keeps each step's bar from moving backward between reports.
    best: [f32; 5],
    final_seconds: Option<f32>,
}

impl Screen {
    pub fn new(is_new: bool, heap_mb: f32) -> Self {
        Self { is_new, heap_mb, started: now_ms(), tick: 0, eta: None, best: [0.0; 5], final_seconds: None }
    }

    fn expected(&self, step: usize) -> f32 {
        STEPS[step].new_3ds_seconds * if self.is_new { 1.0 } else { OLD_3DS_SLOWDOWN }
    }

    /// Returns overall progress and estimated seconds left.
    fn estimate(&mut self) -> (f32, f32) {
        let current = STEP.load(Relaxed) as usize;
        let in_step = now_ms().wrapping_sub(STEP_STARTS[current].load(Relaxed)) as f32 / 1000.0;

        // Compare time spent on finished steps with what they were expected to take.
        let mut expected_done = 0.0;
        let mut actual_done = 0.0;
        for step in 0..current {
            expected_done += self.expected(step);
            actual_done += STEP_ENDS[step].load(Relaxed).wrapping_sub(STEP_STARTS[step].load(Relaxed)) as f32 / 1000.0;
        }
        let speed = if expected_done > 0.0 { (actual_done / expected_done).clamp(0.3, 4.0) } else { 1.0 };

        let total = TOTAL.load(Relaxed);
        let reported = if total > 0 { DONE.load(Relaxed) as f32 / total as f32 } else { 0.0 };
        // Steps that report nothing creep forward by time, and stop short of the end.
        let timed = (in_step / (self.expected(current) * speed)).min(0.95);
        let within = if reported > 0.0 { reported } else { timed };
        self.best[current] = self.best[current].max(within.min(1.0));
        let within = self.best[current];

        let everything: f32 = (0..STEPS.len()).map(|step| self.expected(step)).sum();
        let later: f32 = (current + 1..STEPS.len()).map(|step| self.expected(step)).sum();
        let here = self.expected(current);
        let overall = (expected_done + here * within) / everything;

        // A reporting step can also estimate from its own pace.
        let here_left = if within > 0.02 && reported > 0.0 {
            in_step * (1.0 - within) / within
        } else {
            here * speed * (1.0 - within)
        };
        (overall, here_left + later * speed)
    }

    pub fn draw(&mut self) {
        self.tick = self.tick.wrapping_add(1);
        let finished = finished();
        let failure = FAILURE.lock().unwrap().clone();
        let (overall, raw_eta) = if finished { (1.0, 0.0) } else { self.estimate() };
        let eta = match self.eta {
            Some(old) => old + (raw_eta - old) * 0.03,
            None => raw_eta,
        };
        self.eta = Some(eta);
        let elapsed = now_ms().wrapping_sub(self.started) as f32 / 1000.0;
        let elapsed = if finished { *self.final_seconds.get_or_insert(elapsed) } else { elapsed };

        let this = &*self;
        let state = State { overall, eta, elapsed, finished, failure: failure.as_deref() };
        draw::frame(|| this.draw_top(&state), || this.draw_bottom(finished, failure.is_some()));
    }

    fn status(state: &State) -> &'static str {
        match (state.failure, state.finished) {
            (Some(_), _) => "Could not finish",
            (None, true) => "Your seed is ready",
            _ => STEPS[STEP.load(Relaxed) as usize].label,
        }
    }

    /// Up to three lines under the bar: why it stopped, or what was made.
    fn result_lines(state: &State) -> Vec<(String, u32)> {
        match (state.failure, state.finished) {
            (Some(message), _) => ui_core::menu::wrap(message, 52).into_iter().take(3).map(|line| (line, draw::RED)).collect(),
            (None, true) => RESULT.lock().unwrap().iter().take(3).map(|line| (line.clone(), draw::GREEN)).collect(),
            _ => Vec::new(),
        }
    }

    /// The menu's colors and fonts: a night sky, the gold title, and a plum box holding the progress.
    fn draw_top(&self, state: &State) {
        let sizes = Sizes::new();
        let accent = state.accent();
        draw::gradient(0.0, 0.0, 400.0, 240.0, draw::FOREST, draw::DUSK);
        draw::triforce(200.0, 10.0, 16.0, draw::GOLD);
        draw::text_fit(Font::Title, "A Randomizer Between Worlds", 200.0, 40.0, sizes.title, 370.0, draw::GOLD, CENTER);

        let (left, right, top, bottom) = (24.0, 376.0, 82.0, 230.0);
        draw::framed(left, top, right - left, bottom - top, 2.0, draw::PLUM, draw::PLUM_EDGE);
        let (inner_left, inner_right) = (left + 14.0, right - 14.0);

        let percent = format!("{:.0} %", state.overall * 100.0);
        let percent_width = draw::text_width(Font::Title, &percent, sizes.title);
        let title_top = top + 6.0;
        let status_room = inner_right - inner_left - percent_width - 8.0;
        draw::text_fit(Font::Title, Self::status(state), inner_left, title_top, sizes.title, status_room, draw::GOLD, LEFT);
        draw::text_in(Font::Title, &percent, inner_right, title_top, sizes.title, accent, RIGHT);

        let bar_top = title_top + draw::line_height(sizes.title) + 4.0;
        draw::bar(inner_left, bar_top, inner_right - inner_left, 14.0, state.overall, accent, self.tick, !state.finished);

        let line = draw::line_height(sizes.text);
        let info_top = bar_top + 22.0;
        if !state.finished {
            draw::text(&eta_text(state.eta), inner_left, info_top, sizes.text, draw::WHITE, LEFT);
        }
        draw::text(&clock(state.elapsed), inner_right, info_top, sizes.text, draw::MUTED, RIGHT);

        let lines = Self::result_lines(state);
        if lines.is_empty() {
            let model = if self.is_new { "New 3DS" } else { "Old 3DS" };
            let note = format!("{model}: please keep the console powered.");
            draw::text_fit(Font::Body, &note, inner_left, bottom - 8.0 - line, sizes.text * 0.85, inner_right - inner_left, draw::MUTED, LEFT);
        }
        // A finished run has no time left to show, so its lines start beside the timer.
        let start = if state.finished { info_top } else { info_top + line };
        let step = ((bottom - 6.0 - start) / lines.len().max(1) as f32).min(line);
        for (index, (text, color)) in lines.iter().enumerate() {
            let width = if index == 0 && state.finished { inner_right - inner_left - 50.0 } else { inner_right - inner_left };
            draw::text_fit(Font::Body, text, inner_left, start + index as f32 * step, sizes.text * 0.85, width, *color, LEFT);
        }
    }

    /// The steps, drawn like the menu's rows: navy with gold edges, the current one glowing.
    fn draw_bottom(&self, finished: bool, failed: bool) {
        let sizes = Sizes::new();
        draw::gradient(0.0, 0.0, 320.0, 240.0, draw::DUSK, draw::VIOLET);
        let current = STEP.load(Relaxed) as usize;
        // Draws the current row last, so its glow lies over its neighbors.
        let order = (0..STEPS.len()).filter(|&index| index != current).chain([current]);
        for index in order {
            let step = &STEPS[index];
            let y = ROW_TOP + index as f32 * (ROW_HEIGHT + ROW_GAP);
            let complete = index < current || (finished && !failed);
            let active = index == current && !complete;
            if active {
                draw::glow(ROW_LEFT, y, ROW_WIDTH, ROW_HEIGHT, draw::GOLD);
            }
            let (fill, edge) = if active { (draw::NAVY_LIGHT, draw::GOLD) } else { (draw::NAVY, draw::GOLD_EDGE) };
            draw::framed(ROW_LEFT, y, ROW_WIDTH, ROW_HEIGHT, 2.0, fill, edge);

            let dot_x = ROW_LEFT + 18.0;
            let dot = y + ROW_HEIGHT / 2.0;
            if complete {
                draw::circle(dot_x, dot, 7.0, draw::GREEN);
            } else if active && failed {
                draw::circle(dot_x, dot, 7.0, draw::RED);
            } else if active {
                let pulse = ((self.tick as f32 * 0.15).sin() + 1.0) * 1.2;
                draw::circle(dot_x, dot, 5.0 + pulse, draw::GOLD);
            } else {
                draw::circle(dot_x, dot, 6.0, draw::GOLD_EDGE);
                draw::circle(dot_x, dot, 4.0, draw::NAVY_DEEP);
            }

            let label_left = ROW_LEFT + 34.0;
            let color = if complete || active { draw::WHITE } else { draw::MUTED };
            let scale = sizes.text * if active { 0.9 } else { 1.0 };
            let label_top = if active { y + 3.0 } else { y + (ROW_HEIGHT - draw::line_height(scale)) / 2.0 };
            draw::text(step.label, label_left, label_top, scale, color, LEFT);
            if active && !failed {
                draw::bar(label_left, y + ROW_HEIGHT - 11.0, ROW_LEFT + ROW_WIDTH - 12.0 - label_left, 7.0, self.best[index], draw::GOLD, self.tick, true);
            }
            if complete {
                let seconds = STEP_ENDS[index].load(Relaxed).wrapping_sub(STEP_STARTS[index].load(Relaxed)) as f32 / 1000.0;
                let top = y + (ROW_HEIGHT - draw::line_height(sizes.text * 0.9)) / 2.0;
                draw::text(&clock(seconds), ROW_LEFT + ROW_WIDTH - 12.0, top, sizes.text * 0.9, draw::GREEN, RIGHT);
            }
        }
        let hint = if finished {
            "A Back to Menu   START Exit".to_owned()
        } else {
            format!("Memory {:.1} of {:.1} MB", memory::usage_mb(), self.heap_mb)
        };
        menu_screen::draw_hint(&hint, &Skin::default(), sizes);
    }
}

/// What the top screen shows this frame.
struct State<'a> {
    overall: f32,
    eta: f32,
    elapsed: f32,
    finished: bool,
    failure: Option<&'a str>,
}

impl State<'_> {
    fn accent(&self) -> u32 {
        match (self.failure.is_some(), self.finished) {
            (true, _) => draw::RED,
            (false, true) => draw::GREEN,
            _ => draw::GOLD,
        }
    }
}
