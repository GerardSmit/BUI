//! Event types passed to listeners (named as in gpui).

use crate::geometry::{Pixels, Point};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    /// The mouse's back (side) button. Only listeners for this button hear it, never
    /// `on_any_mouse_down`: it is a window's navigation, not a press on what is under it.
    Back,
    /// The mouse's forward (side) button, heard as `Back` is.
    Forward,
}

/// Modifier keys held during an event.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub shift: bool,
    /// The Control key (on macOS only the Control key itself, never Command).
    pub control: bool,
    pub alt: bool,
    /// The platform key, as in gpui: Command on macOS; never set elsewhere.
    pub platform: bool,
}

impl Modifiers {
    pub(crate) fn from_kit(mods: i32) -> Modifiers {
        use blenderui_sys::{BK_ALT, BK_CMD, BK_CONTROL_KEY, BK_CTRL, BK_SHIFT};
        // Cocoa sets BK_CTRL for Command too; BK_CMD / BK_CONTROL_KEY say which key it was.
        let mac = mods & (BK_CMD | BK_CONTROL_KEY) != 0;
        Modifiers {
            shift: mods & BK_SHIFT != 0,
            control: if mac { mods & BK_CONTROL_KEY != 0 } else { mods & BK_CTRL != 0 },
            alt: mods & BK_ALT != 0,
            platform: mods & BK_CMD != 0,
        }
    }

    /// The kit's bits for these keys, as the host would report them.
    pub(crate) fn to_kit(self) -> i32 {
        use blenderui_sys::{BK_ALT, BK_CMD, BK_CONTROL_KEY, BK_CTRL, BK_SHIFT};
        let mut m = 0;
        if self.shift {
            m |= BK_SHIFT;
        }
        if self.alt {
            m |= BK_ALT;
        }
        if self.platform {
            m |= BK_CTRL | BK_CMD;
        }
        if self.control {
            m |= BK_CTRL;
            if self.platform || cfg!(target_os = "macos") {
                m |= BK_CONTROL_KEY;
            }
        }
        m
    }
}

#[cfg(test)]
mod modifier_tests {
    use super::Modifiers;

    #[test]
    fn modifiers_survive_the_kits_bits() {
        for bits in 0..16u8 {
            let m = Modifiers { shift: bits & 1 != 0, control: bits & 2 != 0, alt: bits & 4 != 0,
                                platform: bits & 8 != 0 };
            assert_eq!(Modifiers::from_kit(m.to_kit()), m);
        }
    }

    #[test]
    fn plain_ctrl_is_control_and_never_the_platform_key() {
        let m = Modifiers::from_kit(blenderui_sys::BK_CTRL);
        assert!(m.control && !m.platform);
    }

    #[test]
    fn cocoas_command_is_the_platform_key_not_control() {
        let m = Modifiers::from_kit(blenderui_sys::BK_CTRL | blenderui_sys::BK_CMD);
        assert!(m.platform && !m.control);
        let m = Modifiers::from_kit(blenderui_sys::BK_CTRL | blenderui_sys::BK_CONTROL_KEY);
        assert!(m.control && !m.platform);
    }
}

#[derive(Clone, Debug)]
pub struct MouseDownEvent {
    pub button: MouseButton,
    pub position: Point<Pixels>,
    pub modifiers: Modifiers,
    pub click_count: usize,
}

#[derive(Clone, Debug)]
pub struct MouseUpEvent {
    pub button: MouseButton,
    pub position: Point<Pixels>,
    pub modifiers: Modifiers,
}

#[derive(Clone, Debug)]
pub struct MouseMoveEvent {
    pub position: Point<Pixels>,
    pub pressed_button: Option<MouseButton>,
    pub modifiers: Modifiers,
}

/// A click: the press and the release.
#[derive(Clone, Debug)]
pub struct ClickEvent {
    pub down: MouseDownEvent,
    pub up: MouseUpEvent,
}

impl ClickEvent {
    pub fn click_count(&self) -> usize {
        self.down.click_count
    }
}

#[derive(Clone, Debug)]
pub struct ScrollWheelEvent {
    pub position: Point<Pixels>,
    /// Lines: positive scrolls the content up (wheel away from the user).
    pub delta: f32,
    pub modifiers: Modifiers,
}

/// A key: `key` is the keysym (`"a"`, `"Return"`, `"Left"` ...).
#[derive(Clone, Debug)]
pub struct Keystroke {
    pub key: String,
    pub key_char: Option<String>,
    pub modifiers: Modifiers,
}

#[derive(Clone, Debug)]
pub struct KeyDownEvent {
    pub keystroke: Keystroke,
}

#[derive(Clone, Debug)]
pub struct KeyUpEvent {
    pub keystroke: Keystroke,
}

/// Something dropped on an element from another app, or a drag out of the window come back:
/// the URIs its `text/uri-list` held.
#[derive(Clone, Debug)]
pub struct DropEvent {
    pub position: Point<Pixels>,
    pub uris: Vec<String>,
}

impl DropEvent {
    /// The URIs of a `text/uri-list` (one per line, `#` lines are comments).
    pub fn from_uri_list(position: Point<Pixels>, text: &str) -> DropEvent {
        let uris = text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#'))
            .map(str::to_owned).collect();
        DropEvent { position, uris }
    }

    /// The local files among the URIs (`file://` ones, percent-decoded).
    pub fn paths(&self) -> Vec<std::path::PathBuf> {
        self.uris.iter().filter_map(|u| file_uri_path(u)).collect()
    }
}

/// The path of a `file://` URI on this machine (an empty or `localhost` host), percent-decoded.
pub fn file_uri_path(uri: &str) -> Option<std::path::PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    let rest = rest.strip_prefix("localhost").unwrap_or(rest);
    if !rest.starts_with('/') {
        return None;
    }
    let bytes = rest.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    path_from_bytes(out)
}

#[cfg(windows)]
fn path_from_bytes(bytes: Vec<u8>) -> Option<std::path::PathBuf> {
    let s = String::from_utf8(bytes).ok()?;
    Some(std::path::PathBuf::from(s.trim_start_matches('/')))
}

#[cfg(not(windows))]
fn path_from_bytes(bytes: Vec<u8>) -> Option<std::path::PathBuf> {
    use std::os::unix::ffi::OsStringExt;
    Some(std::path::PathBuf::from(std::ffi::OsString::from_vec(bytes)))
}

/// A `file://` URI for a path, percent-encoding what a URI can't hold as is.
pub fn file_uri(path: &std::path::Path) -> String {
    let s = path.to_string_lossy().replace('\\', "/");
    let mut out = String::from("file://");
    if !s.starts_with('/') {
        out.push('/');
    }
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"/-_.~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// A form control's value (`input`, `select`, `textarea` ...).
#[derive(Clone, Debug, Default)]
pub struct ChangeEvent {
    /// The value as text (what `.value` holds in the DOM).
    pub value: String,
    /// Checkboxes and radio buttons.
    pub checked: bool,
    /// Numbers, ranges and selects (the selected index).
    pub number: f64,
}
