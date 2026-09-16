//! Where the overlay goes, on every display, at every scale. Pure geometry, so all of it is exact.

use hanglock_core::placement::{pick_monitor, place, swept_box, Monitor, Rect, DEFAULT_MONITOR};
use hanglock_core::rope::config::{CardSpec, RopeConfig};

fn m(index: u32, x0: f64, y0: f64, w: f64, h: f64, scale: f64) -> Monitor {
    Monitor {
        index,
        bounds: Rect::new(x0, y0, x0 + w, y0 + h),
        work: Rect::new(x0, y0, x0 + w, y0 + h - 48.0),
        scale,
        taskbar_top: false,
        taskbar_auto_hidden: false,
        primary: index == 0,
    }
}

fn cfg() -> (RopeConfig, CardSpec) {
    (RopeConfig::default(), CardSpec::default())
}

#[test]
fn hangs_from_the_top_of_the_monitor_it_is_on() {
    let (c, card) = cfg();
    let mon = m(0, 0.0, 0.0, 1920.0, 1080.0, 1.0);
    let p = place(&mon, &c, &card, 150.0, 1.0, 0.5, 16.0, true);
    // Top edge: the frame's top is the display's top, not the work area's, because the taskbar is at
    // the bottom here and a hanging object belongs at the physical edge.
    assert_eq!(p.frame.y0, 0.0);
    assert!(
        p.frame.h() > 150.0,
        "frame must contain the whole swept area"
    );
    // Centred on a 1920-wide display.
    assert!(
        (p.frame.x0 + p.frame.w() * 0.5 - 960.0).abs() < 1.0,
        "not centred: {:?}",
        p.frame
    );
}

/// A secondary display to the *left* of the primary has a negative origin. This is the case that
/// breaks naive placement maths, and the reason the reference project's geometry is a pure function.
#[test]
fn a_monitor_left_of_the_primary_has_a_negative_origin() {
    let (c, card) = cfg();
    let left = m(1, -1920.0, 0.0, 1920.0, 1080.0, 1.0);
    let p = place(&left, &c, &card, 150.0, 1.0, 0.5, 16.0, true);
    let centre = p.frame.x0 + p.frame.w() * 0.5;
    assert!(
        (centre - (-960.0)).abs() < 1.0,
        "centre {centre} is not the left monitor's centre"
    );
    assert!(
        p.frame.x0 >= left.bounds.x0 - 0.5,
        "escaped the monitor on the left"
    );
    assert!(
        p.frame.x1 <= left.bounds.x1 + 0.5,
        "escaped the monitor on the right"
    );
}

/// 100 % on one display and 200 % on another is a normal Windows desktop, and the two spaces must not
/// be mixed: placement is computed in logical px and converted once.
#[test]
fn mixed_dpi_monitors_scale_the_frame_but_not_the_geometry() {
    let (c, card) = cfg();
    let mon = m(0, 0.0, 0.0, 1920.0, 1080.0, 2.0);
    let p = place(&mon, &c, &card, 150.0, 1.0, 0.5, 16.0, true);
    let logical = swept_box(&c, &card, 150.0, 16.0, 1.0);
    assert!(
        (p.frame.w() - logical.w() * 2.0).abs() < 1.0,
        "device width is not 2x the logical width"
    );
    assert!((p.frame.h() - logical.h() * 2.0).abs() < 1.0);
    // The anchor within the frame is reported in the same device px, so the painter needs no scale.
    assert!(p.anchor.x > 0.0 && p.anchor.y > 0.0);
    assert!((p.anchor.x - p.frame.w() * 0.5).abs() < 1.0);
}

#[test]
fn a_top_docked_taskbar_pushes_the_hang_line_down_when_respected() {
    let (c, card) = cfg();
    let mut mon = m(0, 0.0, 0.0, 1920.0, 1080.0, 1.0);
    mon.taskbar_top = true;
    // 48 px of taskbar at the top: work area starts below it.
    mon.work = Rect::new(0.0, 48.0, 1920.0, 1080.0);
    let respected = place(&mon, &c, &card, 150.0, 1.0, 0.5, 16.0, true);
    let over = place(&mon, &c, &card, 150.0, 1.0, 0.5, 16.0, false);
    assert!(
        respected.frame.y0 > over.frame.y0 + 30.0,
        "respect_taskbar did not move the hang line"
    );
}

#[test]
fn an_overlay_wider_than_the_display_centres_instead_of_failing() {
    let (c, card) = cfg();
    let tiny = m(0, 0.0, 0.0, 400.0, 300.0, 1.0);
    let p = place(&tiny, &c, &card, 260.0, 1.75, 0.5, 16.0, true);
    assert!(
        p.clipped,
        "must report that the sweep is clipped, for diagnostics"
    );
    assert_eq!(p.frame.x0, 0.0);
    assert!(
        p.frame.w() > tiny.bounds.w(),
        "overflow is the answer, not a shrunk clock"
    );
}

#[test]
fn a_stale_monitor_index_degrades_to_the_primary() {
    let mons = [
        m(0, 0.0, 0.0, 1920.0, 1080.0, 1.0),
        m(1, 1920.0, 0.0, 1920.0, 1080.0, 1.0),
    ];
    let refs: Vec<&Monitor> = mons.iter().collect();
    assert_eq!(pick_monitor(&refs, 1, 0).index, 1);
    assert_eq!(
        pick_monitor(&refs, 9, 0).index,
        0,
        "a monitor that went away must not hide the clock"
    );
    assert_eq!(pick_monitor(&[], 3, 0).index, DEFAULT_MONITOR.index);
}

#[test]
fn the_swept_box_tracks_the_sweep_and_the_hang() {
    let (c, card) = cfg();
    let short = swept_box(&c, &card, 70.0, 16.0, 1.0);
    let long = swept_box(&c, &card, 260.0, 16.0, 1.0);
    assert!(long.w() > short.w() && long.h() > short.h());
    let mut wide = c;
    wide.sweep_deg = 70.0;
    let wide_box = swept_box(&wide, &card, 150.0, 16.0, 1.0);
    let narrow_box = swept_box(&c, &card, 150.0, 16.0, 1.0);
    assert!(
        wide_box.w() > narrow_box.w(),
        "a wider sweep must widen the window: it is the same decision"
    );
}

#[test]
fn rect_helpers() {
    let r = Rect::new(10.0, 20.0, 110.0, 60.0);
    assert_eq!(r.w(), 100.0);
    assert_eq!(r.h(), 40.0);
    assert!(r.contains(hanglock_core::vec2::Vec2::new(10.0, 20.0)));
    assert!(
        !r.contains(hanglock_core::vec2::Vec2::new(110.0, 60.0)),
        "half-open: right/bottom excluded"
    );
}
