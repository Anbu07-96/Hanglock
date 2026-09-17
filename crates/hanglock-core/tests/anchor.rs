//! What a drag along the top of the screen does to the two numbers in the settings file, and what
//! those numbers do to a window. The loop is the thing under test: `dragged` and `place` must agree
//! to the pixel, or a clock moves somewhere other than where the hand took it.

use hanglock_core::anchor::{self, Anchor};
use hanglock_core::placement::{place, swept_box, Monitor, Rect, ANCHOR_INSET};
use hanglock_core::rope::config::{CardSpec, RopeConfig};
use hanglock_core::settings::Settings;
use hanglock_core::vec2::Vec2;

fn cfg() -> (RopeConfig, CardSpec) {
    (RopeConfig::default(), CardSpec::default())
}

const HANG: f64 = 150.0;
const MARGIN: f64 = 16.0;

fn display(index: u32, w: f64, h: f64, scale: f64) -> Monitor {
    Monitor {
        index,
        bounds: Rect::new(0.0, 0.0, w * scale, h * scale),
        work: Rect::new(0.0, 0.0, w * scale, (h - 40.0) * scale),
        scale,
        taskbar_top: false,
        taskbar_auto_hidden: false,
        primary: index == 0,
    }
}

/// Where the hang point is on the screen, given a document: the device-px answer a drag starts from,
/// and the one a reviewer can check against their own window.
fn placed(m: &Monitor, s: &Settings) -> Vec2 {
    let (c, _) = cfg();
    let p = place(
        m,
        &c,
        &s.card(),
        s.overlay.hang,
        s.overlay.scale,
        s.overlay.anchor_ratio,
        s.overlay.anchor_drop,
        s.overlay.margin,
        true,
    );
    Vec2::new(p.frame.x0 + p.anchor.x, p.frame.y0 + p.anchor.y)
}

/// A document with the pair the drag produced, so the test goes through the same two numbers the
/// settings file holds rather than through a parallel structure.
fn with(s: &Settings, a: Anchor) -> Settings {
    let mut out = *s;
    out.overlay.anchor_ratio = a.ratio;
    out.overlay.anchor_drop = a.drop;
    out
}

fn doc(ratio: f64, drop: f64) -> Settings {
    let mut s = Settings::default();
    s.overlay.anchor_ratio = ratio;
    s.overlay.anchor_drop = drop;
    s
}

#[test]
fn a_drag_lands_exactly_where_the_cursor_went() {
    let m = display(0, 1920.0, 1080.0, 1.0);
    let (c, _) = cfg();
    let start = doc(0.5, 0.0);
    let from = placed(&m, &start);
    for dx in [-420.0, -97.0, 0.0, 133.0, 500.0] {
        let got = anchor::dragged(&m, &start, &c, from, Vec2::new(dx, 0.0));
        let at = placed(&m, &with(&start, got));
        // Not `1e-6`: the pair the file holds is quantised to four decimals (`Anchor::clamped`),
        // which across a 1920-wide display is a fifth of a pixel. The drag is exact in the units the
        // document can express, and a sub-pixel difference is what it costs to be readable.
        assert!(
            (at.x - (from.x + dx)).abs() < 0.2,
            "a drag of {dx} put the hang point at {} instead of {}",
            at.x,
            from.x + dx
        );
        assert!(
            (at.y - from.y).abs() < 1e-6,
            "a horizontal drag moved the clock vertically: {at:?}"
        );
        assert!(got.ratio > 0.0 && got.ratio < 1.0, "and the pair stayed in the file: {got:?}");
    }
}

/// A drag measured from the *stored* pair would sit still for the first 14 px at the top of a
/// display, because `place` keeps that much room for the clamp drawn at the anchor. Measuring from
/// where the clock is avoids the dead band, and this pins the difference.
#[test]
fn pulling_up_to_the_edge_holds_there_instead_of_jumping() {
    let m = display(0, 1920.0, 1080.0, 1.0);
    let (c, _) = cfg();
    let start = doc(0.5, 0.0);
    let from = placed(&m, &start);
    assert!(
        (from.y - ANCHOR_INSET).abs() < 1e-9,
        "a first run hangs from the inset, not from the edge: {from:?}"
    );
    let got = anchor::dragged(&m, &start, &c, from, Vec2::new(0.0, -900.0));
    assert!(
        got.drop < 1e-9,
        "there is nothing above the hang line to store, got {}",
        got.drop
    );
    let at = placed(&m, &with(&start, got));
    assert!(
        (at.y - from.y).abs() < 1e-9,
        "the clock jumped when the drag could not move it: {at:?} vs {from:?}"
    );
}

#[test]
fn a_drag_down_stops_where_the_swing_would_leave_the_display() {
    let m = display(0, 1920.0, 1080.0, 1.0);
    let (c, card) = cfg();
    let box_logical = swept_box(&c, &card, HANG, MARGIN, 1.0);
    let start = doc(0.5, 0.0);
    let from = placed(&m, &start);
    let got = anchor::dragged(&m, &start, &c, from, Vec2::new(0.0, 4000.0));
    let at = placed(&m, &with(&start, got));
    let bottom = at.y + box_logical.y1;
    assert!(
        (bottom - 1080.0).abs() < 1e-3,
        "the swing's bottom edge is at {bottom}, not at the display's"
    );
    assert!(got.drop > 100.0, "and the drop is a real distance: {}", got.drop);
}

#[test]
fn a_drag_across_the_width_saturates_at_its_ends() {
    let m = display(0, 1920.0, 1080.0, 1.0);
    let (c, _) = cfg();
    let start = doc(0.5, 0.0);
    let from = placed(&m, &start);
    let left = anchor::dragged(&m, &start, &c, from, Vec2::new(-8000.0, 0.0));
    let right = anchor::dragged(&m, &start, &c, from, Vec2::new(8000.0, 0.0));
    assert_eq!(left.ratio, 0.0, "the usable width has no left of it");
    assert_eq!(right.ratio, 1.0, "nor right of it");
    // Both still put the clock somewhere on the display, because the frame is clamped and the anchor
    // is derived from the frame rather than the other way round.
    for a in [left, right] {
        let at = placed(&m, &with(&start, a));
        assert!(
            at.x > 0.0 && at.x < 1920.0,
            "hang point at {at:?} is off the display"
        );
    }
}

/// 100 % and 200 % have to mean the same gesture. The device-pixel cursor delta is divided by the
/// scale once, and the pair that comes back has to reproduce that delta after `place` has multiplied
/// it again — the round trip is where a mixed-up coordinate space shows up as a clock that lags the
/// hand or runs ahead of it.
#[test]
fn the_same_drag_means_the_same_distance_at_any_scale() {
    let (c, _) = cfg();
    for scale in [1.0, 1.25, 1.5, 2.0, 3.0] {
        let m = display(0, 1920.0, 1080.0, scale);
        let start = doc(0.5, 0.0);
        let from = placed(&m, &start);
        let delta = Vec2::new(120.0, 64.0);
        let got = anchor::dragged(&m, &start, &c, from, delta);
        let at = placed(&m, &with(&start, got));
        assert!(
            (at.x - (from.x + delta.x)).abs() < 0.2
                && (at.y - (from.y + delta.y)).abs() < 1e-3,
            "at {scale}x a drag of {delta:?} moved the clock to {at:?} from {from:?}"
        );
        // The drop is stored in logical px, so a bigger scale stores a smaller number for the same
        // gesture — and the pair still fits the settings document's range.
        assert!(
            got.drop > 0.0 && got.drop < 4096.0,
            "drop {} left the document's range at {scale}x",
            got.drop
        );
    }
}

#[test]
fn a_pair_outside_the_document_is_pulled_back_rather_than_refused() {
    let s = Settings::default();
    assert_eq!(Anchor::from_overlay(&s.overlay), Anchor::DEFAULT);
    let out = Anchor::clamped(4.0, -20.0);
    assert_eq!(out.ratio, 1.0, "the ratio has two ends and no more");
    assert_eq!(out.drop, 0.0, "and nothing above the hang line is recorded");
    let far = Anchor::clamped(0.5, 1e9);
    assert_eq!(far.drop, 4096.0, "the reach the file documents is the reach the maths accepts");
}

/// The frame is clamped into the display before the anchor is, so near an edge the clock stops
/// following the cursor. The *pair* still says where the cursor went, which is what makes the next
/// drag work from where the object is rather than from a number the window refused to honour.
#[test]
fn a_drag_into_the_edge_self_corrects_on_the_way_back() {
    let m = display(0, 1920.0, 1080.0, 1.0);
    let (c, _) = cfg();
    let start = doc(0.5, 0.0);
    let from = placed(&m, &start);
    let right = anchor::dragged(&m, &start, &c, from, Vec2::new(1500.0, 0.0));
    let stuck = placed(&m, &with(&start, right));
    assert!(
        stuck.x < from.x + 1500.0 - 1.0,
        "the frame clamp is what this test is about, and it did not bind: {stuck:?}"
    );
    let back = anchor::dragged(&m, &with(&start, right), &c, stuck, Vec2::new(-100.0, 0.0));
    let at = placed(&m, &with(&start, back));
    assert!(
        (at.x - (stuck.x - 100.0)).abs() < 0.2,
        "coming back started from the number, not from the clock: {at:?} vs {:?}",
        stuck.x - 100.0
    );
}
