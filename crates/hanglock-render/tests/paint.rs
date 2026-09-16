//! The painter's contract. Not pixel-exact golden images — this is a deterministic software
//! compositor, so what is worth asserting are the properties that the window layer depends on, and
//! the ones a future "nice" change could quietly break.

use hanglock_core::clock::format::{format, Civil, FaceOptions, FaceText};
use hanglock_core::placement::Rect;
use hanglock_core::rope::config::{CardSpec, Posture, RopeConfig};
use hanglock_core::rope::Rope;
use hanglock_core::scene::Scene;
use hanglock_core::vec2::Vec2;
use hanglock_render::{paint, Canvas, Theme};

fn scene_at(theta: f64, centre: Vec2, text: FaceText) -> Scene {
    let mut rope = Rope::new(
        RopeConfig::default(),
        CardSpec::default(),
        Posture::PLATE,
        Vec2::new(280.0, 14.0),
        1.0,
    );
    rope.host = Some(Rect::new(0.0, 0.0, 700.0, 400.0));
    for _ in 0..200 {
        rope.step(1.0 / 60.0);
    }
    let mut s = Scene::new(&rope, &rope.cfg, text, 1.0);
    // Overwrite the pose rather than driving the solver: these tests are about drawing.
    s.card_centre = centre;
    s.theta = theta;
    s
}

fn face(s: &str) -> FaceText {
    let c = Civil {
        year: 2026,
        month: 9,
        day: 15,
        hour: 22,
        minute: 42,
        second: 7,
    };
    let _ = s;
    format(
        &c,
        &FaceOptions {
            hour12: true,
            seconds: false,
            meridiem: true,
        },
    )
}

#[test]
fn every_untouched_pixel_is_exactly_transparent() {
    // This one is not cosmetic. On a layered window, hit testing follows alpha: a pixel with alpha 1
    // out of 255 is *opaque to the mouse*, and a corner that should pass a click to the desktop
    // instead eats it. So the four corners must be zero, not near-zero.
    let (w, h) = (700u32, 400u32);
    let mut cv = Canvas::new(w, h);
    let sc = scene_at(0.0, Vec2::new(350.0, 180.0), face("10:42"));
    paint(&sc, &mut cv, &Theme::default());
    for (x, y) in [
        (0u32, 0u32),
        (w - 1, 0),
        (0, h - 1),
        (w - 1, h - 1),
        (5, 380),
        (690, 300),
    ] {
        assert_eq!(
            cv.alpha_at(x, y),
            0,
            "pixel ({x},{y}) must be fully transparent"
        );
    }
}

#[test]
fn the_plate_is_opaque_where_it_is_drawn() {
    let mut cv = Canvas::new(700, 400);
    let sc = scene_at(0.0, Vec2::new(350.0, 180.0), face("10:42"));
    paint(&sc, &mut cv, &Theme::default());
    let a = cv.alpha_at(350, 180);
    assert!(a > 200, "plate centre alpha {a}, expected an opaque plate");
    // The shadow band below the plate must be *partially* there, or the object is floating, not hung.
    let below = cv.alpha_at(350, (180.0 + sc.card_h * 0.5 + 6.0) as u32);
    assert!(
        below > 0 && below < a,
        "no soft shadow below the plate ({below} vs {a})"
    );
}

#[test]
fn a_tilted_plate_still_covers_its_own_centre() {
    for th in [-0.35, -0.1, 0.0, 0.1, 0.35] {
        let mut cv = Canvas::new(700, 400);
        let sc = scene_at(th, Vec2::new(350.0, 200.0), face("09:07"));
        paint(&sc, &mut cv, &Theme::default());
        assert!(
            cv.alpha_at(350, 200) > 150,
            "rotation {th} left a hole in the plate"
        );
    }
}

#[test]
fn the_drawn_cord_stops_where_the_plate_covers_it() {
    let mut cv = Canvas::new(700, 400);
    let sc = scene_at(0.0, Vec2::new(350.0, 180.0), face("10:42"));
    // The last node is the centre of mass, inside the plate; `drawn_cord_len` must trim it away, or
    // the cord would be drawn on top of the object holding it.
    assert!(
        sc.drawn_cord_len() + 1 < sc.node_count as usize,
        "cord was not trimmed at the plate"
    );
    paint(&sc, &mut cv, &Theme::default());
    assert!(
        !cv.px.iter().all(|&b| b == 0),
        "an all-black frame is not a painted frame"
    );
}

#[test]
fn painting_is_deterministic() {
    let sc = scene_at(0.12, Vec2::new(360.0, 190.0), face("10:42"));
    let mut a = Canvas::new(700, 400);
    let mut b = Canvas::new(700, 400);
    paint(&sc, &mut a, &Theme::default());
    paint(&sc, &mut b, &Theme::default());
    assert_eq!(
        a.px, b.px,
        "two paints of one scene must agree, or the golden images mean nothing"
    );
}

#[test]
fn the_present_rect_is_tighter_than_the_window() {
    let mut cv = Canvas::new(700, 400);
    let sc = scene_at(0.0, Vec2::new(350.0, 180.0), face("10:42"));
    paint(&sc, &mut cv, &Theme::default());
    let r = cv.present_rect().expect("a painted frame has a dirty rect");
    assert!(r.x0 >= 0.0 && r.y0 >= 0.0 && r.x1 <= 700.0 && r.y1 <= 400.0);
    let area = (r.x1 - r.x0) * (r.y1 - r.y0);
    assert!(area < 700.0 * 400.0, "the whole window was dirtied: {area}");
}

#[test]
fn a_glyph_is_wider_than_nothing_and_the_face_has_every_digit() {
    use hanglock_render::face_data::glyph;
    for ch in "0123456789:APM".chars() {
        let g = glyph(ch).unwrap_or_else(|| panic!("face is missing {ch}"));
        assert!(
            !g.lines.is_empty() || !g.dots.is_empty(),
            "{ch} draws nothing"
        );
    }
    assert!(
        glyph('\u{1F600}').is_none(),
        "an unplanned character must be skipped, not drawn as a box"
    );
}
