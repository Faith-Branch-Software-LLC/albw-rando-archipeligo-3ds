//! The progress screens. The worker reports through atomics, and the screen thread reads them.
//!
//! Placeholder art: every shape is drawn in code until real art exists.

use std::ffi::CString;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering::Relaxed};
use std::sync::Mutex;

use citro2d_sys as c2d;

const fn rgba(r: u8, g: u8, b: u8, a: u8) -> u32 {
    ((a as u32) << 24) | ((b as u32) << 16) | ((g as u32) << 8) | r as u32
}

const SKY_TOP: u32 = rgba(0x10, 0x18, 0x2C, 255);
const SKY_BOTTOM: u32 = rgba(0x1C, 0x2C, 0x4A, 255);
const CARD: u32 = rgba(0x24, 0x31, 0x4D, 255);
const CARD_ACTIVE: u32 = rgba(0x2F, 0x41, 0x66, 255);
const TRACK: u32 = rgba(0x0C, 0x12, 0x22, 255);
const GOLD: u32 = rgba(0xF2, 0xC6, 0x4B, 255);
const GREEN: u32 = rgba(0x4F, 0xC2, 0x7A, 255);
const RED: u32 = rgba(0xE0, 0x5A, 0x5A, 255);
const WHITE: u32 = rgba(0xF4, 0xF6, 0xFB, 255);
const MUTED: u32 = rgba(0x98, 0xA6, 0xC2, 255);
const DIM: u32 = rgba(0x4E, 0x5C, 0x7A, 255);
const SHINE: u32 = rgba(0xFF, 0xFF, 0xFF, 70);

const LEFT: i32 = 0;
const CENTER: i32 = 1;
const RIGHT: i32 = 2;

/// One visible step, with its expected seconds on a New 3DS.
struct Step {
    label: &'static str,
    new_3ds_seconds: f32,
}

const STEPS: [Step; 5] = [
    Step { label: "Placing items", new_3ds_seconds: 57.0 },
    Step { label: "Writing hints", new_3ds_seconds: 40.0 },
    Step { label: "Updating the world", new_3ds_seconds: 100.0 },
    Step { label: "Updating items and text", new_3ds_seconds: 27.0 },
    Step { label: "Compressing and saving", new_3ds_seconds: 180.0 },
];
/// Rough speed ratio from measured logs. The ETA corrects itself as real timings arrive.
const OLD_3DS_SLOWDOWN: f32 = 3.2;

fn step_of(phase: &str) -> Option<usize> {
    Some(match phase {
        "Checking reachability" | "Preplacing items" | "Placing items" | "Placing junk items" | "Calculating metrics" => 0,
        "Generating hints" => 1,
        "Patching regions" => 2,
        "Patching actors" | "Patching event flow" | "Patching messages" | "Patching dungeon prizes"
        | "Patching item tables" | "Patching stages" | "Building cutscenes" | "Patching item models"
        | "Patching code" => 3,
        "Compressing files" | "Writing files" => 4,
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
static SAVED_TO: Mutex<Option<String>> = Mutex::new(None);

fn now_ms() -> u32 {
    // SAFETY: Reads the system clock only. Wrapping 32-bit milliseconds cover about 49 days.
    unsafe { ctru_sys::osGetTime() as u32 }
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

/// Records where the patch ended up, if that is not the default folder.
pub fn saved_to(path: String) {
    *SAVED_TO.lock().unwrap() = Some(path);
}

pub fn finish() {
    let step = STEP.load(Relaxed) as usize;
    STEP_ENDS[step].store(now_ms(), Relaxed);
    FINISHED.store(true, Relaxed);
}

pub fn finished() -> bool {
    FINISHED.load(Relaxed)
}

fn text(content: &str, x: f32, y: f32, scale: f32, color: u32, align: i32) {
    let Ok(content) = CString::new(content) else { return };
    // SAFETY: citro2d is running, a scene has begun, and the pointer is live for the call.
    unsafe { c2d::shim_text_aligned(content.as_ptr(), x, y, scale, color, align) };
}

fn rect(x: f32, y: f32, w: f32, h: f32, color: u32) {
    // SAFETY: citro2d is running and a scene has begun.
    unsafe { c2d::shim_rect(x, y, w, h, color) };
}

fn circle(x: f32, y: f32, radius: f32, color: u32) {
    // SAFETY: citro2d is running and a scene has begun.
    unsafe { c2d::shim_circle(x, y, radius, color) };
}

fn pill(x: f32, y: f32, w: f32, h: f32, color: u32) {
    let radius = h / 2.0;
    circle(x + radius, y + radius, radius, color);
    circle(x + w - radius, y + radius, radius, color);
    rect(x + radius, y, (w - h).max(0.0), h, color);
}

/// Draws a rounded bar. A band of light travels along the filled part while work is running.
fn bar(x: f32, y: f32, w: f32, h: f32, fraction: f32, color: u32, tick: u32, animate: bool) {
    pill(x, y, w, h, TRACK);
    let fill = (w * fraction.clamp(0.0, 1.0)).max(h);
    pill(x, y, fill, h, color);
    let inner = fill - h;
    if animate && inner > 4.0 {
        let band = 36.0;
        let travel = inner + band;
        let head = (tick as f32 * 2.5) % travel;
        let start = (head - band).max(0.0);
        let end = head.min(inner);
        rect(x + h / 2.0 + start, y, end - start, h, SHINE);
    }
}

fn triforce(center_x: f32, top: f32, side: f32, color: u32) {
    let height = side * 0.866;
    let piece = |x: f32, y: f32| {
        // SAFETY: citro2d is running and a scene has begun.
        unsafe { c2d::shim_triangle(x, y, x - side / 2.0, y + height, x + side / 2.0, y + height, color) };
    };
    piece(center_x, top);
    piece(center_x - side / 2.0, top + height);
    piece(center_x + side / 2.0, top + height);
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
    usage_mb: fn() -> f32,
    started: u32,
    tick: u32,
    eta: Option<f32>,
    /// Keeps each step's bar from moving backward between reports.
    best: [f32; 5],
    final_seconds: Option<f32>,
}

impl Screen {
    pub fn new(is_new: bool, heap_mb: f32, usage_mb: fn() -> f32) -> Self {
        // SAFETY: Gfx is alive, and this is the only citro2d user.
        assert!(unsafe { c2d::shim_init() }, "failed to start citro2d");
        let now = now_ms();
        STEP_STARTS[0].store(now, Relaxed);
        Self { is_new, heap_mb, usage_mb, started: now, tick: 0, eta: None, best: [0.0; 5], final_seconds: None }
    }

    fn expected(&self, step: usize) -> f32 {
        STEPS[step].new_3ds_seconds * if self.is_new { 1.0 } else { OLD_3DS_SLOWDOWN }
    }

    /// Returns overall progress and estimated seconds left.
    fn estimate(&mut self) -> (f32, f32) {
        let current = STEP.load(Relaxed) as usize;
        let now = now_ms();
        let in_step = now.wrapping_sub(STEP_STARTS[current].load(Relaxed)) as f32 / 1000.0;

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

        // SAFETY: citro2d is running, and this thread is its only user.
        unsafe {
            c2d::shim_frame_begin();
            c2d::shim_screen_begin(0, SKY_TOP);
            c2d::shim_rect_gradient(0.0, 0.0, 400.0, 240.0, SKY_TOP, SKY_BOTTOM);
        }
        self.draw_top(overall, eta, elapsed, finished, failure.as_deref());
        // SAFETY: As above.
        unsafe {
            c2d::shim_screen_begin(1, SKY_TOP);
            c2d::shim_rect_gradient(0.0, 0.0, 320.0, 240.0, SKY_BOTTOM, SKY_TOP);
        }
        self.draw_bottom(finished, failure.is_some());
        // SAFETY: As above.
        unsafe { c2d::shim_frame_end() };
    }

    fn draw_top(&self, overall: f32, eta: f32, elapsed: f32, finished: bool, failure: Option<&str>) {
        let accent = match (failure.is_some(), finished) {
            (true, _) => RED,
            (false, true) => GREEN,
            _ => GOLD,
        };
        triforce(200.0, 16.0, 22.0, accent);
        text("A Link Between Worlds", 200.0, 58.0, 0.8, WHITE, CENTER);
        text("Randomizer", 200.0, 86.0, 0.5, MUTED, CENTER);

        let current = STEP.load(Relaxed) as usize;
        let status = match (failure, finished) {
            (Some(_), _) => "Could not finish",
            (None, true) => "Your seed is ready",
            _ => STEPS[current].label,
        };
        text(status, 24.0, 118.0, 0.6, WHITE, LEFT);
        text(&format!("{:.0}%", overall * 100.0), 376.0, 118.0, 0.6, accent, RIGHT);

        let color = if failure.is_some() { RED } else if finished { GREEN } else { GOLD };
        bar(24.0, 142.0, 352.0, 18.0, overall, color, self.tick, !finished);

        match (failure, finished) {
            (Some(message), _) => {
                let short: String = message.chars().take(58).collect();
                text(&short, 24.0, 168.0, 0.42, RED, LEFT);
                text("Details are in spike.log", 24.0, 186.0, 0.42, MUTED, LEFT);
            }
            (None, true) => {
                let saved = SAVED_TO.lock().unwrap();
                let path = saved.as_deref().unwrap_or("/3ds/albw-rando/spike-out");
                text(&format!("Saved to {path}"), 24.0, 168.0, 0.42, GREEN, LEFT);
            }
            _ => text(&eta_text(eta), 24.0, 168.0, 0.5, GOLD, LEFT),
        }
        text(&clock(elapsed), 376.0, 168.0, 0.5, MUTED, RIGHT);

        if finished {
            text("Press START to exit", 200.0, 212.0, 0.5, WHITE, CENTER);
        } else {
            let model = if self.is_new { "New 3DS" } else { "Old 3DS" };
            text(&format!("{model}  -  Please keep the console powered"), 200.0, 214.0, 0.42, MUTED, CENTER);
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
            rect(10.0, y, 300.0, row_height, if active { CARD_ACTIVE } else { CARD });

            let dot = y + 18.0;
            if complete {
                circle(28.0, dot, 7.0, GREEN);
            } else if active && failed {
                circle(28.0, dot, 7.0, RED);
            } else if active {
                let pulse = ((self.tick as f32 * 0.15).sin() + 1.0) * 1.2;
                circle(28.0, dot, 5.0 + pulse, GOLD);
            } else {
                circle(28.0, dot, 6.0, DIM);
                circle(28.0, dot, 4.0, CARD);
            }

            let color = if complete || active { WHITE } else { MUTED };
            text(step.label, 44.0, if active { y + 3.0 } else { y + 9.0 }, 0.5, color, LEFT);
            if active && !failed {
                bar(44.0, y + 23.0, 256.0, 7.0, self.best[index], GOLD, self.tick, true);
            }
            if complete {
                let seconds = STEP_ENDS[index].load(Relaxed).wrapping_sub(STEP_STARTS[index].load(Relaxed)) as f32 / 1000.0;
                text(&clock(seconds), 300.0, y + 10.0, 0.45, GREEN, RIGHT);
            }
        }
        let used = (self.usage_mb)();
        text(&format!("Memory {used:.1} of {:.1} MB", self.heap_mb), 160.0, 214.0, 0.42, MUTED, CENTER);
    }
}

impl Drop for Screen {
    fn drop(&mut self) {
        // SAFETY: Called once, after the last frame.
        unsafe { c2d::shim_exit() };
    }
}
