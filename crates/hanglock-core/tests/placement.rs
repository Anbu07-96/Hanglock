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
    let p = place(&mon, &c, &card, 150.0, 1.0, 0.5, 0.0, 16.0, true);
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
    let p = place(&left, &c, &card, 150.0, 1.0, 0.5, 0.0, 16.0, true);
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
    let p = place(&mon, &c, &card, 150.0, 1.0, 0.5, 0.0, 16.0, true);
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
    let respected = place(&mon, &c, &card, 150.0, 1.0, 0.5, 0.0, 16.0, true);
    let over = place(&mon, &c, &card, 150.0, 1.0, 0.5, 0.0, 16.0, false);
    // The hang line is the anchor, not the frame's top edge. The frame starts one top margin above
    // the anchor, and is then clamped inside the display, so comparing `frame.y0` values measures
    // that margin (39 px here) rather than the rule — which is why this assertion asked for 30 px
    // of a difference the geometry cannot produce. Absolute anchor = frame origin + the anchor
    // offset the painter is given, which is the y the cord actually hangs from.
    let hang_respected = respected.frame.y0 + respected.anchor.y;
    let hang_over = over.frame.y0 + over.anchor.y;
    // Respecting the taskbar puts it on the work area, to the pixel: 48.
    assert!(
        (hang_respected - 48.0).abs() < 1e-9,
        "hang line at {hang_respected}, not the work-area top"
    );
    // Ignoring it leaves the cord's first 9 px behind the taskbar — the failure the option exists
    // to prevent, and the reason `respect_taskbar` defaults to true.
    assert!(
        hang_over < 48.0,
        "hang line at {hang_over} is not behind the taskbar"
    );
}

#[test]
fn an_overlay_wider_than_the_display_centres_instead_of_failing() {
    let (c, card) = cfg();
    let tiny = m(0, 0.0, 0.0, 400.0, 300.0, 1.0);
    let p = place(&tiny, &c, &card, 260.0, 1.75, 0.5, 0.0, 16.0, true);
    assert!(
        p.clipped,
        "must report that the sweep is clipped, for diagnostics"
    );
    // The rule in `place` is symmetric overflow, and this test is named for it. The assertion that
    // used to sit here — `p.frame.x0 == 0.0` — is the clamped answer the rule deliberately rejects,
    // so it asserted the opposite of the behaviour under test. Tolerance rather than exact equality
    // because `frame` is built by adding and subtracting the same wide size, which costs an ulp.
    let centre = p.frame.x0 + p.frame.w() * 0.5;
    let display_centre = tiny.bounds.x0 + tiny.bounds.w() * 0.5;
    assert!(
        (centre - display_centre).abs() < 1e-6,
        "the overlay is not centred on the display: {:?}",
        p.frame
    );
    assert!(
        p.frame.x0 < tiny.bounds.x0 && p.frame.x1 > tiny.bounds.x1,
        "an overlay wider than the display must overflow both sides evenly: {:?}",
        p.frame
    );
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

/// The drop is the other half of the hang point, and the frame is derived *from* it rather than the
/// other way round: that is what makes an Alt-drag move the clock one pixel per pixel of cursor with
/// no dead band at the top of the screen.
#[test]
fn a_drop_moves_the_hang_point_down_one_logical_px_per_px() {
    let (c, card) = cfg();
    let mon = m(0, 0.0, 0.0, 1920.0, 1080.0, 1.0);
    let line = |drop: f64| {
        let p = place(&mon, &c, &card, 150.0, 1.0, 0.5, drop, 16.0, true);
        p.frame.y0 + p.anchor.y
    };
    // Zero is "as high as the display allows", which is the clamp's own inset, not the edge: the
    // hardware the cord runs through would otherwise be cut off.
    assert!((line(0.0) - 14.0).abs() < 1e-9, "at the inset: {}", line(0.0));
    for drop in [40.0, 100.0, 250.0] {
        let got = line(drop);
        assert!(
            (got - drop).abs() < 1e-9,
            "drop {drop} placed the hang point at {got}"
        );
    }
}

#[test]
fn a_drop_leaves_the_frame_inside_the_display_and_the_swing_inside_the_frame() {
    let (c, card) = cfg();
    let mon = m(0, 0.0, 0.0, 1920.0, 1080.0, 1.0);
    let box_logical = swept_box(&c, &card, 150.0, 16.0, 1.0);
    for drop in [0.0, 17.5, 100.0, 600.0] {
        let p = place(&mon, &c, &card, 150.0, 1.0, 0.5, drop, 16.0, true);
        let anchor_y = p.frame.y0 + p.anchor.y;
        assert!(
            p.frame.y0 >= -0.001 && p.frame.y1 <= 1080.0 + 0.001,
            "drop {drop} put the frame at {:?}",
            p.frame
        );
        // The bottom of the swept box, measured from the anchor: the plate at full stretch, plus its
        // shadow. If the frame's bottom came above this, the swing would be cut off in the window.
        let swing_bottom = anchor_y + box_logical.y1;
        assert!(
            p.frame.y1 >= swing_bottom - 0.001,
            "drop {drop}: frame bottom {} is above the swing's {}",
            p.frame.y1,
            swing_bottom
        );
    }
}

/// A display too short for the swing has no drop that fits. The answer is the top of the display, not
/// a panic and not a refusal to place the window: a clock the user can see and reset beats one that
/// declined to exist.
#[test]
fn a_short_display_floors_the_drop_at_the_inset_instead_of_failing() {
    let (c, card) = cfg();
    let short = m(0, 0.0, 0.0, 800.0, 300.0, 1.0);
    let p = place(&short, &c, &card, 150.0, 1.0, 0.5, 500.0, 16.0, true);
    // The frame cannot fit and says so; the hang point is nonetheless on the display, which is what
    // leaves the clock reachable enough to be reset.
    let anchor_y = p.frame.y0 + p.anchor.y;
    assert!(
        anchor_y > 0.0 && anchor_y < 300.0,
        "hang point at {anchor_y} is off a 300 px display: {:?}",
        p.frame
    );
    assert!(
        p.anchor.y > 0.0 && p.anchor.y < p.frame.h(),
        "the hang point left the frame: {:?} in {:?}",
        p.anchor,
        p.frame
    );
    assert!(p.frame.y0 >= 0.0 && p.frame.y1 <= 300.0 + 0.001, "{:?}", p.frame);
}
