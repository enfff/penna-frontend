//! Swipe-gesture fallbacks for NavigationView's built-in swipes.
//!
//! NavigationView's built-in swipe gestures live in the bubble phase. When a
//! page's ScrolledWindow cannot scroll (a short note, or a grid that fits on
//! screen) the built-in swipe works as-is, with live finger-tracking. Once the
//! content is scrollable, the ScrolledWindow consumes the smooth-scroll stream
//! for vertical scrolling and starves the built-in tracker. These installers
//! re-add equivalent capture-phase controllers that engage only in that
//! scrollable case, so the gesture keeps working without double-firing.

use std::cell::Cell;
use std::rc::Rc;

use gtk::glib;
use gtk::prelude::*;
use gtk::subclass::prelude::*;

use crate::window::PennaFrontendWindow;

const BACK_SWIPE_EDGE_ZONE_PX: f64 = 48.0;
const BACK_SWIPE_MIN_DISTANCE_PX: f64 = 16.0;
// Rightward-dominant motion past this commits the back-swipe. Must stay well
// below the editor's drag threshold so the capture gesture claims the
// sequence before the ScrolledWindow does.
const BACK_SWIPE_EARLY_COMMIT_PX: f64 = 4.0;
// Matches GtkSettings `gtk-dnd-drag-threshold` (default 8): the distance at
// which the ScrolledWindow's own drag gesture would claim the sequence.
const BACK_SWIPE_DECIDE_PX: f64 = 8.0;
// Touchpad back-swipe: two-finger swiping reaches us as a smooth scroll
// stream, not touch events. Accumulated deltas past this decide whether the
// stream is horizontal enough to be a back-swipe at all.
const BACK_SWIPE_TOUCHPAD_DECIDE_PX: f64 = 10.0;
// Accumulated rightward travel that triggers the actual pop. The pop runs the
// standard animated transition; there is no live finger-tracking because
// libadwaita's interactive swipe is driven by private tracker internals we
// cannot feed from a capture-phase controller. Keep this low so the wait
// before the animation feels short.
const BACK_SWIPE_TOUCHPAD_POP_PX: f64 = 40.0;

// NavigationView's built-in back-swipe is a bubble-phase gesture, so it
// loses arbitration to the editor's ScrolledWindow once a note is long
// enough to scroll and the swipe never fires. Re-adding it as a
// capture-phase drag on the editor page makes it the first gesture
// consulted, ahead of any descendant. It claims the sequence as soon as
// the drag is clearly a rightward swipe from the left edge, which cancels
// every descendant gesture (the ScrolledWindow included) before its
// bubble phase runs; anything else (vertical scroll, a touch that did not
// start at the edge) is denied so the usual scrolling / text behaviour
// still works.
pub fn install_editor_back_swipe(window: &PennaFrontendWindow) {
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    enum Phase {
        Ignored,
        Deciding,
        Back,
    }
    let phase: Rc<Cell<Option<Phase>>> = Rc::new(Cell::new(None));

    let gesture = gtk::GestureDrag::builder().touch_only(true).build();
    gesture.set_propagation_phase(gtk::PropagationPhase::Capture);

    {
        let phase = Rc::clone(&phase);
        gesture.connect_drag_begin(glib::clone!(
            #[weak(rename_to = window)]
            window,
            move |gesture, start_x, _| {
                let imp = window.imp();
                let in_editor = *imp.in_editor_view.borrow();
                if !in_editor || start_x > BACK_SWIPE_EDGE_ZONE_PX {
                    phase.set(Some(Phase::Ignored));
                    gesture.set_state(gtk::EventSequenceState::Denied);
                    return;
                }
                phase.set(Some(Phase::Deciding));
            }
        ));
    }

    {
        let phase = Rc::clone(&phase);
        gesture.connect_drag_update(move |gesture, dx, dy| {
            let current = phase.get();
            if current != Some(Phase::Deciding) {
                return;
            }
            if dx >= BACK_SWIPE_EARLY_COMMIT_PX && dx >= dy.abs() {
                // Commit before the ScrolledWindow's drag gesture reaches
                // its threshold: claiming from a capture phase cancels
                // all descendant gestures for this sequence.
                phase.set(Some(Phase::Back));
                gesture.set_state(gtk::EventSequenceState::Claimed);
            } else if dx.abs() >= BACK_SWIPE_DECIDE_PX || dy.abs() >= BACK_SWIPE_DECIDE_PX {
                // Past the scroll threshold without a rightward commit:
                // this is a scroll or other gesture, not a back-swipe.
                phase.set(Some(Phase::Ignored));
                gesture.set_state(gtk::EventSequenceState::Denied);
            }
        });
    }

    {
        let phase = Rc::clone(&phase);
        gesture.connect_drag_end(glib::clone!(
            #[weak(rename_to = window)]
            window,
            move |_, dx, _| {
                let confirmed =
                    phase.get() == Some(Phase::Back) && dx >= BACK_SWIPE_MIN_DISTANCE_PX;
                phase.set(None);
                if confirmed {
                    window.show_grid_view();
                }
            }
        ));
    }

    window.imp().editor_page.add_controller(gesture);
}

// Touchpads never produce touch events: a two-finger swipe arrives as a
// smooth scroll stream. libadwaita's NavigationView handles these via a
// bubble-phase scroll controller (see adw-swipe-tracker.c), which works
// until the editor's ScrolledWindow starts consuming the stream for
// vertical scrolling on long notes — the tracker is starved and the
// built-in back-swipe dies. So we watch the stream ourselves from a
// capture-phase EventControllerScroll on the editor page, mirroring the
// tracker's semantics: touchpad swipes are not positional, so no edge
// zone applies (its default swipe area is the whole view), and with
// natural scrolling a rightward finger push yields NEGATIVE dx (the
// tracker likewise maps delta < 0 to back-navigation). Only engage when
// the content is actually scrollable — otherwise the built-in tracker
// still works and we'd risk double-popping.
pub fn install_editor_back_swipe_touchpad(window: &PennaFrontendWindow) {
    #[derive(Clone, Copy)]
    enum Stream {
        Idle,
        Armed { acc_dx: f64, acc_dy: f64 },
        Done,
    }
    let stream: Rc<Cell<Stream>> = Rc::new(Cell::new(Stream::Idle));

    let gesture = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::BOTH_AXES);
    gesture.set_propagation_phase(gtk::PropagationPhase::Capture);

    {
        let stream = Rc::clone(&stream);
        gesture.connect_scroll_begin(glib::clone!(
            #[weak(rename_to = window)]
            window,
            move |_| {
                stream.set(Stream::Idle);
                let imp = window.imp();
                let in_editor = *imp.in_editor_view.borrow();
                // Touchpad swipes are not positional: engage anywhere on
                // the page, like libadwaita's own whole-view swipe area.
                let armed = in_editor && editor_content_scrollable(&window);
                if armed {
                    stream.set(Stream::Armed {
                        acc_dx: 0.0,
                        acc_dy: 0.0,
                    });
                }
            }
        ));
    }

    {
        let stream = Rc::clone(&stream);
        gesture.connect_scroll(glib::clone!(
            #[weak(rename_to = window)]
            window,
            #[upgrade_or_else]
            || glib::Propagation::Proceed,
            move |_, dx, dy| {
                match stream.get() {
                    Stream::Idle => glib::Propagation::Proceed,
                    Stream::Done => glib::Propagation::Stop,
                    Stream::Armed {
                        mut acc_dx,
                        mut acc_dy,
                    } => {
                        acc_dx += dx;
                        acc_dy += dy;
                        if acc_dx.abs().max(acc_dy.abs()) >= BACK_SWIPE_TOUCHPAD_DECIDE_PX {
                            // Natural scrolling: a rightward finger push
                            // arrives as negative dx, matching how the
                            // tracker maps delta < 0 to back-navigation.
                            if acc_dx < 0.0 && -acc_dx >= acc_dy.abs() {
                                if -acc_dx >= BACK_SWIPE_TOUCHPAD_POP_PX {
                                    window.show_grid_view();
                                    stream.set(Stream::Done);
                                    return glib::Propagation::Stop;
                                }
                            } else {
                                stream.set(Stream::Idle);
                                return glib::Propagation::Proceed;
                            }
                        }
                        stream.set(Stream::Armed { acc_dx, acc_dy });
                        glib::Propagation::Proceed
                    }
                }
            }
        ));
    }

    {
        let stream = Rc::clone(&stream);
        gesture.connect_scroll_end(move |_| {
            stream.set(Stream::Idle);
        });
    }

    window.imp().editor_page.add_controller(gesture);
}

// Touchpad forward-swipe on the notes grid: a two-finger leftward swipe
// reaches us as a smooth scroll stream, not touch events. When the grid is not
// scrollable, NavigationView's built-in forward swipe already handles it with
// live finger-tracking, so this only engages when the grid is scrollable —
// otherwise the ScrolledWindow starves the built-in tracker and the swipe
// dies. Mirrors the editor touchpad back-swipe, but fires on leftward motion
// (POSITIVE dx with natural scrolling).
pub fn install_grid_reopen_swipe_touchpad(window: &PennaFrontendWindow) {
    #[derive(Clone, Copy)]
    enum Stream {
        Idle,
        Armed { acc_dx: f64, acc_dy: f64 },
        Done,
    }
    let stream: Rc<Cell<Stream>> = Rc::new(Cell::new(Stream::Idle));

    let gesture = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::BOTH_AXES);
    gesture.set_propagation_phase(gtk::PropagationPhase::Capture);

    {
        let stream = Rc::clone(&stream);
        gesture.connect_scroll_begin(glib::clone!(
            #[weak(rename_to = window)]
            window,
            move |_| {
                stream.set(Stream::Idle);
                let imp = window.imp();
                let in_grid = *imp.in_notes_grid_view.borrow();
                // Only engage when the grid actually scrolls: when it does not,
                // the built-in NavigationView forward swipe handles the gesture
                // (with live tracking) and we would double-fire alongside it.
                let armed = in_grid && grid_content_scrollable(&window);
                if armed {
                    stream.set(Stream::Armed {
                        acc_dx: 0.0,
                        acc_dy: 0.0,
                    });
                }
            }
        ));
    }

    {
        let stream = Rc::clone(&stream);
        gesture.connect_scroll(glib::clone!(
            #[weak(rename_to = window)]
            window,
            #[upgrade_or_else]
            || glib::Propagation::Proceed,
            move |_, dx, dy| {
                match stream.get() {
                    Stream::Idle => glib::Propagation::Proceed,
                    Stream::Done => glib::Propagation::Stop,
                    Stream::Armed {
                        mut acc_dx,
                        mut acc_dy,
                    } => {
                        acc_dx += dx;
                        acc_dy += dy;
                        if acc_dx.abs().max(acc_dy.abs()) >= BACK_SWIPE_TOUCHPAD_DECIDE_PX {
                            if acc_dx > 0.0 && acc_dx >= acc_dy.abs() {
                                if acc_dx >= BACK_SWIPE_TOUCHPAD_POP_PX {
                                    window.reopen_last_entry();
                                    stream.set(Stream::Done);
                                    return glib::Propagation::Stop;
                                }
                            } else {
                                stream.set(Stream::Idle);
                                return glib::Propagation::Proceed;
                            }
                        }
                        stream.set(Stream::Armed { acc_dx, acc_dy });
                        glib::Propagation::Proceed
                    }
                }
            }
        ));
    }

    {
        let stream = Rc::clone(&stream);
        gesture.connect_scroll_end(move |_| {
            stream.set(Stream::Idle);
        });
    }

    window.imp().notes_page.add_controller(gesture);
}

fn editor_content_scrollable(window: &PennaFrontendWindow) -> bool {
    window
        .imp()
        .editor_view
        .ancestor(gtk::ScrolledWindow::static_type())
        .and_then(|widget| widget.downcast::<gtk::ScrolledWindow>().ok())
        .is_some_and(|scrolled| {
            let adj = scrolled.vadjustment();
            adj.upper() > adj.page_size() + 1.0
        })
}

fn grid_content_scrollable(window: &PennaFrontendWindow) -> bool {
    window
        .imp()
        .notes_flowbox
        .ancestor(gtk::ScrolledWindow::static_type())
        .and_then(|widget| widget.downcast::<gtk::ScrolledWindow>().ok())
        .is_some_and(|scrolled| {
            let adj = scrolled.vadjustment();
            adj.upper() > adj.page_size() + 1.0
        })
}
