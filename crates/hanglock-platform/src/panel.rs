//! The settings form: which controls a settings window contains, what they say, and what a click
//! asks the app to do.
//!
//! This lives in the seam crate rather than in the Windows layer on purpose. A settings window is a
//! list of rows, and a second surface that rebuilt that list itself would drift from the first inside
//! one release — a checkbox the menu does not offer, a label worded differently in each. So the
//! *shape* of the form is data derived from [`Settings`] here, and all a window adds is how to draw
//! one [`Control`].
//!
//! [`change`] is the other half: it turns a click into a [`Command`], the same commands the tray
//! sends. There is no draft state and no Apply button, because every answer is an immediate, already
//! range-checked request that the model would have accepted from the menu. A window that could put the
//! document in a state the rest of the app has to special-case is the thing this module exists to
//! prevent.

use hanglock_core::anchor::Anchor;
use hanglock_core::ids::{ClickThrough, PostureKind};
use hanglock_core::placement::Monitor;
use hanglock_core::settings::{limits, Settings};

use crate::Command;

/// One control in the window, in window order. Carried on every row so a window can map a click back
/// to a setting without keeping its own table of what row three means — that table is this list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowId {
    LaunchAtLogin,
    Topmost,
    Monitor,
    Hour12,
    Seconds,
    Meridiem,
    ClockSize,
    HangLength,
    ClickThrough,
    Posture,
    AnchorAcross,
    AnchorDrop,
    ResetPosition,
}

/// How a row was answered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// A checkbox was flipped.
    Toggle,
    /// Option `n` of a list was chosen.
    Pick(usize),
    /// The `-` button of a nudge row.
    Down,
    /// The `+` button of a nudge row.
    Up,
    /// A row that only does something.
    Press,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Control {
    Check {
        on: bool,
    },
    Choice {
        options: Vec<String>,
        /// Which option the document currently holds. A `Choice` always lists the whole set, so a
        /// setting can never be reachable only by hand-editing the file.
        selected: usize,
    },
    Nudge {
        text: String,
        /// Whether each button would change anything, so a window can grey it out instead of letting
        /// a click land on a value already at its limit.
        can_down: bool,
        can_up: bool,
    },
    /// A button whose words are the row's label.
    Push,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub id: RowId,
    pub label: &'static str,
    pub control: Control,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Group {
    pub title: &'static str,
    pub rows: Vec<Row>,
}

/// The cord lengths the tray and the window step through, short to long — one table so the two
/// surfaces cannot disagree about what "hang longer" means. The ends are the settings file's own
/// limits, which is what makes the last step a real stop rather than a silent clamp.
pub const HANG_STEPS: [f64; 6] = [limits::HANG.0, 110.0, 150.0, 190.0, 230.0, limits::HANG.1];

/// One step of one nudge row, in that row's units.
const RATIO_STEP: f64 = 0.05;
const DROP_STEP: f64 = 10.0;

/// The window's contents, derived from the document and from the displays that exist right now.
///
/// Three groups, three builders, and the grouping is the *point* rather than a layout detail:
/// "General" is what the app does to the machine, "Clock" is what the digits say, "Behaviour" is
/// how the window and the mouse react. A row belongs to exactly one of them, and a row that appears
/// in two would be two settings with one name.
#[must_use]
pub fn form(s: &Settings, monitors: &[Monitor]) -> Vec<Group> {
    vec![
        general_group(s, monitors),
        clock_group(s),
        behaviour_group(s),
    ]
}

/// What the app does to the machine, and where on the machine it hangs.
fn general_group(s: &Settings, monitors: &[Monitor]) -> Group {
    let o = &s.overlay;
    Group {
        title: "General",
        rows: vec![
            Row {
                id: RowId::LaunchAtLogin,
                label: "Start with Windows",
                control: Control::Check {
                    on: s.general.launch_at_login,
                },
            },
            Row {
                id: RowId::Topmost,
                label: "Keep it above other windows",
                control: Control::Check { on: o.topmost },
            },
            Row {
                id: RowId::Monitor,
                label: "Hang it from this display",
                control: Control::Choice {
                    options: monitor_options(monitors, o.monitor_index),
                    selected: index_of_monitor(monitors, o.monitor_index),
                },
            },
        ],
    }
}

/// The digits, and how big a plate they sit on.
fn clock_group(s: &Settings) -> Group {
    let o = &s.overlay;
    let f = &s.face;
    Group {
        title: "Clock",
        rows: vec![
            Row {
                id: RowId::Hour12,
                label: "12-hour clock",
                control: Control::Check { on: f.hour12 },
            },
            Row {
                id: RowId::Seconds,
                label: "Show seconds",
                control: Control::Check { on: f.seconds },
            },
            Row {
                id: RowId::Meridiem,
                label: "Show AM / PM",
                control: Control::Check { on: f.meridiem },
            },
            Row {
                id: RowId::ClockSize,
                label: "Size",
                control: Control::Nudge {
                    text: percent(o.scale),
                    can_down: o.scale > limits::SCALE.0 + 0.001,
                    can_up: o.scale < limits::SCALE.1 - 0.001,
                },
            },
            Row {
                id: RowId::HangLength,
                label: "Cord",
                control: Control::Nudge {
                    text: cord_text(o.hang),
                    can_down: hang_index(o.hang) > 0,
                    can_up: hang_below_top(o.hang),
                },
            },
        ],
    }
}

/// The mouse, the swing, and where the clock is allowed to be.
fn behaviour_group(s: &Settings) -> Group {
    let o = &s.overlay;
    let f = &s.face;
    Group {
        title: "Behaviour",
        rows: vec![
            Row {
                id: RowId::ClickThrough,
                label: "Mouse",
                control: Control::Choice {
                    options: mouse_options(),
                    selected: index_of_click(o.click_through),
                },
            },
            Row {
                id: RowId::Posture,
                label: "How it swings",
                control: Control::Choice {
                    options: posture_options(),
                    selected: index_of_posture(f.posture),
                },
            },
            Row {
                id: RowId::AnchorAcross,
                label: "Across the display",
                control: Control::Nudge {
                    text: percent(o.anchor_ratio),
                    can_down: o.anchor_ratio > limits::ANCHOR_RATIO.0 + 0.001,
                    can_up: o.anchor_ratio < limits::ANCHOR_RATIO.1 - 0.001,
                },
            },
            Row {
                id: RowId::AnchorDrop,
                label: "Below the top edge",
                control: Control::Nudge {
                    text: drops_text(o.anchor_drop),
                    can_down: o.anchor_drop > limits::ANCHOR_DROP.0 + 0.001,
                    can_up: o.anchor_drop < limits::ANCHOR_DROP.1 - 0.001,
                },
            },
            Row {
                id: RowId::ResetPosition,
                label: "Put the clock back at the top centre",
                control: Control::Push,
            },
        ],
    }
}

/// Where `hang` sits in [`HANG_STEPS`].
///
/// A value a user typed by hand is not on the ladder, so this answers with the rung the next step
/// would move *from* — the last one at or below it — which is what makes the two buttons agree with
/// each other instead of both pushing towards the middle.
#[must_use]
pub fn hang_index(hang: f64) -> usize {
    let mut i = 0;
    let mut n = 1;
    while n < HANG_STEPS.len() {
        if HANG_STEPS[n] <= hang + 0.001 {
            i = n;
        }
        n += 1;
    }
    i
}

fn hang_below_top(hang: f64) -> bool {
    hang_index(hang) + 1 < HANG_STEPS.len()
}

/// The label the `Cord` row shows: the length, and which rung of the ladder that is, so a user who
/// typed 173 can see the two the buttons will move between.
fn cord_text(hang: f64) -> String {
    let i = hang_index(hang);
    format!("{} px ({} of {})", hang as i64, i + 1, HANG_STEPS.len())
}

fn drops_text(drop: f64) -> String {
    format!("{} px", drop as i64)
}

fn percent(v: f64) -> String {
    format!("{}%", (v * 100.0).round() as i64)
}

fn mouse_options() -> Vec<String> {
    let mut v = Vec::with_capacity(ClickThrough::ALL.len());
    for c in ClickThrough::ALL {
        v.push(c.label().to_string());
    }
    v
}

fn posture_options() -> Vec<String> {
    let mut v = Vec::with_capacity(PostureKind::ALL.len());
    for p in PostureKind::ALL {
        v.push(p.label().to_string());
    }
    v
}

fn index_of_click(cur: ClickThrough) -> usize {
    position(ClickThrough::ALL.iter().position(|c| *c == cur))
}

fn index_of_posture(cur: PostureKind) -> usize {
    position(PostureKind::ALL.iter().position(|p| *p == cur))
}

fn index_of_monitor(monitors: &[Monitor], wanted: u32) -> usize {
    position(monitors.iter().position(|m| m.index == wanted))
}

/// A `Choice` row has to point at *something*, and the first option is the one the document would
/// have been written with. `position` answers `None` only for a setting a hand-edited file put outside
/// the enum's set — which `sanitize` has already replaced, so this is a formality, not a fallback to
/// a wrong value.
fn position(found: Option<usize>) -> usize {
    found.unwrap_or(0)
}

/// One entry per display: enough to recognise it, nothing that identifies it. Resolution, scale
/// factor and index answer "which one is this", and no device name or serial number has to be read
/// off the system to say it.
fn monitor_options(monitors: &[Monitor], wanted: u32) -> Vec<String> {
    let mut v = Vec::with_capacity(monitors.len());
    for m in monitors {
        let b = m.bounds_logical();
        let mark = if m.index == wanted { " <- here" } else { "" };
        v.push(format!(
            "{}: {} x {} at {}%{}",
            m.index + 1,
            b.w(),
            b.h(),
            (m.scale * 100.0).round() as i64,
            mark
        ));
    }
    v
}

/// What a click on `id` asks for, or nothing when the click cannot change the document — an index
/// outside the list, or a button the window should not have enabled.
#[must_use]
pub fn change(s: &Settings, id: RowId, step: Step) -> Option<Command> {
    match (id, step) {
        (RowId::LaunchAtLogin, Step::Toggle) => Some(Command::ToggleLaunchAtLogin),
        (RowId::Topmost, Step::Toggle) => Some(Command::ToggleTopmost),
        (RowId::Monitor, Step::Pick(n)) => Some(Command::SetMonitor(n as u32)),
        (RowId::Hour12, Step::Toggle) => Some(Command::Toggle12Hour),
        (RowId::Seconds, Step::Toggle) => Some(Command::ToggleSeconds),
        (RowId::Meridiem, Step::Toggle) => Some(Command::ToggleMeridiem),
        (RowId::Posture, Step::Pick(n)) => {
            PostureKind::ALL.get(n).copied().map(Command::SetPosture)
        }
        (RowId::ClickThrough, Step::Pick(n)) => ClickThrough::ALL
            .get(n)
            .copied()
            .map(Command::SetClickThrough),
        (RowId::ClockSize, Step::Down) => Some(Command::Smaller),
        (RowId::ClockSize, Step::Up) => Some(Command::Bigger),
        (RowId::HangLength, Step::Down) => Some(Command::HangDown),
        (RowId::HangLength, Step::Up) => Some(Command::HangUp),
        // Both anchor rows answer with the whole pair, and the direction is all that differs between
        // them — which is why the four cases are two: `anchor_change` reads `id` for the row, so a
        // merged arm cannot answer for the wrong setting.
        (RowId::AnchorAcross | RowId::AnchorDrop, Step::Down) => anchor_change(s, id, false),
        (RowId::AnchorAcross | RowId::AnchorDrop, Step::Up) => anchor_change(s, id, true),
        (RowId::ResetPosition, Step::Press) => Some(Command::ResetPosition),
        _ => None,
    }
}

/// The anchor the window asks for when one of its two nudge rows is pressed. Both rows carry the
/// whole pair, because the pair is what the settings document holds and half a pair is not a
/// position.
fn anchor_change(s: &Settings, id: RowId, up: bool) -> Option<Command> {
    let o = &s.overlay;
    let dir = if up { 1.0 } else { -1.0 };
    let want = match id {
        RowId::AnchorAcross => Anchor::clamped(o.anchor_ratio + dir * RATIO_STEP, o.anchor_drop),
        RowId::AnchorDrop => Anchor::clamped(o.anchor_ratio, o.anchor_drop + dir * DROP_STEP),
        _ => return None,
    };
    if want == Anchor::from_overlay(o) {
        // Already at the limit the row's own buttons should have disabled.
        return None;
    }
    Some(Command::SetAnchor(want))
}
