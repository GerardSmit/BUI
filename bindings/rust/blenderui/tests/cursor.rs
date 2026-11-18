//! The pointer's cursor follows what the last render set, even over the same element.

use std::cell::Cell;
use std::rc::Rc;

use blenderui::prelude::*;

struct Edge {
    near: Rc<Cell<bool>>,
}

impl Render for Edge {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let near = self.near.clone();
        body().m_0().child(
            div()
                .id("pane")
                .w(px(200.0))
                .h(px(100.0))
                .when(self.near.get(), |d| d.cursor_col_resize())
                .on_mouse_move(move |e, w, _| {
                    let now = e.position.x.0 > 150.0;
                    if near.replace(now) != now {
                        w.refresh();
                    }
                }),
        )
    }
}

#[test]
fn a_cursor_a_render_sets_shows_while_the_pointer_stays_on_the_same_element() {
    Application::headless().run(|cx| {
        let opts = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds { origin: point(px(0.0), px(0.0)), size: size(px(300.0), px(200.0)) })),
            ..Default::default()
        };
        let near = Rc::new(Cell::new(false));
        let win = cx.open_window(opts, |_, cx| cx.new(|_| Edge { near: near.clone() })).unwrap();
        let id = win.id;
        cx.draw(id);
        cx.simulate_mouse_move(id, 50.0, 50.0);
        cx.draw(id);
        assert_eq!(cx.window(id).unwrap().cursor_shape(), 0);
        cx.simulate_mouse_move(id, 160.0, 50.0);
        cx.draw(id);
        assert!(near.get());
        cx.simulate_mouse_move(id, 161.0, 50.0);
        cx.draw(id);
        assert_eq!(cx.window(id).unwrap().cursor_shape(), 1, "the col-resize cursor");
        cx.simulate_mouse_move(id, 50.0, 50.0);
        cx.draw(id);
        cx.simulate_mouse_move(id, 51.0, 50.0);
        cx.draw(id);
        assert_eq!(cx.window(id).unwrap().cursor_shape(), 0, "and back");
    });
}
