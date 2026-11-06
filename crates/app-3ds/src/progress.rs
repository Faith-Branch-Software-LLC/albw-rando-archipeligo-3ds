//! The screens shown while making a seed. The worker reports through atomics, and the screen thread
//! reads them.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering::Relaxed};
use std::sync::Mutex;

use crate::draw::{self, CENTER, LEFT, RIGHT};
use crate::memory;

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
        draw::frame(
            || this.draw_top(overall, eta, elapsed, finished, failure.as_deref()),
            || this.draw_bottom(finished, failure.is_some()),
        );
    }

    fn draw_top(&self, overall: f32, eta: f32, elapsed: f32, finished: bool, failure: Option<&str>) {
        let accent = match (failure.is_some(), finished) {
            (true, _) => draw::RED,
            (false, true) => draw::GREEN,
            _ => draw::GOLD,
        };
        draw::triforce(200.0, 12.0, 20.0, accent);
        draw::text("A Link Between Worlds", 200.0, 50.0, 0.75, draw::WHITE, CENTER);

        let current = STEP.load(Relaxed) as usize;
        let status = match (failure, finished) {
            (Some(_), _) => "Could not finish",
            (None, true) => "Your seed is ready",
            _ => STEPS[current].label,
        };
        draw::text(status, 24.0, 88.0, 0.58, draw::WHITE, LEFT);
        draw::text(&format!("{:.0}%", overall * 100.0), 376.0, 88.0, 0.58, accent, RIGHT);
        draw::bar(24.0, 112.0, 352.0, 16.0, overall, accent, self.tick, !finished);

        match (failure, finished) {
            (Some(message), _) => {
                for (index, line) in ui_core::menu::wrap(message, 56).iter().take(3).enumerate() {
                    draw::text(line, 24.0, 136.0 + index as f32 * 17.0, 0.44, draw::RED, LEFT);
                }
            }
            (None, true) => {
                for (index, line) in RESULT.lock().unwrap().iter().take(3).enumerate() {
                    draw::text(line, 24.0, 136.0 + index as f32 * 17.0, 0.44, draw::GREEN, LEFT);
                }
            }
            _ => draw::text(&eta_text(eta), 24.0, 136.0, 0.5, draw::GOLD, LEFT),
        }
        draw::text(&clock(elapsed), 376.0, 136.0, 0.5, draw::MUTED, RIGHT);

        if finished {
            draw::text("A Back to Menu    START Exit", 200.0, 212.0, 0.5, draw::WHITE, CENTER);
        } else {
            let model = if self.is_new { "New 3DS" } else { "Old 3DS" };
            draw::text(&format!("{model}  -  Please keep the console powered"), 200.0, 214.0, 0.42, draw::MUTED, CENTER);
        }
    }

    fn draw_bottom(&self, finished: bool, failed: bool) {
        let current = STEP.load(Relaxed) as usize;
        let row_height = 36.0;
        let gap = 4.0;
        for (index, step) in STEPS.iter().enumerate() {
            let y = 8.0 + index as f32 * (row_height + gap);
            let complete = index < current || (finished && !failed);
            let active = index == current && !complete;
            draw::rect(10.0, y, 300.0, row_height, if active { draw::CARD_ACTIVE } else { draw::CARD });

            let dot = y + 18.0;
            if complete {
                draw::circle(28.0, dot, 7.0, draw::GREEN);
            } else if active && failed {
                draw::circle(28.0, dot, 7.0, draw::RED);
            } else if active {
                let pulse = ((self.tick as f32 * 0.15).sin() + 1.0) * 1.2;
                draw::circle(28.0, dot, 5.0 + pulse, draw::GOLD);
            } else {
                draw::circle(28.0, dot, 6.0, draw::DIM);
                draw::circle(28.0, dot, 4.0, draw::CARD);
            }

            let color = if complete || active { draw::WHITE } else { draw::MUTED };
            draw::text(step.label, 44.0, if active { y + 3.0 } else { y + 9.0 }, 0.5, color, LEFT);
            if active && !failed {
                draw::bar(44.0, y + 23.0, 256.0, 7.0, self.best[index], draw::GOLD, self.tick, true);
            }
            if complete {
                let seconds = STEP_ENDS[index].load(Relaxed).wrapping_sub(STEP_STARTS[index].load(Relaxed)) as f32 / 1000.0;
                draw::text(&clock(seconds), 300.0, y + 10.0, 0.45, draw::GREEN, RIGHT);
            }
        }
        draw::text(&format!("Memory {:.1} of {:.1} MB", memory::usage_mb(), self.heap_mb), 160.0, 214.0, 0.42, draw::MUTED, CENTER);
    }
}
