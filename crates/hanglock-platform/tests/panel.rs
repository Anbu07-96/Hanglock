//! The settings form, without a window: what it offers, what it says, and what one click asks for.
//!
//! A settings window is the one surface where a row can be *drawn* correctly and wired wrongly, and
//! the failure is silent - a checkbox that ticks, and changes nothing. These tests read the form the
//! way a window does and answer each click the way the tray would, so the two of them have to agree
//! with the document.

use hanglock_core::ids::{ClickThrough, PostureKind};
use hanglock_core::placement::{Monitor, Rect};
use hanglock_core::settings::Settings;
use hanglock_platform::panel::{self, Control, RowId, Step};
use hanglock_platform::Command;

fn docs() -> Settings {
    Settings::default()
}

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

fn one() -> Vec<Monitor> {
    vec![display(0, 1920.0, 1080.0, 1.0)]
}

fn control(s: &Settings, id: RowId) -> Control {
    control_in(s, &one(), id)
}

fn control_in(s: &Settings, mons: &[Monitor], id: RowId) -> Control {
    for g in panel::form(s, mons) {
        for r in g.rows {
            if r.id == id {
                return r.control;
            }
        }
    }
    panic!("{id:?} is not in the form")
}

fn ids(s: &Settings) -> Vec<RowId> {
    panel::form(s, &one())
        .iter()
        .flat_map(|g| g.rows.iter().map(|r| r.id))
        .collect()
}

#[test]
fn the_window_offers_exactly_the_essential_choices() {
    let s = docs();
    let want = [
        RowId::LaunchAtLogin,
        RowId::Topmost,
        RowId::Monitor,
        RowId::Hour12,
        RowId::Seconds,
        RowId::Meridiem,
        RowId::ClockSize,
        RowId::HangLength,
        RowId::ClickThrough,
        RowId::Posture,
        RowId::AnchorAcross,
        RowId::AnchorDrop,
        RowId::ResetPosition,
    ];
    assert_eq!(
        ids(&s),
        want.to_vec(),
        "the form *is* the list of what a user can set"
    );
    let groups = panel::form(&s, &one());
    let titles: Vec<&str> = groups.iter().map(|g| g.title).collect();
    assert_eq!(titles, vec!["General", "Clock", "Behaviour"]);
    for g in &groups {
        for r in &g.rows {
            match &r.control {
                Control::Check { .. } | Control::Push => {}
                Control::Choice { options, .. } => {
                    assert!(options.len() > 1, "a choice with one option is a label");
                }
                Control::Nudge { text, .. } => {
                    assert!(!text.is_empty(), "{} reads as nothing", r.label);
                }
            }
        }
    }
}

/// Every row reflects the document it was built from - the property a window cannot check for itself,
/// because a window is drawn once and then remembers what it drew.
#[test]
fn every_row_reads_the_document_it_was_handed() {
    let mut s = docs();
    s.general.launch_at_login = true;
    s.overlay.topmost = false;
    s.face.hour12 = false;
    s.face.seconds = true;
    s.face.meridiem = false;
    s.overlay.click_through = ClickThrough::Solid;
    s.face.posture = PostureKind::Locked;
    s.overlay.anchor_ratio = 0.25;
    s.overlay.anchor_drop = 120.0;
    s.overlay.hang = 230.0;
    for (id, on) in [
        (RowId::LaunchAtLogin, true),
        (RowId::Topmost, false),
        (RowId::Hour12, false),
        (RowId::Seconds, true),
        (RowId::Meridiem, false),
    ] {
        assert_eq!(
            control(&s, id),
            Control::Check { on },
            "{id:?} misreads the document"
        );
    }
    let Control::Choice { options, selected } = control(&s, RowId::ClickThrough) else {
        panic!("the mouse row is a choice");
    };
    assert_eq!(options[selected], ClickThrough::Solid.label());
    let Control::Choice { options, selected } = control(&s, RowId::Posture) else {
        panic!("the posture row is a choice");
    };
    assert_eq!(options[selected], PostureKind::Locked.label());
    let Control::Nudge {
        text,
        can_down,
        can_up,
    } = control(&s, RowId::HangLength)
    else {
        panic!("the cord row is a nudge");
    };
    assert!(text.starts_with("230 px"), "{text}");
    assert!(can_down, "230 is not the shortest cord");
    assert!(can_up, "and not the longest");
    let Control::Nudge { text, can_up, .. } = control(&s, RowId::AnchorDrop) else {
        panic!("the drop row is a nudge");
    };
    assert_eq!(text, "120 px");
    assert!(can_up);
    let Control::Nudge { text, can_down, .. } = control(&s, RowId::AnchorAcross) else {
        panic!("the across row is a nudge");
    };
    assert_eq!(text, "25%");
    assert!(can_down);
}

#[test]
fn the_cord_row_stops_at_the_ends_of_its_ladder() {
    let mut s = docs();
    s.overlay.hang = panel::HANG_STEPS[0];
    let Control::Nudge {
        can_down, can_up, ..
    } = control(&s, RowId::HangLength)
    else {
        panic!("the cord row is a nudge");
    };
    assert!(!can_down, "the shortest cord has nothing shorter");
    assert!(can_up);
    s.overlay.hang = panel::HANG_STEPS[panel::HANG_STEPS.len() - 1];
    let Control::Nudge {
        can_down,
        can_up,
        text,
    } = control(&s, RowId::HangLength)
    else {
        panic!("the cord row is a nudge");
    };
    assert!(can_down);
    assert!(!can_up, "and nothing longer");
    let n = panel::HANG_STEPS.len();
    assert!(
        text.contains(&format!("{n} of {n}")),
        "{text} should say which rung it is"
    );
    // A hand-typed length lands between two rungs, and the readout says which pair the buttons move
    // between rather than inventing a value the ladder does not have.
    s.overlay.hang = 170.0;
    assert_eq!(panel::hang_index(170.0), 2);
    s.overlay.hang = 150.0;
    assert_eq!(
        panel::hang_index(150.0),
        2,
        "exactly on a rung reads as that rung"
    );
    s.overlay.hang = 151.0;
    assert_eq!(
        panel::hang_index(151.0),
        3,
        "and just above it reads as the next"
    );
}

#[test]
fn a_click_on_each_row_asks_for_the_tray_s_own_command() {
    let s = docs();
    let ask = |id: RowId, step: Step| {
        panel::change(&s, id, step).unwrap_or_else(|| panic!("{id:?} + {step:?} did nothing"))
    };
    assert_eq!(
        ask(RowId::LaunchAtLogin, Step::Toggle),
        Command::ToggleLaunchAtLogin
    );
    assert_eq!(ask(RowId::Topmost, Step::Toggle), Command::ToggleTopmost);
    assert_eq!(ask(RowId::Hour12, Step::Toggle), Command::Toggle12Hour);
    assert_eq!(ask(RowId::Seconds, Step::Toggle), Command::ToggleSeconds);
    assert_eq!(ask(RowId::Meridiem, Step::Toggle), Command::ToggleMeridiem);
    assert_eq!(ask(RowId::ClockSize, Step::Up), Command::Bigger);
    assert_eq!(ask(RowId::ClockSize, Step::Down), Command::Smaller);
    assert_eq!(ask(RowId::HangLength, Step::Up), Command::HangUp);
    assert_eq!(ask(RowId::HangLength, Step::Down), Command::HangDown);
    assert_eq!(
        ask(RowId::ResetPosition, Step::Press),
        Command::ResetPosition
    );
    assert_eq!(ask(RowId::Monitor, Step::Pick(1)), Command::SetMonitor(1));
    assert_eq!(
        ask(RowId::Posture, Step::Pick(0)),
        Command::SetPosture(PostureKind::Natural)
    );
    assert_eq!(
        ask(RowId::ClickThrough, Step::Pick(0)),
        Command::SetClickThrough(ClickThrough::Solid)
    );
    assert_eq!(
        ask(RowId::ClickThrough, Step::Pick(2)),
        Command::SetClickThrough(ClickThrough::Always)
    );
    let Command::SetAnchor(a) = ask(RowId::AnchorDrop, Step::Up) else {
        panic!("the drop row sets the anchor");
    };
    assert_eq!((a.ratio, a.drop), (0.5, 10.0));
    let Command::SetAnchor(a) = ask(RowId::AnchorAcross, Step::Down) else {
        panic!("the across row sets the anchor");
    };
    assert!(
        (a.ratio - 0.45).abs() < 1e-9 && a.drop == 0.0,
        "the pair came back wrong: {a:?}"
    );
}

/// An index outside the list, and a row already at its limit: `None`, not a command that changes
/// nothing and not a panic. A window races the user clicking a button it is about to disable.
#[test]
fn a_click_that_cannot_change_anything_asks_for_nothing() {
    let s = docs();
    assert_eq!(panel::change(&s, RowId::Posture, Step::Pick(99)), None);
    assert_eq!(
        panel::change(&s, RowId::ClickThrough, Step::Pick(7)),
        None,
        "and does not fall back to the first option"
    );
    assert_eq!(panel::change(&s, RowId::Monitor, Step::Toggle), None);
    assert_eq!(panel::change(&s, RowId::Seconds, Step::Up), None);
    assert_eq!(
        panel::change(&s, RowId::ResetPosition, Step::Toggle),
        Some(Command::ResetPosition),
        "a push row answers any click on it"
    );
    let at_the_top = docs();
    assert_eq!(
        panel::change(&at_the_top, RowId::AnchorDrop, Step::Down),
        None,
        "already as high as the display allows"
    );
}

#[test]
fn the_display_list_says_which_one_the_clock_is_on_and_nothing_else() {
    let mut s = docs();
    s.overlay.monitor_index = 1;
    let mons = vec![
        display(0, 1920.0, 1080.0, 1.0),
        display(1, 2560.0, 1440.0, 1.5),
    ];
    let Control::Choice { options, selected } = control_in(&s, &mons, RowId::Monitor) else {
        panic!("the display row is a choice");
    };
    assert_eq!(options.len(), 2);
    assert_eq!(selected, 1);
    assert!(options[1].contains("2560 x 1440"), "{}", options[1]);
    assert!(
        options[1].contains("150%"),
        "DPI belongs in the label: {}",
        options[1]
    );
    assert!(options[1].contains("<- here"), "{}", options[1]);
    assert!(!options[0].contains("here"), "{}", options[0]);
}

#[test]
fn one_monitor_is_still_a_row_and_not_a_lecture() {
    let s = docs();
    let mons = one();
    let Control::Choice { options, selected } = control_in(&s, &mons, RowId::Monitor) else {
        panic!("the display row is a choice");
    };
    assert_eq!(options.len(), 1, "one display, one option");
    assert_eq!(selected, 0);
    assert_eq!(
        panel::change(&s, RowId::Monitor, Step::Pick(0)),
        Some(Command::SetMonitor(0)),
        "and choosing it is still a valid question to ask"
    );
}
