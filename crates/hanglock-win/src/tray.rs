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
use hanglock_core::ids::PostureKind;
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
/// not know what a settings document is.
#[derive(Clone, Copy, Debug, Default)]
pub struct MenuState {
    pub visible: bool,
    pub topmost: bool,
    pub seconds: bool,
    pub hour12: bool,
    pub posture: PostureKind,
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
        self.tooltip = sys::wide(text);
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
    #[must_use]
    pub fn show_menu(&mut self, at: (i32, i32), st: MenuState) -> Option<Command> {
        let mut items: Vec<(usize, Command)> = Vec::with_capacity(16);
        let mut n: usize = 0;
        let menu = unsafe { sys::CreatePopupMenu() };
        if menu.is_null() {
            return None;
        }
        macro_rules! item {
            ($label:expr, $cmd:expr, $checked:expr) => {{
                n += 1;
                let id = ID_BASE + n;
                let flags = if $checked { sys::MF_CHECKED } else { 0 };
                let text = sys::wide($label);
                unsafe {
                    sys::AppendMenuW(menu, flags | sys::MF_STRING, id, text.as_ptr());
                }
                items.push((id, $cmd));
            }};
        }
        macro_rules! sep {
            () => {
                unsafe {
                    sys::AppendMenuW(menu, sys::MF_SEPARATOR, 0, std::ptr::null());
                }
            };
        }
        item!("Show clock", Command::ToggleVisible, st.visible);
        item!("Always on top", Command::ToggleTopmost, st.topmost);
        sep!();
        item!("Show seconds", Command::ToggleSeconds, st.seconds);
        item!("12-hour time", Command::Toggle12Hour, st.hour12);
        sep!();
        item!(
            "Posture: natural",
            Command::SetPosture(PostureKind::Natural),
            st.posture == PostureKind::Natural
        );
        item!(
            "Posture: plate",
            Command::SetPosture(PostureKind::Plate),
            st.posture == PostureKind::Plate
        );
        item!(
            "Posture: mounted",
            Command::SetPosture(PostureKind::Mounted),
            st.posture == PostureKind::Mounted
        );
        item!(
            "Posture: locked",
            Command::SetPosture(PostureKind::Locked),
            st.posture == PostureKind::Locked
        );
        sep!();
        item!("Hang longer", Command::HangUp, false);
        item!("Hang shorter", Command::HangDown, false);
        item!("Bigger", Command::Bigger, false);
        item!("Smaller", Command::Smaller, false);
        item!("Reset position", Command::ResetPosition, false);
        sep!();
        item!("Quit Hanglock", Command::Quit, false);

        // A popup will not dismiss itself on click-away unless its owner window is foreground, and
        // taking foreground away from the user's app is exactly what this overlay must never do. The
        // documented sequence: claim foreground, track, then send a null message so the system
        // releases the capture properly, then hand foreground back. The caret position in the other
        // app is untouched throughout, because we never activate anything.
        unsafe {
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
            sys::DestroyMenu(menu);
            items
                .iter()
                .find(|(id, _)| *id == chosen as usize)
                .map(|(_, c)| *c)
        }
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
