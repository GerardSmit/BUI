//! `BUI_PROFILE=1`: per-phase frame timings, averaged and printed to stderr
//! every 60 frames. Costs one thread-local bool check when off.

use std::cell::Cell;
use std::time::Instant;

/// The timed phases of a frame.
#[derive(Clone, Copy)]
pub(crate) enum Phase {
    FrameInterval,
    Render,
    Cascade,
    Build,
    Boxes,
    Taffy,
    Place,
    Paint,
    Flush,
    Widgets,
}

const NAMES: [&str; 10] = [
    "frame interval", "render", "  cascade", "build", "boxes", "taffy", "place", "paint", "flush",
    "widgets+hover",
];

thread_local! {
    static PRINTS: bool = std::env::var_os("BUI_PROFILE").is_some();
    static COUNTS: Cell<bool> = const { Cell::new(false) };
    static ACC: [Cell<f64>; 10] = const { [const { Cell::new(0.0) }; 10] };
    static TOTAL: [Cell<f64>; 10] = const { [const { Cell::new(0.0) }; 10] };
    static FRAMES: Cell<u32> = const { Cell::new(0) };
}

pub(crate) fn on() -> bool {
    PRINTS.with(|p| *p) || COUNTS.with(Cell::get)
}

/// Milliseconds this thread's frames spent in each part, summed while
/// [`count_phases`] is on (or `BUI_PROFILE` is set): subtract two readings
/// to place one frame's cost.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PhaseTimes {
    /// The views' `render` and deferred elements: building the element tree.
    pub render: f64,
    /// Cascade, boxes, taffy and placing: laying the tree out.
    pub layout: f64,
    /// Painting the laid-out frame and flushing it to the canvas.
    pub paint: f64,
    /// Widgets, hover and the kit's own drawing (rasterising, headless).
    pub widgets: f64,
}

impl std::ops::Sub for PhaseTimes {
    type Output = Self;

    fn sub(self, o: Self) -> Self {
        Self { render: self.render - o.render, layout: self.layout - o.layout, paint: self.paint - o.paint, widgets: self.widgets - o.widgets }
    }
}

/// Start or stop summing [`PhaseTimes`] on this thread.
pub fn count_phases(on: bool) {
    COUNTS.with(|c| c.set(on));
}

/// The [`PhaseTimes`] summed so far on this thread.
#[must_use]
pub fn phase_times() -> PhaseTimes {
    TOTAL.with(|t| {
        let at = |p: Phase| t[p as usize].get();
        PhaseTimes {
            render: at(Phase::Render),
            layout: at(Phase::Build) + at(Phase::Boxes) + at(Phase::Taffy) + at(Phase::Place),
            paint: at(Phase::Paint) + at(Phase::Flush),
            widgets: at(Phase::Widgets),
        }
    })
}

/// Times its scope into a phase.
pub(crate) struct Span(Option<(Phase, Instant)>);

pub(crate) fn span(phase: Phase) -> Span {
    Span(on().then(|| (phase, Instant::now())))
}

impl Drop for Span {
    fn drop(&mut self) {
        if let Some((phase, t)) = self.0 {
            add(phase, t.elapsed().as_secs_f64() * 1000.0);
        }
    }
}

/// Add `ms` to a phase.
pub(crate) fn add(phase: Phase, ms: f64) {
    ACC.with(|a| {
        let c = &a[phase as usize];
        c.set(c.get() + ms);
    });
    TOTAL.with(|t| {
        let c = &t[phase as usize];
        c.set(c.get() + ms);
    });
}

/// End of a frame: print the averages every 60 frames.
pub(crate) fn frame_done() {
    if !PRINTS.with(|p| *p) {
        return;
    }
    let n = FRAMES.with(|f| {
        f.set(f.get() + 1);
        f.get()
    });
    if n < 60 {
        return;
    }
    FRAMES.with(|f| f.set(0));
    let line: Vec<String> = ACC.with(|a| {
        NAMES.iter().zip(a.iter())
            .filter(|(_, c)| c.get() > 0.0)
            .map(|(name, c)| format!("{name} {:.2}", c.replace(0.0) / 60.0))
            .collect()
    });
    eprintln!("frame ms: {}", line.join("  "));
}
