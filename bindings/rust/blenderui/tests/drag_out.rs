//! Drags out of the window (`Window::start_drag`) and drops on elements (`Div::on_drop`).

use std::cell::RefCell;
use std::rc::Rc;

use blenderui::prelude::*;

fn open<V: Render>(cx: &mut App, w: f32, h: f32, v: V) -> WindowHandle<V> {
    let opts = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds {
            origin: point(px(0.0), px(0.0)),
            size: size(px(w), px(h)),
        })),
        ..Default::default()
    };
    cx.open_window(opts, |_, cx| cx.new(|_| v)).unwrap()
}

#[derive(Default)]
struct Log {
    clicks: usize,
    moves_held: usize,
    drops: Vec<(String, Vec<String>)>,
}

struct Source {
    log: Rc<RefCell<Log>>,
}

impl Render for Source {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let (a, b, c, d) = (self.log.clone(), self.log.clone(), self.log.clone(), self.log.clone());
        body()
            .m_0()
            .child(
                div().id("file").w(px(80.0)).h(px(40.0))
                    .on_click(move |_, _, _| a.borrow_mut().clicks += 1)
                    .on_mouse_move(move |e, w, _| {
                        if e.pressed_button == Some(MouseButton::Left) {
                            b.borrow_mut().moves_held += 1;
                            w.start_drag(DragOut {
                                offers: vec![("text/uri-list".into(), b"file:///tmp/a%20b.png\r\n".to_vec()),
                                             ("text/plain".into(), b"/tmp/a b.png".to_vec())],
                                icon: Some(DragIcon { width: 2, height: 1, pixels: vec![0xff00_0000, 0] }),
                            });
                        }
                    }),
            )
            .child(div().id("box").w(px(80.0)).h(px(40.0))
                .on_drop(move |e, _, _| c.borrow_mut().drops.push(("box".into(), e.uris.clone()))))
            .child(div().id("outer").w(px(80.0)).h(px(40.0))
                .on_drop(move |e, _, _| d.borrow_mut().drops.push(("outer".into(), e.uris.clone())))
                .child(div().id("inner").size(px(10.0))))
    }
}

fn centre(cx: &mut App, id: WindowId, el: &str) -> (f32, f32) {
    let b = cx.element_bounds(id, el).unwrap();
    (b.origin.x + b.size.width / 2.0, b.origin.y + b.size.height / 2.0)
}

#[test]
fn a_held_move_starts_a_drag_and_the_press_makes_no_click() {
    Application::headless().run(|cx| {
        let log = Rc::new(RefCell::new(Log::default()));
        let win = open(cx, 200.0, 200.0, Source { log: log.clone() });
        let id = win.id;
        cx.draw(id);
        let (x, y) = centre(cx, id, "file");
        cx.simulate_drag(id, (x, y), (x + 20.0, y));
        let drags = cx.window(id).unwrap().drags_out().to_vec();
        assert_eq!(drags.len(), 1, "one drag, the moves after it see no button held");
        assert_eq!(log.borrow().moves_held, 1);
        assert_eq!(drags[0].offered("text/plain"), Some(&b"/tmp/a b.png"[..]));
        assert_eq!(drags[0].offered("text/uri-list"), Some(&b"file:///tmp/a%20b.png\r\n"[..]));
        assert_eq!(drags[0].icon.as_ref().map(|i| (i.width, i.height)), Some((2, 1)));
        assert_eq!(log.borrow().clicks, 0, "the release that ends a drag is no click");
        cx.simulate_click(id, x, y);
        assert_eq!(log.borrow().clicks, 1, "a click after the drag still clicks");
    });
}

#[test]
fn no_drag_starts_without_a_press() {
    Application::headless().run(|cx| {
        let log = Rc::new(RefCell::new(Log::default()));
        let win = open(cx, 200.0, 200.0, Source { log });
        let id = win.id;
        cx.draw(id);
        let started = cx.window(id).is_some();
        assert!(started);
        let (x, y) = centre(cx, id, "file");
        cx.simulate_mouse_move(id, x, y);
        cx.simulate_mouse_move(id, x + 5.0, y);
        assert!(cx.window(id).unwrap().drags_out().is_empty());
    });
}

#[test]
fn a_drop_reaches_the_element_under_it_and_its_ancestors() {
    Application::headless().run(|cx| {
        let log = Rc::new(RefCell::new(Log::default()));
        let win = open(cx, 200.0, 200.0, Source { log: log.clone() });
        let id = win.id;
        cx.draw(id);
        let (x, y) = centre(cx, id, "box");
        cx.simulate_drop(id, x, y, "# from a file manager\r\nfile:///tmp/a%20b.png\r\nfile:///tmp/c\r\n");
        let (ix, iy) = centre(cx, id, "inner");
        cx.simulate_drop(id, ix, iy, "file:///tmp/d");
        let (fx, fy) = centre(cx, id, "file");
        cx.simulate_drop(id, fx, fy, "file:///tmp/e");
        cx.simulate_drop(id, x, y, "\r\n# nothing\r\n");
        let drops = &log.borrow().drops;
        assert_eq!(drops, &vec![
            ("box".to_string(), vec!["file:///tmp/a%20b.png".to_string(), "file:///tmp/c".to_string()]),
            ("outer".to_string(), vec!["file:///tmp/d".to_string()]),
        ]);
    });
}

#[test]
fn file_uris_are_paths_and_back() {
    let e = DropEvent::from_uri_list(point(px(0.0), px(0.0)),
                                     "file:///tmp/a%20b.png\nhttps://example.com/x\nfile://localhost/srv/c%25d\nfile://host/x\n");
    assert_eq!(e.paths(), vec![std::path::PathBuf::from("/tmp/a b.png"), std::path::PathBuf::from("/srv/c%d")]);
    assert_eq!(file_uri(std::path::Path::new("/tmp/a b#1.png")), "file:///tmp/a%20b%231.png");
    assert_eq!(file_uri_path(&file_uri(std::path::Path::new("/tmp/é ü"))), Some(std::path::PathBuf::from("/tmp/é ü")));
}

#[test]
fn a_headless_clipboard_takes_several_types_and_pastes_the_text() {
    Application::headless().run(|cx| {
        let log = Rc::new(RefCell::new(Log::default()));
        let win = open(cx, 200.0, 200.0, Source { log });
        let id = win.id;
        cx.draw(id);
        let offers = vec![("image/png".to_string(), b"\x89PNG".to_vec()), ("text/plain".to_string(), b"/tmp/a.png".to_vec())];
        let took = win.update(cx, |_, w, _| w.write_clipboard_offers(&offers)).unwrap();
        assert!(took);
        assert_eq!(cx.window(id).unwrap().clipboard_offers(), offers.as_slice());
        assert_eq!(headless_clipboard().as_deref(), Some("/tmp/a.png"));
    });
}
