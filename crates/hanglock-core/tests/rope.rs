//! The solver's contract, as numbers.
//!
//! "Swings naturally" is otherwise only checkable by eye, and the eye is not in CI. Every test here
//! is an assertion the reference project's docs made *about* behaviour and proved *with* a test — and
//! three of them describe bugs that test caught before a person would have: dragging past reach
//! tearing the links, a projection that oscillated instead of converging, and a sleep rule that
//! never fired.

use hanglock_core::placement::Rect;
use hanglock_core::rope::config::{CardSpec, Posture, RopeConfig};
use hanglock_core::rope::Rope;
use hanglock_core::vec2::Vec2;

fn rope() -> Rope {
    let mut r = Rope::new(
        RopeConfig::default(),
        CardSpec::default(),
        Posture::PLATE,
        Vec2::new(280.0, 7.0),
        1.0,
    );
    r.host = Some(Rect::new(0.0, 0.0, 560.0, 300.0));
    r
}

/// Two 120 Hz frames must equal one 60 Hz frame. This is what the fixed timestep is *for*: a display
/// the user changes, a laptop that drops to 60 Hz to save power, and a 144 Hz monitor must all
/// produce the same swing, not three of them.
#[test]
fn behaviour_is_identical_at_60_and_120_hz() {
    let mut a = rope();
    let mut b = rope();
    for _ in 0..600 {
        a.step(1.0 / 60.0);
        b.step(1.0 / 120.0);
        b.step(1.0 / 120.0);
    }
    for i in 0..a.nodes.len() {
        let d = a.nodes[i].pos.dist(b.nodes[i].pos);
        assert!(
            d < 1e-6,
            "node {i} diverged by {d} px between refresh rates"
        );
        assert!(a.nodes[i].pos.x.is_finite() && a.nodes[i].pos.y.is_finite());
    }
}

/// Free swinging stays inside the ceiling. 1.02 of a ~9.4 px link is 0.19 px: below the threshold at
/// which a line stops looking like the same line.
#[test]
fn the_cord_never_stretches_under_free_swing() {
    let mut r = rope();
    let mut worst = 1.0_f64;
    for _ in 0..1200 {
        r.step(1.0 / 60.0);
        worst = worst.max(r.max_stretch());
    }
    assert!(
        worst <= 1.02,
        "free swing stretched the cord to {worst}x rest length"
    );
}

/// A flick that reverses direction and is yanked past the cord's reach: the input the solver must not
/// be able to break on. The reach clamp is what keeps this bounded; without it the same input leaves
/// both ends further apart than the cord can span and the links have nowhere to go but longer.
#[test]
fn a_hard_flick_stays_within_tolerance() {
    let mut r = rope();
    let c = r.card_centre();
    assert!(
        r.begin_drag(c, 10.0),
        "press on the plate must start a drag"
    );
    let mut worst = 1.0_f64;
    for i in 0..120 {
        let x = r.anchor.x + 240.0 * (i as f64 / 4.0).sin();
        r.move_drag(Vec2::new(x, r.anchor.y + 150.0), Vec2::new(3600.0, 0.0));
        r.step(1.0 / 60.0);
        worst = worst.max(r.max_stretch());
    }
    r.end_drag();
    for _ in 0..60 {
        r.step(1.0 / 60.0);
        worst = worst.max(r.max_stretch());
    }
    // 1.03 rather than 1.02: while held, the terminal node is immovable, so the projection can only
    // shorten a link by moving the *other* end, and one frame of a 3600 px/s reversal is more than the
    // passes available can absorb. Measured on the reference model: 1.0248.
    assert!(
        worst <= 1.03,
        "drag+flick stretched the cord to {worst}x rest length"
    );
}

/// Synthetic, superhuman input must stay bounded and recover. No pointer can produce this; the
/// assertion is that the solver cannot diverge even when something does.
#[test]
fn torture_input_stays_bounded_and_recovers() {
    let mut r = rope();
    let c = r.card_centre();
    r.begin_drag(c, 10.0);
    let mut worst = 1.0_f64;
    for i in 0..120 {
        let x = r.anchor.x + 900.0 * (i as f64 / 2.5).sin();
        r.move_drag(Vec2::new(x, r.anchor.y - 400.0), Vec2::new(900.0, -3000.0));
        r.step(1.0 / 60.0);
        worst = worst.max(r.max_stretch());
    }
    r.end_drag();
    for _ in 0..600 {
        r.step(1.0 / 60.0);
    }
    for n in &r.nodes {
        assert!(
            n.pos.x.is_finite() && n.pos.y.is_finite(),
            "NaN after torture input"
        );
    }
    assert!(
        worst < 1.5,
        "cord stretched to {worst}x under torture: no bounded recovery"
    );
    // And it settles: the brake and the friction must still win afterwards.
    assert!(r.sleeping, "solver never slept after a violent input");
}

#[test]
fn the_anchor_is_pinned_and_heavier_at_the_end() {
    let r = rope();
    assert_eq!(r.nodes[0].inv_mass, 0.0, "anchor must be immovable");
    assert!(
        r.nodes[r.nodes.len() - 1].inv_mass < 1.0,
        "the plate must be heavier than a cord node"
    );
    assert_eq!(r.nodes.len(), RopeConfig::default().segments + 1);
}

/// Releasing mid-motion must hand the pointer's velocity to the object. Measured against an undamped
/// pendulum: a 425 px/s throw should rise through ~83° of arc, and the cord's own stored energy
/// makes a little more possible, not much less.
#[test]
fn a_release_keeps_its_momentum() {
    let mut r = rope();
    r.reset(-0.62);
    let c = r.card_centre();
    assert!(r.begin_drag(c, 10.0));
    let mut prev = c;
    for i in 0..26 {
        let ang = -0.62 + 0.048 * i as f64;
        let reach = r.hang * r.cfg.reach_ratio;
        let next = Vec2::new(
            r.anchor.x + ang.sin() * reach,
            r.anchor.y + ang.cos() * reach,
        );
        let vel = Vec2::new((next.x - prev.x) * 60.0, (next.y - prev.y) * 60.0);
        r.move_drag(next, vel);
        prev = next;
        r.step(1.0 / 60.0);
    }
    let start = r.card_centre().x - r.anchor.x;
    r.end_drag();
    let mut extreme = start;
    for _ in 0..600 {
        r.step(1.0 / 60.0);
        let d = r.card_centre().x - r.anchor.x;
        if d > extreme {
            extreme = d;
        }
    }
    let carried = extreme - start;
    assert!(
        carried > 60.0,
        "throw carried only {carried:.1} px sideways: momentum was lost"
    );
    assert!(
        extreme.abs() <= r.hang * (r.cfg.sweep_deg.to_radians().sin()) + 2.0,
        "swing left the sector: {extreme}"
    );
}

#[test]
fn drag_tracking_is_exact_at_human_speeds() {
    let mut r = rope();
    let c = r.card_centre();
    r.begin_drag(c, 10.0);
    for i in 0..40 {
        // 5 000 px/s, which is faster than any mouse a person can move.
        let target = Vec2::new(r.anchor.x + 1.4 * i as f64, r.anchor.y + 140.0);
        r.move_drag(target, Vec2::new(5000.0, 0.0));
        r.step(1.0 / 60.0);
        let err = r.card_centre().dist(r.reachable(target));
        assert!(
            err < 1e-6,
            "held node lagged the cursor by {err} px at a human speed"
        );
    }
}

#[test]
fn the_drag_target_cannot_leave_the_reachable_sector() {
    let r = rope();
    let beyond = Vec2::new(r.anchor.x + 10_000.0, r.anchor.y - 10_000.0);
    let clamped = r.reachable(beyond);
    let reach = r.hang * r.cfg.reach_ratio;
    let d = clamped.dist(r.anchor);
    assert!(
        d <= reach + 1e-6,
        "clamped target at {d} px, beyond reach {reach}"
    );
    // Above the anchor is not a place a cord can put the plate.
    let up = r.reachable(Vec2::new(r.anchor.x, r.anchor.y - 500.0));
    assert!(up.y > r.anchor.y, "target went above the anchor: {up:?}");
}

#[test]
fn a_settled_rope_sleeps_and_a_touched_one_wakes() {
    let mut r = rope();
    for _ in 0..1200 {
        r.step(1.0 / 60.0);
        if r.sleeping {
            break;
        }
    }
    assert!(r.sleeping, "rope never slept");
    let before = r.card_centre();
    // Asleep means *asleep*: step() must not move anything, which is the whole idle-cost claim.
    for _ in 0..600 {
        r.step(1.0 / 60.0);
    }
    assert_eq!(r.card_centre(), before, "a sleeping rope kept moving");
    r.wake();
    assert!(!r.sleeping);
    r.step(1.0 / 60.0);
    assert!(
        r.card_centre() != before,
        "a woken rope must resume physics"
    );
}

/// Sleep must land the object hanging straight, not frozen mid-swing. Without the brake the friction
/// model parks the plate a few degrees off vertical, which reads as a broken clock.
#[test]
fn it_sleeps_hanging_straight() {
    let mut r = rope();
    for _ in 0..1200 {
        r.step(1.0 / 60.0);
        if r.sleeping {
            break;
        }
    }
    assert!(r.sleeping);
    let off = (r.card_centre().x - r.anchor.x).abs();
    assert!(off < 0.25, "settled {off:.2} px off vertical");
    assert!(
        r.att.theta.abs() < 5e-3,
        "settled tilted {} rad",
        r.att.theta
    );
}

#[test]
fn a_stall_cannot_burst_into_catchup_steps() {
    let mut r = rope();
    let a = r.card_centre();
    // Sixty seconds of "time" in one call: the accumulator clamp must turn that into max_frame of
    // work, not 14 400 fixed steps.
    r.step(60.0);
    let b = r.card_centre();
    let moved = b.dist(a);
    assert!(
        moved < 12.0,
        "a 60 s stall advanced the rope {moved} px: accumulator is unclamped"
    );
}

#[test]
fn posture_clamps_tilt_and_lock_holds_it_level() {
    for (posture, limit) in [
        (Posture::NATURAL, 0.46),
        (Posture::PLATE, 0.16),
        (Posture::MOUNTED, 0.05),
        (Posture::LOCKED, 1e-9),
    ] {
        let mut r = Rope::new(
            RopeConfig::default(),
            CardSpec::default(),
            posture,
            Vec2::new(280.0, 7.0),
            1.0,
        );
        r.host = Some(Rect::new(0.0, 0.0, 560.0, 300.0));
        let c = r.card_centre();
        r.begin_drag(c, 10.0);
        let mut peak = 0.0_f64;
        for i in 0..80 {
            let x = r.anchor.x + 130.0 * (i as f64 / 6.0).sin();
            r.move_drag(Vec2::new(x, r.anchor.y + 130.0), Vec2::new(2000.0, 0.0));
            r.step(1.0 / 60.0);
            peak = peak.max(r.att.theta.abs());
        }
        assert!(
            peak <= limit + 1e-6,
            "{} tilted {peak} rad past its limit",
            if limit < 1e-3 { "locked" } else { "posture" }
        );
    }
}

#[test]
fn refitting_preserves_motion_instead_of_resetting() {
    let mut r = rope();
    for _ in 0..30 {
        r.step(1.0 / 60.0);
    }
    let speed_before = r.nodes[r.nodes.len() - 1].displacement().len();
    r.refit(1.5, 190.0);
    let speed_after = r.nodes[r.nodes.len() - 1].displacement().len();
    assert!(speed_after > 1e-6, "a resize discarded the rope's motion");
    assert!(!r.sleeping, "a resize must wake the solver");
    let _ = speed_before;
}
