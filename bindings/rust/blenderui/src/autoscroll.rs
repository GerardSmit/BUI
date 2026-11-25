//! Browser-style middle-button autoscroll.
//!
//! A middle press over something that scrolls (a scroll container, or an element that hears the
//! wheel, as a virtual list does) anchors there, unless something under it listens for the
//! middle button's press itself (`on_mouse_down(MouseButton::Middle, ..)`: an image viewer's
//! pan), which then hears it as any press. It scrolls up and down, and sideways too when a
//! container under it overflows sideways or an `on_autoscroll` listener there says it scrolls
//! sideways ([`crate::Window::autoscroll_sideways`]); sideways steps reach listeners as wheel
//! events with Shift held. While the button is held, the pointer's distance from the anchor
//! along each axis scrolls it, faster the further it is, every frame; the release stops it. A
//! press and release without leaving the anchor's dead zone makes it sticky: moving the pointer
//! scrolls until any click, Escape or the wheel. The window captures the pointer meanwhile
//! (`BK_capture_pointer`: over a text box too) and shows the system's pan cursor for the way it
//! goes ([`Anchor::cursor`]).
//!
//! Frames come only while it moves: inside the dead zone, or with nothing left to scroll (the top
//! or the bottom), none are asked for until the pointer moves again.

use crate::layout::Key;

/// Pixels around the anchor that scroll nothing.
pub const DEAD_ZONE: f32 = 12.0;

/// The fastest scroll, px per second.
pub const MAX_SPEED: f32 = 6000.0;

/// The longest step a frame takes, in seconds (a stalled frame doesn't jump).
pub const MAX_STEP: f64 = 0.1;

/// Scroll speed (px per second, positive: down or right in the content) for the pointer `d` px
/// past the anchor along an axis: none in the dead zone, then about 7 px/s per px, a little
/// faster further out.
#[must_use]
pub fn speed(d: f32) -> f32 {
    let past = d.abs() - DEAD_ZONE;
    if past <= 0.0 {
        return 0.0;
    }
    (past * 7.0 + past * past * 0.04).min(MAX_SPEED) * d.signum()
}

/// The ways an autoscroll can scroll, fixed when it starts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Axes {
    pub x: bool,
    pub y: bool,
}

/// Which way an autoscroll goes with the pointer where it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Heading {
    /// In the dead zone: nothing scrolls.
    Still,
    N,
    NE,
    E,
    SE,
    S,
    SW,
    W,
    NW,
}

impl Heading {
    /// The heading of a scroll at `v` (px per second, x then y; positive: right, down).
    #[must_use]
    pub fn of(v: (f32, f32)) -> Heading {
        use std::cmp::Ordering::{Equal, Greater, Less};
        let sign = |a: f32| a.partial_cmp(&0.0).unwrap_or(Equal);
        match (sign(v.0), sign(v.1)) {
            (Equal, Equal) => Heading::Still,
            (Equal, Less) => Heading::N,
            (Greater, Less) => Heading::NE,
            (Greater, Equal) => Heading::E,
            (Greater, Greater) => Heading::SE,
            (Equal, Greater) => Heading::S,
            (Less, Greater) => Heading::SW,
            (Less, Equal) => Heading::W,
            (Less, Less) => Heading::NW,
        }
    }
}

/// The kit's autoscroll cursors (`BK_CURSOR_SCROLL_*`): the system's pan cursors (Windows'
/// resources 32652 to 32662; the CSS `*-resize` / `all-scroll` cursors on Wayland, the
/// cursor font's arrows on X11, the resize cursors on macOS).
pub mod cursor {
    /// Still, up and down.
    pub const NS: i32 = 13;
    /// Still, left and right.
    pub const WE: i32 = 14;
    /// Still, every way.
    pub const ALL: i32 = 15;
    pub const N: i32 = 16;
    pub const S: i32 = 17;
    pub const W: i32 = 18;
    pub const E: i32 = 19;
    pub const NW: i32 = 20;
    pub const NE: i32 = 21;
    pub const SW: i32 = 22;
    pub const SE: i32 = 23;
}

/// An autoscroll as a host shows it ([`crate::Window::autoscroll`]): where it is anchored
/// (window logical px, where its marker goes), the ways it can go, and the way it goes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Anchor {
    pub at: (f32, f32),
    pub axes: Axes,
    pub heading: Heading,
}

impl Anchor {
    /// The cursor it shows: still, the one for the ways it can go; moving, the way it goes.
    #[must_use]
    pub fn cursor(&self) -> i32 {
        match self.heading {
            Heading::Still => match (self.axes.x, self.axes.y) {
                (true, true) => cursor::ALL,
                (true, false) => cursor::WE,
                _ => cursor::NS,
            },
            Heading::N => cursor::N,
            Heading::NE => cursor::NE,
            Heading::E => cursor::E,
            Heading::SE => cursor::SE,
            Heading::S => cursor::S,
            Heading::SW => cursor::SW,
            Heading::W => cursor::W,
            Heading::NW => cursor::NW,
        }
    }
}

/// An autoscroll in progress.
#[derive(Clone, Debug)]
pub(crate) struct Autoscroll {
    /// Where the middle button went down (window logical px).
    pub origin: (f32, f32),
    /// The elements under it then, outermost first: what scrolls.
    pub path: Vec<Key>,
    /// The ways it scrolls.
    pub axes: Axes,
    /// Released without leaving the dead zone: it goes on until a click, Escape or the wheel.
    pub sticky: bool,
    /// The pointer has left the dead zone since the press.
    pub moved: bool,
    /// The clock time of the last step, while moving (`None`: still).
    pub last: Option<f64>,
}

impl Autoscroll {
    pub fn new(origin: (f32, f32), path: Vec<Key>, axes: Axes) -> Self {
        Autoscroll { origin, path, axes, sticky: false, moved: false, last: None }
    }

    /// The pointer is at `p`: whether it is out of the dead zone (it notes it).
    pub fn pointer(&mut self, p: (f32, f32)) -> bool {
        let out = (p.0 - self.origin.0).hypot(p.1 - self.origin.1) > DEAD_ZONE;
        self.moved |= out;
        out
    }

    /// Its speed along each axis it scrolls with the pointer at `p`, px per second.
    pub fn velocity(&self, p: (f32, f32)) -> (f32, f32) {
        let along = |on: bool, d: f32| if on { speed(d) } else { 0.0 };
        (along(self.axes.x, p.0 - self.origin.0), along(self.axes.y, p.1 - self.origin.1))
    }

    /// What a host shows of it with the pointer at `p`.
    pub fn anchor(&self, p: (f32, f32)) -> Anchor {
        Anchor { at: self.origin, axes: self.axes, heading: Heading::of(self.velocity(p)) }
    }

    /// How far to scroll at clock time `now` with the pointer at `p` (px, x then y; positive:
    /// right, down), or `None` when still (no frame wanted). The first step after stillness
    /// only starts the clock: `Some((0.0, 0.0))`.
    pub fn step(&mut self, now: f64, p: (f32, f32)) -> Option<(f32, f32)> {
        let v = self.velocity(p);
        if v == (0.0, 0.0) {
            self.last = None;
            return None;
        }
        let Some(last) = self.last.replace(now) else { return Some((0.0, 0.0)) };
        let dt = (now - last).clamp(0.0, MAX_STEP) as f32;
        Some((v.0 * dt, v.1 * dt))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const UP_DOWN: Axes = Axes { x: false, y: true };
    const SIDEWAYS: Axes = Axes { x: true, y: false };
    const BOTH: Axes = Axes { x: true, y: true };

    #[test]
    fn the_dead_zone_scrolls_nothing_then_faster_further_out() {
        assert_eq!(speed(0.0), 0.0);
        assert_eq!(speed(DEAD_ZONE), 0.0);
        assert_eq!(speed(-DEAD_ZONE), 0.0);
        assert!(speed(40.0) > 0.0 && speed(-40.0) < 0.0);
        assert_eq!(speed(-40.0), -speed(40.0));
        assert!(speed(200.0) > 2.0 * speed(100.0) - 1.0);
        assert_eq!(speed(10_000.0), MAX_SPEED);
    }

    #[test]
    fn steps_follow_the_clock_and_stop_when_still() {
        let mut a = Autoscroll::new((100.0, 100.0), Vec::new(), UP_DOWN);
        assert!(!a.pointer((105.0, 105.0)));
        assert_eq!(a.step(1.0, (105.0, 105.0)), None);
        assert!(!a.moved);
        assert!(a.pointer((100.0, 200.0)));
        assert!(a.moved);
        // the first step starts the clock, the next moves by speed x time
        assert_eq!(a.step(2.0, (100.0, 200.0)), Some((0.0, 0.0)));
        let (dx, dy) = a.step(2.05, (100.0, 200.0)).unwrap();
        assert!(dx == 0.0 && (dy - speed(100.0) * 0.05).abs() < 0.01, "{dy}");
        // a long stall moves at most MAX_STEP's worth
        let (_, dy) = a.step(10.0, (100.0, 200.0)).unwrap();
        assert!((dy - speed(100.0) * MAX_STEP as f32).abs() < 0.01, "{dy}");
        // back in the dead zone: still
        assert_eq!(a.step(10.1, (100.0, 101.0)), None);
        assert_eq!(a.last, None);
        // up is negative
        assert_eq!(a.step(11.0, (100.0, 0.0)), Some((0.0, 0.0)));
        assert!(a.step(11.1, (100.0, 0.0)).unwrap().1 < 0.0);
        // sideways scrolls nothing on an up-and-down autoscroll
        assert_eq!(a.step(12.0, (300.0, 100.0)), None);
    }

    #[test]
    fn both_axes_step_together() {
        let mut a = Autoscroll::new((100.0, 100.0), Vec::new(), BOTH);
        assert_eq!(a.step(1.0, (0.0, 250.0)), Some((0.0, 0.0)));
        let (dx, dy) = a.step(1.05, (0.0, 250.0)).unwrap();
        assert!((dx - speed(-100.0) * 0.05).abs() < 0.01 && (dy - speed(150.0) * 0.05).abs() < 0.01, "{dx} {dy}");
    }

    #[test]
    fn the_cursor_is_the_pan_cursor_for_the_ways_it_can_go_and_the_way_it_goes() {
        let at = (100.0, 100.0);
        let shown = |axes: Axes, p: (f32, f32)| {
            let a = Autoscroll::new(at, Vec::new(), axes).anchor(p);
            assert_eq!(a.at, at, "the anchor stays where it was pressed");
            (a.heading, a.cursor())
        };
        let still = [(UP_DOWN, cursor::NS), (SIDEWAYS, cursor::WE), (BOTH, cursor::ALL)];
        for (axes, c) in still {
            assert_eq!(shown(axes, at), (Heading::Still, c), "{axes:?}");
            assert_eq!(shown(axes, (104.0, 96.0)), (Heading::Still, c), "{axes:?}: the dead zone");
        }
        let d = 80.0;
        let ways = [
            ((0.0, -d), Heading::N, cursor::N),
            ((d, -d), Heading::NE, cursor::NE),
            ((d, 0.0), Heading::E, cursor::E),
            ((d, d), Heading::SE, cursor::SE),
            ((0.0, d), Heading::S, cursor::S),
            ((-d, d), Heading::SW, cursor::SW),
            ((-d, 0.0), Heading::W, cursor::W),
            ((-d, -d), Heading::NW, cursor::NW),
        ];
        for ((dx, dy), heading, c) in ways {
            assert_eq!(shown(BOTH, (at.0 + dx, at.1 + dy)), (heading, c), "both ways, {heading:?}");
        }
        assert_eq!(shown(UP_DOWN, (at.0 + d, at.1 - d)), (Heading::N, cursor::N), "up and down only: sideways is ignored");
        assert_eq!(shown(UP_DOWN, (at.0 - d, at.1)), (Heading::Still, cursor::NS));
        assert_eq!(shown(SIDEWAYS, (at.0 - d, at.1 + d)), (Heading::W, cursor::W), "sideways only: up and down are ignored");
        assert_eq!(shown(SIDEWAYS, (at.0 + d, at.1)), (Heading::E, cursor::E));
        let all: std::collections::BTreeSet<i32> = still.iter().map(|s| s.1).chain(ways.iter().map(|w| w.2)).collect();
        assert_eq!(all, (13..=23).collect(), "every BK_CURSOR_SCROLL_* once");
    }
}
