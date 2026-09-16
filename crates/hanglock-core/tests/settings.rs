//! The rules that decide whether a user keeps their preferences across an upgrade. All of these
//! exist because the alternative is a support thread titled "it reset everything".

use hanglock_core::ids::{ClickThrough, PostureKind};
use hanglock_core::settings::{Settings, SCHEMA};

#[test]
fn an_empty_document_is_defaults_not_corruption() {
    let (s, _) = Settings::from_toml("");
    assert_eq!(s, Settings::default());
    assert_eq!(s.schema, SCHEMA);
}

#[test]
fn a_partial_document_keeps_everything_it_does_not_mention() {
    let (s, w) = Settings::from_toml("[overlay]\nhang = 230\n");
    assert_eq!(s.overlay.hang, 230.0);
    assert!(
        s.overlay.topmost,
        "unrelated default must survive a partial file"
    );
    assert_eq!(s.face.posture, PostureKind::Plate);
    assert!(
        w.iter().all(|x| !x.contains("ignored")),
        "a known key must not be reported unknown: {w:?}"
    );
}

/// The exact failure mode a synthesised `#[derive(Deserialize)]` would have produced here: one new
/// field in a new release, and every existing user's file loses the rest.
#[test]
fn an_old_document_loads_into_a_new_build() {
    let old = "schema = 1\n\n[overlay]\nhang = 190\ntopmost = false\n";
    let (s, _) = Settings::from_toml(old);
    assert_eq!(s.overlay.hang, 190.0);
    assert!(!s.overlay.topmost);
    assert_eq!(
        s.overlay.margin, 16.0,
        "a field the old file never had takes its default"
    );
}

#[test]
fn unknown_keys_are_ignored_and_reported_not_fatal() {
    let doc = "[overlay]\nhang = 150\nquantum_mode = true\n\n[frobnicate]\nsize = 9\n";
    let (s, w) = Settings::from_toml(doc);
    assert_eq!(s.overlay.hang, 150.0);
    assert!(w.iter().any(|x| x.contains("quantum_mode")));
    assert!(w.iter().any(|x| x.contains("frobnicate")));
}

#[test]
fn out_of_range_values_are_clamped_by_the_same_limits_the_ui_uses() {
    use hanglock_core::settings::limits;
    let doc = "[overlay]\nhang = 99999\nscale = 0.01\nopacity = -3\nanchor_ratio = 42\nmargin = 1e9\n[general]\nfps_cap = 5\n";
    let (s, _) = Settings::from_toml(doc);
    assert_eq!(s.overlay.hang, limits::HANG.1);
    assert_eq!(s.overlay.scale, limits::SCALE.0);
    assert_eq!(s.overlay.opacity, limits::OPACITY.0);
    assert_eq!(s.overlay.anchor_ratio, limits::ANCHOR_RATIO.1);
    assert_eq!(s.overlay.margin, limits::MARGIN.1);
    assert_eq!(s.general.fps_cap, limits::FPS_CAP.0);
}

#[test]
fn a_nonsense_value_keeps_the_default_and_says_so() {
    let (s, w) = Settings::from_toml("[face]\nposture = \"spinning\"\n[overlay]\nclick_through = \"maybe\"\nopacity = \"quite nice\"\n");
    assert_eq!(s.face.posture, PostureKind::Plate);
    assert_eq!(s.overlay.click_through, ClickThrough::Hover);
    assert_eq!(s.overlay.opacity, 1.0);
    assert_eq!(w.len(), 2, "both bad enum values reported: {w:?}");
}

#[test]
fn a_round_trip_loses_nothing() {
    let mut s = Settings::default();
    s.overlay.hang = 190.0;
    s.overlay.scale = 1.35;
    s.overlay.monitor_index = 2;
    s.overlay.click_through = ClickThrough::Always;
    s.face.seconds = true;
    s.face.hour12 = false;
    s.face.posture = PostureKind::Mounted;
    s.general.launch_at_login = true;
    s.general.fps_cap = 120;
    let text = s.to_toml();
    let (back, warnings) = Settings::from_toml(&text);
    assert!(
        warnings.is_empty(),
        "a file we wrote ourselves must not warn: {warnings:?}"
    );
    assert_eq!(s, back);
}

#[test]
fn a_comment_or_blank_line_is_not_an_error() {
    let doc = "# my clock\n\n[overlay]\n# how far down\nhang = 110 # trailing note\n";
    let (s, w) = Settings::from_toml(doc);
    assert_eq!(s.overlay.hang, 110.0);
    assert!(w.is_empty(), "{w:?}");
}

#[test]
fn writing_twice_produces_the_same_bytes() {
    let s = Settings::default();
    assert_eq!(
        s.to_toml(),
        s.to_toml(),
        "a settings diff in a bug report has to mean something"
    );
}

#[test]
fn posture_and_click_through_names_round_trip() {
    for p in [
        PostureKind::Natural,
        PostureKind::Plate,
        PostureKind::Mounted,
        PostureKind::Locked,
    ] {
        assert_eq!(PostureKind::parse(p.as_str()), Some(p));
    }
    for c in [ClickThrough::Hover, ClickThrough::Always] {
        assert_eq!(ClickThrough::parse(c.as_str()), Some(c));
    }
    assert_eq!(
        PostureKind::parse("Plate"),
        None,
        "keys are lower-case on purpose; a typo is reported"
    );
}

#[test]
fn sanitize_is_idempotent_and_total() {
    let mut s = Settings {
        schema: 99,
        ..Settings::default()
    };
    s.overlay.hang = f64::NAN;
    s.overlay.anchor_ratio = -1.0;
    s.sanitize();
    assert!(
        s.overlay.hang.is_finite(),
        "NaN must not survive the boundary"
    );
    assert_eq!(s.overlay.anchor_ratio, 0.0);
}
