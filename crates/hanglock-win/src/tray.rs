//! The notification-area icon and the menu that hangs off it.
//!
//! The menu is a real Win32 popup rather than a drawn one, for the same reason the reference project
//! uses a real `NSMenu`: native metrics, keyboard navigation, screen readers, and no rendering
//! responsibility of our own. It carries commands only — no sliders, no read-outs.
//!
//! The icon is also the app's *survival* mechanism. The overlay window must never appear in Alt+Tab,
//! so without a tray icon there is no way out but Task Manager. Everything else on this menu is
//! convenience; `Quit` is load-bearing, which is why the card's own right-click menu duplicates it.

use crate::icon;
use crate::sys;
use hanglock_core::ids::{ClickThrough, ClockStyle, PostureKind};
use hanglock_platform::Command;

/// `TPM_RETURNCMD` gives the selected id straight back, so no message plumbing is needed and the ids
/// never escape this file.
const ID_BASE: usize = 0x4800;

pub struct Tray {
    uid: u32,
    hicon: sys::HICON,
    added: bool,
    tooltip: Vec<sys::WCHAR>,
    hwnd: sys::HWND,
}

/// The state the menu shows checkmarks for, passed in rather than read from settings: the tray must
/// not know what a settings document is. A snapshot for the same reason — a menu is open for as long as
/// the user is reading it, and a menu that queries live state would have to be rebuilt under the
/// cursor mid-track.
#[derive(Clone, Debug, Default)]
pub struct MenuState {
    pub visible: bool,
    pub topmost: bool,
    pub seconds: bool,
    pub hour12: bool,
    pub meridiem: bool,
    pub style: ClockStyle,
    pub posture: PostureKind,
    pub click_through: ClickThrough,
    pub launch_at_login: bool,
    /// `(index, label)` per display, in the order they were enumerated. Labels rather than monitors
    /// because the tray's job is to draw words it is handed, not to decide which display is which.
    pub monitors: Vec<(u32, String)>,
    pub monitor: u32,
    /// A refusal worth reading. Drawn as a greyed line at the top, because a menu item that silently
    /// did nothing is what a user would otherwise conclude happened.
    pub notice: Option<&'static str>,
}

impl Tray {
    #[must_use]
    /// # Safety
    /// `hwnd`/`instance` reach `Shell_NotifyIconW`/`CreateIcon`; both must be live handles of
    /// this process (the window may not exist yet, the icon is only staged here).
    pub unsafe fn new(hwnd: sys::HWND, instance: sys::HMODULE) -> Self {
        Self {
            uid: 1,
            hicon: unsafe { icon::create(instance) },
            added: false,
            tooltip: sys::wide("Hanglock"),
            hwnd,
        }
    }

    #[must_use]
    pub fn is_installed(&self) -> bool {
        self.added
    }

    pub fn install(&mut self, tooltip: &str) {
        self.set_tooltip(tooltip);
        let mut d = self.data();
        d.flags = sys::NIF_MESSAGE | sys::NIF_ICON | sys::NIF_TIP;
        d.callback_message = sys::WM_TRAYICON;
        d.icon = self.hicon;
        unsafe {
            self.added = sys::Shell_NotifyIconW(sys::NIM_ADD, &d) != 0;
        }
    }

    pub fn uninstall(&mut self) {
        if !self.added {
            return;
        }
        let mut d = self.data();
        d.uid = self.uid;
        unsafe {
            sys::Shell_NotifyIconW(sys::NIM_DELETE, &d);
        }
        self.added = false;
    }

    pub fn set_tooltip(&mut self, text: &str) {
        let want = sys::wide(text);
        if self.tooltip == want {
            // `NIM_MODIFY` is a round trip to another process, and the adapter calls this after every
            // apply — which is every frame the rope is awake. Comparing the encoded form is what keeps
            // an unchanged tooltip free instead of turning a settled clock into a chatty one.
            return;
        }
        self.tooltip = want;
        if !self.added {
            return;
        }
        let mut d = self.data();
        d.flags = sys::NIF_TIP;
        let n = self.tooltip.len().min(d.tip.len() - 1);
        d.tip[..n].copy_from_slice(&self.tooltip[..n]);
        d.tip[n] = 0;
        unsafe {
            sys::Shell_NotifyIconW(sys::NIM_MODIFY, &d);
        }
    }

    fn data(&self) -> sys::NOTIFYICONDATAW {
        let mut d: sys::NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
        d.cb_size = std::mem::size_of::<sys::NOTIFYICONDATAW>() as u32;
        d.hwnd = self.hwnd;
        d.uid = self.uid;
        d
    }

    /// Show the menu with its top-left at a *screen* point, and return the chosen command.
    ///
    /// This blocks until the menu is dismissed. That is the right shape for a menu and it removes an
    /// interleaving: no tick can fire while the user is reading, so no command can arrive while the
    /// clock is mid-drag.
    ///
    /// The layout is three groups, not a list of every setting: what a person comes here for is the
    /// pair of decisions the clock cannot show them (is it on top, and does it take the mouse), the
    /// two they change for an evening (seconds, 12/24), and the one that gets them out of a corner
    /// (`Reset position`). Everything else that lives here is also in the settings window; the things
    /// that live *only* here are `Reset position` and `Exit`, which is the ratio a tray menu should
    /// have.
    #[must_use]
    pub fn show_menu(&mut self, at: (i32, i32), st: &MenuState) -> Option<Command> {
        let menu = unsafe { sys::CreatePopupMenu() };
        if menu.is_null() {
            return None;
        }
        let mut plan = Plan::default();
        plan.fill(menu, st);
        let chosen = self.track(menu, at, &plan);
        unsafe { sys::DestroyMenu(menu) };
        chosen
    }

    /// Display the popup at a screen point and wait for it.
    ///
    /// A popup will not dismiss itself on click-away unless its owner window is foreground, and taking
    /// foreground away from the user's app is exactly what this overlay must never do. The documented
    /// sequence: claim foreground, track, then send a null message so the system releases the capture
    /// properly, then hand foreground back. The caret position in the other app is untouched throughout,
    /// because we never activate anything.
    fn track(&self, menu: sys::HMENU, at: (i32, i32), plan: &Plan) -> Option<Command> {
        let chosen = unsafe {
            let prev = sys::GetForegroundWindow();
            sys::SetForegroundWindow(self.hwnd);
            let chosen = sys::TrackPopupMenuEx(
                menu,
                sys::TPM_RETURNCMD | sys::TPM_RIGHTBUTTON | sys::TPM_BOTTOMALIGN,
                at.0,
                at.1,
                self.hwnd,
                std::ptr::null(),
            );
            sys::SendMessageW(self.hwnd, sys::WM_NULL, 0, 0);
            if !prev.is_null() && prev != self.hwnd {
                sys::SetForegroundWindow(prev);
            }
            chosen
        };
        plan.command(chosen as usize)
    }
}

/// A greyed line with no id behind it: it can be read and cannot be chosen, which is what a refusal
/// deserves. `Plan::command` can never match it, since real ids start above `ID_BASE`.
fn note(menu: sys::HMENU, label: &str) {
    let text = sys::wide(label);
    unsafe {
        sys::AppendMenuW(menu, sys::MF_STRING | sys::MF_GRAYED, 0, text.as_ptr());
    }
}

fn sep(menu: sys::HMENU) {
    unsafe {
        sys::AppendMenuW(menu, sys::MF_SEPARATOR, 0, std::ptr::null());
    }
}

/// A submenu, or a null handle the caller skips. The parent owns it once appended — destroying the menu
/// destroys its submenus with it, which is why nothing frees `sub` separately. A `CreatePopupMenu` that
/// failed is a null handle, and the parent simply does not get that branch: a menu with one missing
/// submenu is a worse outcome than a menu with one fewer choice, and neither is worth failing over.
fn submenu(menu: sys::HMENU, title: &str) -> sys::HMENU {
    let sub = unsafe { sys::CreatePopupMenu() };
    if !sub.is_null() {
        let text = sys::wide(title);
        unsafe {
            sys::AppendMenuW(menu, sys::MF_POPUP, sub as usize, text.as_ptr());
        }
    }
    sub
}

/// The id counter and the id-to-command table of one popup.
///
/// A menu item has to carry a value with it, and `TrackPopupMenuEx` answers with only the id — so the
/// table is the menu's meaning, and it is built at the same moment as the menu. Keeping the two in one
/// object is what stops an id being handed out twice or a label and a command disagreeing: there is one
/// method that does both, and no way to call only half of it.
#[derive(Default)]
struct Plan {
    next: usize,
    items: Vec<(usize, Command)>,
}

impl Plan {
    /// One pickable row. `checked` is the tick, `on` whether it can be chosen at all — a row the user
    /// is not allowed to pick is still worth showing them, which is the difference between a greyed
    /// item and an absent one.
    fn item(&mut self, menu: sys::HMENU, label: &str, cmd: Command, checked: bool, on: bool) {
        self.next += 1;
        let id = ID_BASE + self.next;
        let mut flags = sys::MF_STRING;
        if checked {
            flags |= sys::MF_CHECKED;
        }
        if !on {
            flags |= sys::MF_GRAYED;
        }
        let text = sys::wide(label);
        unsafe {
            sys::AppendMenuW(menu, flags, id, text.as_ptr());
        }
        self.items.push((id, cmd));
    }

    fn command(&self, id: usize) -> Option<Command> {
        self.items
            .iter()
            .find(|(got, _)| *got == id)
            .map(|(_, cmd)| *cmd)
    }

    /// The menu itself, in the order a person reads it: the two decisions the clock cannot show you,
    /// the two you change for an evening, the way out of a corner, then the window.
    fn fill(&mut self, menu: sys::HMENU, st: &MenuState) {
        if let Some(what) = st.notice {
            note(menu, what);
            sep(menu);
        }
        self.item(menu, "Show clock", Command::ToggleVisible, st.visible, true);
        sep(menu);
        self.item(
            menu,
            "Always on top",
            Command::ToggleTopmost,
            st.topmost,
            true,
        );
        let mouse = format!("Mouse: {}", st.click_through.label());
        let sub = submenu(menu, &mouse);
        if !sub.is_null() {
            for c in ClickThrough::ALL {
                self.item(
                    sub,
                    c.label(),
                    Command::SetClickThrough(c),
                    st.click_through == c,
                    true,
                );
            }
        }
        self.item(
            menu,
            "Show seconds",
            Command::ToggleSeconds,
            st.seconds,
            true,
        );
        self.item(menu, "12-hour time", Command::Toggle12Hour, st.hour12, true);
        self.item(
            menu,
            "AM / PM",
            Command::ToggleMeridiem,
            st.meridiem,
            st.hour12,
        );
        let style = format!("Style: {}", st.style.label());
        let sub = submenu(menu, &style);
        if !sub.is_null() {
            for clock_style in ClockStyle::ALL {
                self.item(
                    sub,
                    clock_style.label(),
                    Command::SetStyle(clock_style),
                    st.style == clock_style,
                    true,
                );
            }
        }
        let swing = format!("How it swings: {}", st.posture.label());
        let sub = submenu(menu, &swing);
        if !sub.is_null() {
            for p in PostureKind::ALL {
                self.item(
                    sub,
                    p.label(),
                    Command::SetPosture(p),
                    st.posture == p,
                    true,
                );
            }
        }
        let many = st.monitors.len() > 1;
        let sub = submenu(menu, "Hang from this display");
        if !sub.is_null() {
            for (index, label) in &st.monitors {
                self.item(
                    sub,
                    label,
                    Command::SetMonitor(*index),
                    *index == st.monitor,
                    many,
                );
            }
        }
        sep(menu);
        self.item(menu, "Hang longer", Command::HangUp, false, true);
        self.item(menu, "Hang shorter", Command::HangDown, false, true);
        self.item(menu, "Bigger", Command::Bigger, false, true);
        self.item(menu, "Smaller", Command::Smaller, false, true);
        self.item(menu, "Reset position", Command::ResetPosition, false, true);
        sep(menu);
        self.item(menu, "Settings...", Command::OpenSettings, false, true);
        self.item(
            menu,
            "Start with Windows",
            Command::ToggleLaunchAtLogin,
            st.launch_at_login,
            true,
        );
        sep(menu);
        self.item(menu, "About Hanglock", Command::About, false, true);
        self.item(menu, "Exit Hanglock", Command::Quit, false, true);
    }
}

impl Drop for Tray {
    fn drop(&mut self) {
        self.uninstall();
        if !self.hicon.is_null() {
            unsafe {
                sys::DestroyIcon(self.hicon);
            }
        }
    }
}

/// The `lParam` of a `WM_TRAYICON`: low word is the mouse message, high word the version's id.
#[must_use]
pub fn tray_event(l_param: sys::LPARAM) -> (u32, u32) {
    let v = l_param as u32;
    (v & 0xFFFF, v >> 16)
}
