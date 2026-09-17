//! The settings window: one small classic-mode Win32 window whose only job is to draw the rows the
//! model defines.
//!
//! # Why this shape
//!
//! The window owns nothing. It is handed `Vec<Group>` produced by [`hanglock_platform::panel::form`]
//! from the settings that are currently applied, and a click is turned back into a row and a
//! [`hanglock_platform::panel::Step`] and given to the app, which answers with a [`hanglock_platform::Command`] and the
//! model's own reaction to it. There is no draft copy of any setting in this file, no Apply button,
//! and no validation: the document stays the single writer, so the tray and the window cannot drift
//! and the window cannot put the app in a state the rest of it has to special-case.
//!
//! A control repaints itself the instant it is clicked (`BS_AUTO*`), so the picture can be a frame
//! ahead of the model — and can never end up wrong, because the next [`refresh`] restates every
//! control from the settings that were actually applied. That is also why there is no spinner or text
//! edit: a box the user can type into needs a moment at which the typed text becomes a value, and
//! that moment is where a settings window usually starts owning state. Every row here is a click that
//! is already a valid command.
//!
//! # What is deliberately not used
//!
//! No comctl32 property sheet (that would be tabs for three sections, plus an activation-context
//! question `docs/windows-overlay-notes.md` declines to open), no dialog template (a byte encoding
//! that cannot be checked from Linux), no UI framework, no manifest. `BUTTON` and `STATIC` are
//! ordinary user32 window classes, and the keyboard support is one call — [`sys::IsDialogMessageW`]
//! from the message loop, which gives Tab, arrows within a radio group, Space and Esc for free.
//!
//! # Geometry
//!
//! Logical pixels times this window's own DPI, re-laid out on `WM_DPICHANGED`: the overlay may be on a
//! 200 % display, and a settings window that only looked right at 100 % on the primary monitor would
//! be the one part of the app that is not DPI-aware.

use crate::sys;
use crate::window::{AppHook, Runtime};
use hanglock_platform::panel::{self, Control, Group, Row};

/// The window class. Registered once per process by [`open`], tolerating the already-registered error
/// the same way the overlay's class does.
const CLASS: &str = "HanglockPanel";
/// Logical pixels. `WIDTH` is wide enough for the longest option the form can produce — a display row
/// reading `2: 2560 x 1440 at 150%` beside a label — without the text being clipped, and narrow
/// enough that the window still reads as a small panel rather than an application.
const MARGIN: f64 = 14.0;
const WIDTH: f64 = 460.0;
const ROW_H: f64 = 24.0;
const RADIO_H: f64 = 22.0;
const ROW_GAP: f64 = 6.0;
const CAP_H: f64 = 22.0;
const GROUP_GAP: f64 = 12.0;
const LABEL_W: f64 = 180.0;
const BUTTON_W: f64 = 64.0;
const BUTTON_GAP: f64 = 6.0;
/// Child control ids. A nudge row is four controls, so the slot number carries both the row and the
/// part, and one division recovers them from a `WM_COMMAND`.
const ID_BASE: usize = 0x4900;
const PARTS_PER_ROW: usize = 4;
const ID_CLOSE: usize = ID_BASE - 1;

/// What to do when a row was answered: the app's turn, reached without this window knowing what an
/// app is. A function pointer rather than a generic parameter because [`crate::Host`], which owns the
/// window's lifetime, is not generic over the app type — and the only thing the panel needs to do with
/// the pointer is pass it back with the pointer it was given.
pub type AnswerFn = unsafe fn(*mut core::ffi::c_void, RowId, panel::Step);

/// Open a panel window for `state`. Set by `run`, which knows `A`, so that `Host::show_settings` does
/// not have to.
pub type OpenFn = unsafe fn(*mut core::ffi::c_void, sys::HMODULE, Vec<Group>) -> sys::HWND;

/// Which piece of a row this is. A row is one *setting* and up to four controls, and the difference
/// matters: only the buttons take the tab, and only the value and the label are `STATIC`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Part {
    /// The row's own words, as a label. Not used by a check row, where the same words are the box's
    /// caption — a checkbox with a separate label next to it is two things saying one thing.
    Label,
    Check,
    Radio,
    Value,
    Less,
    More,
    Push,
}

/// One control of the window, and the row it belongs to.
struct Panel {
    groups: Vec<Group>,
    /// `children[row][part]`, in the order [`parts`] lists them, which is also the tab order.
    children: Vec<Vec<sys::HWND>>,
    captions: Vec<sys::HWND>,
    close: sys::HWND,
    font: sys::HFONT,
    instance: sys::HMODULE,
    /// Whatever [`OpenFn`] was handed, passed straight to [`AnswerFn`]. For the app it is the overlay's
    /// `Runtime`, which is why the lifetime rule is `window.rs`'s: the message loop cannot outlive the
    /// frame that owns `Runtime`, and this window is destroyed before that frame ends.
    runtime: *mut core::ffi::c_void,
    on_answer: AnswerFn,
}

impl Panel {
    /// The row a control id points at. The index is the flat row order, which is also the order the
    /// ids were handed out in, so the two cannot disagree about which row is which.
    fn row_at(&self, index: usize) -> Option<Row> {
        self.groups
            .iter()
            .flat_map(|g| g.rows.iter())
            .nth(index)
            .cloned()
    }
}

/// The controls of one row, in the order they are created and tabbed through.
fn parts(row: &Row) -> Vec<(Part, usize)> {
    match &row.control {
        Control::Check { .. } => vec![(Part::Check, 0)],
        Control::Choice { options, .. } => {
            let mut v = Vec::with_capacity(options.len() + 1);
            v.push((Part::Label, 0));
            for (i, _) in options.iter().enumerate() {
                v.push((Part::Radio, i));
            }
            v
        }
        Control::Nudge { .. } => vec![
            (Part::Label, 0),
            (Part::Less, 0),
            (Part::Value, 0),
            (Part::More, 0),
        ],
        Control::Push => vec![(Part::Push, 0)],
    }
}

fn is_button(part: Part) -> bool {
    matches!(
        part,
        Part::Check | Part::Radio | Part::Less | Part::More | Part::Push
    )
}

/// The row's height, which is more than one line only when a choice lists its options underneath.
fn row_height(row: &Row) -> f64 {
    match &row.control {
        Control::Choice { options, .. } => ROW_H + RADIO_H * options.len() as f64,
        _ => ROW_H,
    }
}

/// The caption of one control: what it says, which for a radio is the option's own words and for a
/// nudge's value is the number the model is holding.
fn text_of(row: &Row, part: Part, which: usize) -> String {
    match part {
        // A nudge's and a choice's label is a separate caption; a check row and a push row *are* their
        // label, which is why the same words come back from three parts and one expression.
        Part::Label | Part::Check | Part::Push => row.label.to_string(),
        Part::Radio => match &row.control {
            Control::Choice { options, .. } => options.get(which).cloned().unwrap_or_default(),
            _ => String::new(),
        },
        Part::Value => match &row.control {
            Control::Nudge { text, .. } => text.clone(),
            _ => String::new(),
        },
        Part::Less => "Less".to_string(),
        Part::More => "More".to_string(),
    }
}

/// Whether a control is ticked. A check row knows its own state; a radio knows its index.
fn checked_of(row: &Row, part: Part, which: usize) -> bool {
    match (&row.control, part) {
        (Control::Check { on }, Part::Check) => *on,
        (Control::Choice { selected, .. }, Part::Radio) => *selected == which,
        _ => false,
    }
}

/// Whether a control can be used. Only the two buttons of a nudge can be refused, and they are refused
/// exactly when the row says the step would not change anything — so a click never lands on a value
/// that is already at its limit.
fn enabled_of(row: &Row, part: Part) -> bool {
    match (&row.control, part) {
        (Control::Nudge { can_down, .. }, Part::Less) => *can_down,
        (Control::Nudge { can_up, .. }, Part::More) => *can_up,
        _ => true,
    }
}

/// This window's DPI as a scale factor, so one formula serves 100 % and 250 %.
fn scale_of(hwnd: sys::HWND) -> f64 {
    let dpi = unsafe { sys::GetDpiForWindow(hwnd) };
    if dpi == 0 {
        1.0
    } else {
        f64::from(dpi) / hanglock_core::units::LOGICAL_DPI
    }
}

/// Where one control goes, in device px, given the row's top edge.
fn box_of(row: &Row, part: Part, which: usize, y: f64, w: f64, s: f64) -> (i32, i32, i32, i32) {
    let right = w - MARGIN * s;
    let line = y;
    let radio_line = y + (ROW_H + RADIO_H * which as f64) * s;
    let (x0, x1, top, bottom) = match part {
        Part::Radio => (
            (MARGIN + 16.0) * s,
            right,
            radio_line,
            radio_line + RADIO_H * s,
        ),
        Part::Label if matches!(row.control, Control::Choice { .. }) => {
            (MARGIN * s, right, line, line + ROW_H * s)
        }
        Part::Label => (MARGIN * s, (MARGIN + LABEL_W) * s, line, line + ROW_H * s),
        Part::Value => (
            (MARGIN + LABEL_W + 8.0) * s,
            right - (2.0 * BUTTON_W + 2.0 * BUTTON_GAP) * s,
            line,
            line + ROW_H * s,
        ),
        Part::Less => (
            right - (2.0 * BUTTON_W + BUTTON_GAP) * s,
            right - (BUTTON_W + BUTTON_GAP) * s,
            line,
            line + ROW_H * s,
        ),
        Part::More => (right - BUTTON_W * s, right, line, line + ROW_H * s),
        Part::Check | Part::Push => (MARGIN * s, right, line, line + ROW_H * s),
    };
    (
        x0.round() as i32,
        top.round() as i32,
        (x1 - x0).round().max(1.0) as i32,
        (bottom - top).round().max(1.0) as i32,
    )
}

unsafe fn make_child(
    parent: sys::HWND,
    panel: &Panel,
    row: &Row,
    part: Part,
    which: usize,
    first: bool,
    id: usize,
    s: f64,
) -> sys::HWND {
    let class = sys::wide(if is_button(part) { "BUTTON" } else { "STATIC" });
    let mut style = sys::WS_VISIBLE | if is_button(part) { 0 } else { sys::SS_LEFT };
    style |= match part {
        Part::Check => sys::BS_AUTOCHECKBOX,
        Part::Radio => sys::BS_AUTORADIOBUTTON,
        Part::Less | Part::More | Part::Push => sys::BS_PUSHBUTTON,
        Part::Label | Part::Value => 0,
    };
    // One tab stop per *button* and one `WS_GROUP` per *row*: Tab then walks the settings and the
    // arrow keys walk the options of a choice, which is what the dialog manager makes of a caption
    // followed by a run of radio buttons. The caption opens the group, so the arrows cannot leave the
    // row they belong to.
    if is_button(part) {
        style |= sys::WS_TABSTOP;
    }
    if first {
        style |= sys::WS_GROUP;
    }
    let text = sys::wide(&text_of(row, part, which));
    let (x, y, w, h) = box_of(row, part, which, 0.0, WIDTH * s, s);
    unsafe {
        let hwnd = sys::CreateWindowExW(
            0,
            class.as_ptr(),
            text.as_ptr(),
            sys::WS_CHILD | style,
            x,
            y,
            w,
            h,
            parent,
            sys::id_menu(id),
            panel.instance,
            std::ptr::null(),
        );
        if !panel.font.is_null() {
            sys::SendMessageW(hwnd, sys::WM_SETFONT, panel.font as usize, 1);
        }
        hwnd
    }
}

/// Restate every control from `panel.groups`: create what is missing, then set text, tick, enable and
/// frame. This is the whole of the window's state handling, and it is idempotent, which is why opening,
/// refreshing after a command and reacting to a DPI change are the same call.
///
/// The work is in [`sync_row`] and [`size_to_content`]; what is left here is the walk that gives them
/// coordinates, so the two questions — what does each control say, and how big is the window — are
/// answered one at a time.
unsafe fn repaint(hwnd: sys::HWND, panel: &mut Panel) {
    let s = scale_of(hwnd);
    let w = WIDTH * s;
    let mut y = MARGIN * s;
    let mut gi = 0usize;
    let mut ri = 0usize;
    // Cloned so the rows can be read while the children are created: a borrow of `panel.groups` and a
    // mutation of `panel.children` would otherwise fight, and the row set is a dozen small values that
    // are walked only when a command lands, not per frame.
    let groups = panel.groups.clone();
    for group in &groups {
        if panel.captions.len() <= gi {
            let cap = unsafe { make_plain_child(hwnd, panel, group.title, 0, false) };
            panel.captions.push(cap);
        }
        let cap = panel.captions[gi];
        unsafe {
            sys::MoveWindow(
                cap,
                (MARGIN * s) as i32,
                y as i32,
                (w - 2.0 * MARGIN * s) as i32,
                (CAP_H * s) as i32,
                1,
            );
        }
        y += (CAP_H + 2.0) * s;
        for row in &group.rows {
            sync_row(hwnd, panel, row, ri, y, w, s);
            y += (row_height(row) + ROW_GAP) * s;
            ri += 1;
        }
        y += GROUP_GAP * s;
        gi += 1;
    }
    if panel.close.is_null() {
        panel.close = unsafe { make_plain_child(hwnd, panel, "Close", ID_CLOSE, true) };
    }
    let close_w = (BUTTON_W + 32.0) * s;
    let close_h = (ROW_H + 8.0) * s;
    unsafe {
        sys::MoveWindow(
            panel.close,
            (w - MARGIN * s - close_w).round() as i32,
            y.round() as i32,
            close_w.round() as i32,
            close_h.round() as i32,
            1,
        );
    }
    y += close_h + MARGIN * s;
    unsafe { size_to_content(hwnd, w, y) };
}

/// One row's controls: create what is missing, then restate text, place, enable state and tick.
///
/// A row that already has all its parts is only *restated* — that is what makes a command, a DPI
/// change and a first paint the same operation, and why there is no "update" path separate from an
/// "initialise" one to get out of sync with the other.
unsafe fn sync_row(
    hwnd: sys::HWND,
    panel: &mut Panel,
    row: &Row,
    ri: usize,
    y: f64,
    w: f64,
    s: f64,
) {
    let want = parts(row);
    if panel.children.len() <= ri {
        panel.children.push(Vec::new());
    }
    while panel.children[ri].len() < want.len() {
        let k = panel.children[ri].len();
        let (part, which) = want[k];
        let id = ID_BASE + ri * PARTS_PER_ROW + k;
        let child = unsafe { make_child(hwnd, panel, row, part, which, k == 0, id, s) };
        panel.children[ri].push(child);
    }
    for (k, (part, which)) in want.iter().enumerate() {
        let Some(child) = panel.children[ri].get(k).copied() else {
            continue;
        };
        let (x, ty, cw, ch) = box_of(row, *part, *which, y, w, s);
        let text = sys::wide(&text_of(row, *part, *which));
        unsafe {
            sys::SetWindowTextW(child, text.as_ptr());
            sys::MoveWindow(child, x, ty, cw, ch, 1);
            sys::EnableWindow(child, i32::from(enabled_of(row, *part)));
            if matches!(*part, Part::Check | Part::Radio) {
                let on = checked_of(row, *part, *which);
                sys::SendMessageW(
                    child,
                    sys::BM_SETCHECK,
                    if on {
                        sys::BST_CHECKED
                    } else {
                        sys::BST_UNCHECKED
                    },
                    0,
                );
            }
        }
    }
}

/// Give the window the rect its content needs.
///
/// The rows decide the *client* size; the frame around it belongs to the window manager, and its width
/// is neither constant nor small once a caption, a border and a per-monitor-DPI scale are all in the
/// same sum. So the client rect goes in and the window rect comes out, rather than this file guessing
/// at a border width the way a hand-written dialog template would have to.
unsafe fn size_to_content(hwnd: sys::HWND, w: f64, y: f64) {
    let style = unsafe { sys::GetWindowLongPtrW(hwnd, sys::GWL_STYLE) } as sys::DWORD;
    let mut want = sys::WINRECT {
        left: 0,
        top: 0,
        right: w.round() as i32,
        bottom: y.round() as i32,
    };
    let (ww, hh) = if unsafe { sys::AdjustWindowRectEx(&mut want, style, 0, 0) } != 0 {
        (want.right - want.left, want.bottom - want.top)
    } else {
        (w.round() as i32, y.round() as i32)
    };
    unsafe {
        sys::SetWindowPos(
            hwnd,
            std::ptr::null_mut(),
            0,
            0,
            ww,
            hh,
            sys::SWP_NOMOVE | sys::SWP_NOZORDER | sys::SWP_NOACTIVATE,
        );
    }
}

/// A row caption or the Close button: a control whose only state is its text, made here so the font
/// and the parent are handled in one place.
unsafe fn make_plain_child(
    hwnd: sys::HWND,
    panel: &Panel,
    text: &str,
    id: usize,
    button: bool,
) -> sys::HWND {
    let class = sys::wide(if button { "BUTTON" } else { "STATIC" });
    let wide = sys::wide(text);
    let style = sys::WS_CHILD
        | sys::WS_VISIBLE
        | if button {
            sys::BS_PUSHBUTTON | sys::WS_TABSTOP | sys::WS_GROUP
        } else {
            sys::SS_LEFT | sys::WS_GROUP
        };
    unsafe {
        let child = sys::CreateWindowExW(
            0,
            class.as_ptr(),
            wide.as_ptr(),
            style,
            0,
            0,
            10,
            10,
            hwnd,
            sys::id_menu(id),
            panel.instance,
            std::ptr::null(),
        );
        if !panel.font.is_null() {
            sys::SendMessageW(child, sys::WM_SETFONT, panel.font as usize, 1);
        }
        child
    }
}

/// The system UI font, without a manifest or a common control. The shell's icon-title font is Segoe UI
/// at the size the user chose; `DEFAULT_GUI_FONT`, the other candidate, is the old 8 pt ANSI face and
/// would make the window look thirty years old.
unsafe fn panel_font() -> sys::HFONT {
    let mut lf = sys::LOGFONTW::default();
    unsafe {
        if sys::SystemParametersInfoW(
            sys::SPI_GETICONTITLELOGFONT,
            std::mem::size_of::<sys::LOGFONTW>() as u32,
            &mut lf as *mut _ as *mut core::ffi::c_void,
            0,
        ) == 0
        {
            return std::ptr::null_mut();
        }
        sys::CreateFontIndirectW(&lf)
    }
}

/// Create the settings window and lay its rows out.
///
/// `Host::show_settings` owns the create-or-focus decision, so this function only ever runs when there
/// is no window on screen. A second `Settings…` click must not reach here: two windows would be two
/// pictures of the same rows, which is what this file exists to prevent.
///
/// # Safety
/// `runtime` is passed unchanged to `on_answer`, which for the app in this workspace means it must be
/// the live `Runtime` of the overlay window — the same pointer its `GWLP_USERDATA` holds. `instance`
/// must be a live module handle for this process, and `on_answer` a function that accepts `runtime`.
pub unsafe fn open(
    runtime: *mut core::ffi::c_void,
    instance: sys::HMODULE,
    groups: Vec<Group>,
    on_answer: AnswerFn,
) -> sys::HWND {
    unsafe {
        let name = sys::wide(CLASS);
        let wc = sys::WNDCLASSEXW {
            cb_size: std::mem::size_of::<sys::WNDCLASSEXW>() as u32,
            // No class styles: a settings window is repainted by its own controls, and `CS_HREDRAW`
            // would only add work when the user drags it between displays of different DPI.
            style: 0,
            lpfn_wnd_proc: Some(wndproc),
            h_instance: instance,
            // The button-face brush is what makes a `STATIC` caption look like part of the window
            // instead of a white card, without a `WM_CTLCOLORSTATIC` handler of our own.
            h_br_background: sys::GetSysColorBrush(sys::COLOR_BTNFACE),
            lpsz_class_name: name.as_ptr(),
            ..unsafe { std::mem::zeroed() }
        };
        if sys::RegisterClassExW(&wc) == 0 && sys::GetLastError() != sys::ERROR_CLASS_ALREADY_EXISTS
        {
            return std::ptr::null_mut();
        }
        let title = sys::wide("Hanglock settings");
        // Caption and close box, and nothing else: no thick frame to resize against content laid out
        // for one width, no minimize button on a window whose whole life is a few seconds long.
        let style = sys::WS_CAPTION | sys::WS_SYSMENU;
        let hwnd = sys::CreateWindowExW(
            0,
            name.as_ptr(),
            title.as_ptr(),
            style,
            sys::CW_USEDEFAULT,
            sys::CW_USEDEFAULT,
            WIDTH as i32,
            360,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            instance,
            std::ptr::null(),
        );
        if hwnd.is_null() {
            return std::ptr::null_mut();
        }
        let raw = Box::into_raw(Box::new(Panel {
            groups,
            children: Vec::new(),
            captions: Vec::new(),
            close: std::ptr::null_mut(),
            font: panel_font(),
            instance,
            runtime,
            on_answer,
        }));
        sys::SetWindowLongPtrW(hwnd, sys::GWLP_USERDATA, raw as isize);
        let panel = unsafe { &mut *raw };
        repaint(hwnd, panel);
        sys::ShowWindow(hwnd, sys::SW_RESTORE);
        sys::SetForegroundWindow(hwnd);
        hwnd
    }
}

/// Bring an open window to the front. The window is never re-created on a second `Settings…`, because
/// a second window would be a second picture of the same rows, which is precisely what this file
/// exists to prevent.
pub fn focus(hwnd: sys::HWND) {
    unsafe {
        sys::ShowWindow(hwnd, sys::SW_RESTORE);
        sys::SetForegroundWindow(hwnd);
    }
}

/// Replace the rows. Called after every command the model answers, from `Host` and from this window's
/// own click path, so the picture is never one step behind the document.
pub fn refresh(hwnd: sys::HWND, groups: Vec<Group>) {
    let ud = unsafe { sys::GetWindowLongPtrW(hwnd, sys::GWLP_USERDATA) };
    if ud == 0 {
        return;
    }
    let panel = unsafe { &mut *(ud as *mut Panel) };
    if panel.groups == groups {
        return;
    }
    panel.groups = groups;
    unsafe {
        repaint(hwnd, panel);
    }
}

/// A `WM_COMMAND`'s `lParam` for a child is the child's handle, and its `wParam` carries the id in the
/// low word and the notification in the high word.
fn command_parts(w: usize) -> (usize, usize) {
    (w & 0xFFFF, (w >> 16) & 0xFFFF)
}

unsafe extern "system" fn wndproc(hwnd: sys::HWND, msg: u32, w: usize, l: isize) -> isize {
    let ud = unsafe { sys::GetWindowLongPtrW(hwnd, sys::GWLP_USERDATA) };
    if ud == 0 {
        // `CreateWindowExW` runs `WM_CREATE` and `WM_NCCREATE` before the window long is written, so
        // the first messages of this window's life arrive with nothing behind the pointer.
        return unsafe { sys::DefWindowProcW(hwnd, msg, w, l) };
    }
    let panel = unsafe { &mut *(ud as *mut Panel) };
    match msg {
        sys::WM_COMMAND => {
            let (id, code) = command_parts(w);
            if id == sys::IDCANCEL {
                // Esc. The dialog manager turns the key into this id, and a window that ignored it
                // would break the one reflex every Windows user has.
                unsafe {
                    sys::DestroyWindow(hwnd);
                }
                return 0;
            }
            if code != sys::BN_CLICKED {
                // `EN_SETFOCUS` and the rest of a child's non-click notifications are its own business.
                return unsafe { sys::DefWindowProcW(hwnd, msg, w, l) };
            }
            if id == ID_CLOSE || id == sys::IDOK {
                // The button, and the Enter key as the dialog manager turns it into a default-button
                // id. Both mean "I have finished looking".
                unsafe {
                    sys::DestroyWindow(hwnd);
                }
                return 0;
            }
            if id < ID_BASE {
                // Some other child's notification (the caption's system menu, for instance). Nothing in
                // the row range, so nothing to translate — and the subtraction below has to know that.
                return unsafe { sys::DefWindowProcW(hwnd, msg, w, l) };
            }
            let slot = id - ID_BASE;
            let row_index = slot / PARTS_PER_ROW;
            let part_index = slot % PARTS_PER_ROW;
            let Some(row) = panel.row_at(row_index) else {
                return 0;
            };
            let Some((part, which)) = parts(&row).get(part_index).copied() else {
                return 0;
            };
            let Some(step) = step_for(part, which) else {
                return 0;
            };
            unsafe {
                (panel.on_answer)(panel.runtime, row.id, step);
            }
            0
        }
        sys::WM_CLOSE => {
            unsafe {
                sys::DestroyWindow(hwnd);
            }
            0
        }
        sys::WM_DPICHANGED => {
            // The suggested rect from the system is a frame hint; this window has no scroll state and
            // no user-chosen size, so the only thing worth honouring is the new scale, and that is
            // what `repaint` re-derives from the window.
            unsafe {
                let rect = &*(l as *const sys::WINRECT);
                sys::SetWindowPos(
                    hwnd,
                    std::ptr::null_mut(),
                    rect.left,
                    rect.top,
                    rect.right - rect.left,
                    rect.bottom - rect.top,
                    sys::SWP_NOZORDER | sys::SWP_NOACTIVATE,
                );
            }
            unsafe {
                repaint(hwnd, panel);
            }
            0
        }
        sys::WM_NCDESTROY => {
            // Detach first, then free: a message that arrives between the two must find a null window
            // long rather than a pointer to a `Panel` that has been dropped. The children are gone by
            // now, so the font they were drawn in can go with them.
            unsafe {
                sys::SetWindowLongPtrW(hwnd, sys::GWLP_USERDATA, 0);
                if !panel.font.is_null() {
                    sys::DeleteObject(panel.font);
                }
                drop(Box::from_raw(ud as *mut Panel));
            }
            0
        }
        _ => unsafe { sys::DefWindowProcW(hwnd, msg, w, l) },
    }
}

/// The one place the panel reaches into the app: the overlay's `Runtime`, whose `A` this function was
/// instantiated for. Keeping the cast here means `wndproc` never has to know what an app type is.
///
/// # Safety
/// `state` must be a live `Runtime<A>` for the `A` this was created with, exactly as `window.rs`'s
/// `GWLP_USERDATA` contract states.
pub unsafe fn answer<A: AppHook + 'static>(
    state: *mut core::ffi::c_void,
    id: RowId,
    step: panel::Step,
) {
    let rt = unsafe { &mut *(state as *mut Runtime<A>) };
    rt.app.panel_command(&mut rt.host, id, step);
}

/// What a click on one control of one row means. The label and the value of a nudge say nothing: they
/// are there to be read.
fn step_for(part: Part, which: usize) -> Option<panel::Step> {
    match part {
        Part::Check => Some(panel::Step::Toggle),
        Part::Radio => Some(panel::Step::Pick(which)),
        Part::Less => Some(panel::Step::Down),
        Part::More => Some(panel::Step::Up),
        Part::Push => Some(panel::Step::Press),
        Part::Label | Part::Value => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hanglock_core::settings::Settings;

    /// A panel that answers nothing, for the tests that only read the rows. A named `unsafe fn` rather
    /// than a closure, because the field is an `unsafe fn` pointer and a closure cannot produce one.
    unsafe fn no_answer(_state: *mut core::ffi::c_void, _id: RowId, _step: panel::Step) {}

    /// Every row of the real form, in the order the window will lay them out.
    fn rows() -> Vec<Row> {
        panel::form(&Settings::default(), &[])
            .iter()
            .flat_map(|g| g.rows.clone())
            .collect()
    }

    #[test]
    fn the_id_per_row_holds_every_row_s_parts() {
        // `WM_COMMAND` recovers a row and a part from one id by dividing by `PARTS_PER_ROW`. If a row
        // ever grows past that, two controls share an id and a click answers the wrong setting — with
        // no crash and nothing to see. This is the assert that makes adding a fifth part an edit here
        // rather than a bug report.
        for row in rows() {
            assert!(
                parts(&row).len() <= PARTS_PER_ROW,
                "{:?} has {} parts",
                row.id,
                parts(&row).len()
            );
        }
    }

    #[test]
    fn every_control_fits_its_window_at_every_scale() {
        // A control placed outside the client rect is not clipped with a visible edge, it is simply
        // unreachable, and the only symptom a user gets is a setting they cannot find.
        for scale in [0.5, 1.0, 1.25, 1.5, 2.0, 3.0] {
            let w = WIDTH * scale;
            for row in rows() {
                let h = row_height(row);
                for (k, (part, which)) in parts(&row).iter().enumerate() {
                    let y = if *part == Part::Radio { ROW_H } else { 0.0 };
                    let (x, top, cw, ch) = box_of(row, *part, *which, y, w, scale);
                    assert!(cw > 0 && ch > 0, "empty control {:?}/{:?}", row.id, part);
                    assert!(x >= 0, "negative x {:?}/{:?} at {scale}", row.id, part);
                    assert!(
                        f64::from(x + cw) <= w + 1.0,
                        "{:?}/{:?} overflows the client at {scale}: {} > {w}",
                        row.id,
                        part,
                        x + cw
                    );
                    assert!(
                        f64::from(top + ch) <= (y + h) * scale + 1.0,
                        "{:?}/{:?} part {k} leaves its row at {scale}",
                        row.id,
                        part
                    );
                }
            }
        }
    }

    #[test]
    fn a_choice_ticks_exactly_one_option() {
        for row in rows() {
            if let Control::Choice { options, .. } = &row.control {
                if options.is_empty() {
                    // Reachable only in a test that passes no displays: the shipped window always has
                    // at least one monitor to offer.
                    continue;
                }
                let ticks = (0..options.len())
                    .filter(|k| checked_of(&row, Part::Radio, *k))
                    .count();
                assert_eq!(ticks, 1, "{:?} ticked {ticks} options", row.id);
            }
        }
    }

    #[test]
    fn the_unchangeable_parts_of_a_row_say_nothing() {
        // A label is not a button: it must never answer with a step, or reading the form would be an
        // edit.
        for row in rows() {
            for (part, which) in parts(&row) {
                let said = step_for(part, which);
                match part {
                    Part::Label | Part::Value => assert_eq!(said, None),
                    _ => assert!(said.is_some(), "{:?} / {part:?}", row.id),
                }
            }
        }
    }

    #[test]
    fn a_nudge_greys_out_the_button_that_cannot_move_the_value() {
        let mut s = Settings::default();
        s.overlay.scale = hanglock_core::settings::limits::SCALE.1;
        let groups = panel::form(&s, &[]);
        let size = groups
            .iter()
            .flat_map(|g| g.rows.iter())
            .find(|r| r.id == RowId::ClockSize)
            .expect("the form has a Size row");
        assert!(
            !enabled_of(size, Part::More),
            "bigger is offered at the top of the range"
        );
        assert!(
            enabled_of(size, Part::Less),
            "smaller is refused at the top"
        );
    }

    #[test]
    fn rows_are_read_back_in_creation_order() {
        // The ids are handed out by flat row index, so `row_at` has to agree with `repaint` about which
        // row is which.
        let all = rows();
        let mut p = Panel {
            groups: panel::form(&Settings::default(), &[]),
            children: Vec::new(),
            captions: Vec::new(),
            close: std::ptr::null_mut(),
            font: std::ptr::null_mut(),
            instance: std::ptr::null_mut(),
            runtime: std::ptr::null_mut(),
            on_answer: no_answer,
        };
        for (i, want) in all.iter().enumerate() {
            assert_eq!(p.row_at(i).map(|r| r.id), Some(want.id));
        }
        assert_eq!(p.row_at(all.len()), None);
        p.groups.clear();
        assert_eq!(p.row_at(0), None);
    }
}
