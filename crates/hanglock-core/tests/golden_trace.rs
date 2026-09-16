//! Locks the solver to `tools/model/hanglock_ref.py`, the reference model the physics was designed
//! in. The reference emits one line per frame with the node positions, the plate attitude, the sleep
//! flag and the worst stretch; this test replays the identical input and compares.
//!
//! Why a golden file rather than only property tests: the *feel* of a hanging object lives in
//! hundreds of small numbers, and property tests cannot notice a change that makes the swing subtly
//! wrong. They can also be satisfied by a solver that no longer matches the reviewed behaviour at
//! all. If someone rewrites the integrator and this fails, the question is not "is the tolerance too
//! tight" — it is "did you re-review the feel".
//!
//! `1e-6` px is the tolerance, not zero, because the reference computes `math.hypot` while the
//! hot path here uses `sqrt(x*x + y*y)`. That is a last-bit difference in one distance per link,
//! amplified a little by the recursion; anything tighter would be testing the C library.

use hanglock_core::placement::Rect;
use hanglock_core::rope::config::{CardSpec, Posture, RopeConfig};
use hanglock_core::rope::Rope;
use hanglock_core::vec2::Vec2;

/// A tenth of a thousandth of a pixel. Tight enough that changing `damping` by one part in 10 000
/// fails the test, loose enough that it is not measuring the C library's `hypot` against our own
/// `sqrt`, and 10 000x below anything a person could see.
const TOL: f64 = 1e-4;

#[test]
fn solver_matches_the_reference_model() {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/golden/trace_drag_settle.txt"))
        .expect("run `python3 tools/model/hanglock_ref.py trace tests/golden/trace_drag_settle.json` to regenerate");
    let mut lines = text.lines().filter(|l| !l.trim().is_empty()).peekable();
    let mut expect_prefix = |want: &str| -> Vec<String> {
        let l = lines.next().expect("unexpected end of trace");
        assert!(l.starts_with(want), "expected {want}, got {l}");
        l.split_whitespace().skip(1).map(|s| s.to_string()).collect()
    };
    let seg = expect_prefix("segments")[0].parse::<usize>().unwrap();
    let anchor: Vec<f64> = expect_prefix("anchor").iter().map(|s| s.parse().unwrap()).collect();
    let hang: f64 = expect_prefix("hang")[0].parse().unwrap();

    let mut rope = Rope::new(RopeConfig::default(), CardSpec::default(), Posture::PLATE, Vec2::new(anchor[0], anchor[1]), 1.0);
    rope.host = Some(Rect::new(0.0, 0.0, 560.0, 300.0));
    assert_eq!(rope.cfg.segments, seg, "segment count drifted from the reference");
    assert!((rope.hang - hang).abs() < TOL, "hang length drifted: {} vs {hang}", rope.hang);

    let frames: usize = expect_prefix("frames")[0].parse().unwrap();
    let mut worst = 0.0_f64;
    for i in 0..frames {
        // The recorded input, reproduced rather than parsed: drag right by 7 px a frame for twenty
        // frames, release, then let it be. Same script, two implementations.
        if i == 0 {
            let c = rope.card_centre();
            assert!(rope.begin_drag(c, 10.0));
        }
        if i < 20 {
            rope.move_drag(Vec2::new(rope.anchor.x + 7.0 * i as f64, rope.anchor.y + 140.0), Vec2::new(420.0, 0.0));
        } else if i == 20 {
            rope.end_drag();
        }
        rope.step(1.0 / 60.0);

        let line = lines.next().expect("trace shorter than its header promised");
        let toks: Vec<&str> = line.split_whitespace().collect();
        assert_eq!(toks[0], "F");
        assert_eq!(toks[1].parse::<usize>().unwrap(), i);
        for n in 0..seg + 1 {
            let x: f64 = toks[2 + n * 2].parse().unwrap();
            let y: f64 = toks[3 + n * 2].parse().unwrap();
            let d = rope.nodes[n].pos.dist(Vec2::new(x, y));
            assert!(d < TOL, "frame {i} node {n}: {d} px from the reference");
            worst = worst.max(d);
        }
        let th: f64 = toks[2 + (seg + 1) * 2].parse().unwrap();
        assert!((rope.att.theta - th).abs() < 1e-7, "frame {i}: attitude diverged");
        let asleep = toks[4 + (seg + 1) * 2] == "1";
        assert_eq!(rope.sleeping, asleep, "frame {i}: sleep state diverged (the idle contract)");
        let stretch: f64 = toks[5 + (seg + 1) * 2].parse().unwrap();
        assert!((rope.max_stretch() - stretch).abs() < 1e-7, "frame {i}: stretch diverged");
    }
    assert!(worst < TOL, "worst per-node drift was {worst}");
    let _ = lines;
}
