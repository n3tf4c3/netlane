//! Native tray lifetime is separate from the panel and routing session.
//! All methods and callbacks stay on the creating UI thread.
use anyhow::{Result, ensure};
use std::{
    cell::{Cell, RefCell},
    marker::PhantomData,
    ptr::{null, null_mut},
    rc::Rc,
    sync::mpsc::{self, Receiver, Sender},
};
use windows_sys::Win32::{
    Foundation::{ERROR_CLASS_ALREADY_EXISTS, GetLastError, HWND, LPARAM, LRESULT, POINT, WPARAM},
    System::LibraryLoader::GetModuleHandleW,
    UI::{Shell::*, WindowsAndMessaging::*},
};

const CALLBACK: u32 = WM_APP + 17;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayAction {
    Open,
    Diagnostics,
    Start,
    Stop,
    Exit,
    Unavailable,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TrayState {
    pub status: String,
    pub enabled_rules: usize,
    pub total_rules: usize,
    pub dirty: bool,
    pub can_start: bool,
    pub can_stop: bool,
    pub can_exit: bool,
}
impl TrayState {
    fn menu(&self) -> Vec<(usize, String, bool)> {
        vec![
            (0, self.status.clone(), false),
            (
                0,
                format!(
                    "Regras habilitadas: {} de {}",
                    self.enabled_rules, self.total_rules
                ),
                false,
            ),
            (
                0,
                if self.dirty {
                    "Alterações não salvas"
                } else {
                    "Regras salvas"
                }
                .into(),
                false,
            ),
            (1, "Abrir NetLane".into(), true),
            (2, "Diagnóstico".into(), true),
            (3, "Iniciar sessão…".into(), self.can_start),
            (4, "Parar com segurança".into(), self.can_stop),
            (5, "Sair".into(), self.can_exit),
        ]
    }
}
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
fn action(id: usize) -> Option<TrayAction> {
    match id {
        1 => Some(TrayAction::Open),
        2 => Some(TrayAction::Diagnostics),
        3 => Some(TrayAction::Start),
        4 => Some(TrayAction::Stop),
        5 => Some(TrayAction::Exit),
        _ => None,
    }
}
struct CallbackState {
    hwnd: Cell<HWND>,
    icon: HICON,
    available: Cell<bool>,
    taskbar_created: u32,
    model: RefCell<TrayState>,
    sender: Sender<TrayAction>,
}
impl CallbackState {
    fn data(&self) -> NOTIFYICONDATAW {
        let mut data = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: self.hwnd.get(),
            uID: 1,
            uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_SHOWTIP,
            uCallbackMessage: CALLBACK,
            hIcon: self.icon,
            ..Default::default()
        };
        let tip = wide("NetLane · Cada app, sua rota");
        data.szTip[..tip.len()].copy_from_slice(&tip);
        data
    }
    fn add(&self) -> bool {
        let mut data = self.data();
        // SAFETY: initialized data references this live hidden window and shared module icon.
        let added = unsafe { Shell_NotifyIconW(NIM_ADD, &data) } != 0;
        if !added {
            return false;
        }
        data.Anonymous.uVersion = NOTIFYICON_VERSION_4;
        // SAFETY: the icon with this window/id was added above.
        if unsafe { Shell_NotifyIconW(NIM_SETVERSION, &data) } == 0 {
            self.remove();
            return false;
        }
        true
    }
    fn remove(&self) {
        // SAFETY: initialized data identifies only this process's own tray icon.
        unsafe {
            Shell_NotifyIconW(NIM_DELETE, &self.data());
        }
        self.available.set(false);
    }
    fn popup(&self) {
        // Clone first: no RefCell borrow survives the native menu's nested message loop.
        let rows = self.model.borrow().menu();
        // SAFETY: CreatePopupMenu takes no pointers and returns an owned handle.
        let menu = unsafe { CreatePopupMenu() };
        if menu.is_null() {
            return;
        }
        let mut valid = true;
        for (id, label, enabled) in rows {
            let text = wide(&label);
            // SAFETY: menu is live and text is nul terminated for the duration of this call.
            valid &= unsafe {
                AppendMenuW(
                    menu,
                    MF_STRING | if enabled { MF_ENABLED } else { MF_GRAYED },
                    id,
                    text.as_ptr(),
                )
            } != 0;
        }
        let mut point = POINT::default();
        // SAFETY: point is writable; hwnd/menu are owned by this UI thread.
        let selection = unsafe {
            if valid && GetCursorPos(&mut point) != 0 {
                SetForegroundWindow(self.hwnd.get());
                let id = TrackPopupMenu(
                    menu,
                    TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON,
                    point.x,
                    point.y,
                    0,
                    self.hwnd.get(),
                    null(),
                );
                PostMessageW(self.hwnd.get(), WM_NULL, 0, 0);
                id as usize
            } else {
                0
            }
        };
        // SAFETY: menu is owned and TrackPopupMenu returned; no menu pointers remain.
        unsafe {
            DestroyMenu(menu);
            Shell_NotifyIconW(NIM_SETFOCUS, &self.data());
        }
        if let Some(action) = action(selection) {
            let _ = self.sender.send(action);
        }
    }
}
unsafe extern "system" fn window_proc(hwnd: HWND, message: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    if message == WM_NCCREATE {
        // SAFETY: Windows supplies a CREATESTRUCTW during WM_NCCREATE. The Rc passed
        // as lpCreateParams remains alive until after DestroyWindow in Tray::drop.
        unsafe {
            let create = &*(l as *const CREATESTRUCTW);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
        }
    }
    // SAFETY: this class stores only our stable Rc pointer in GWLP_USERDATA.
    let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *const CallbackState;
    if !ptr.is_null() {
        // SAFETY: pointer comes from Rc::as_ptr and Tray still owns a strong reference
        // when each callback begins. A temporary strong reference also keeps it alive
        // if the native menu's nested loop causes Tray to be dropped before returning.
        let state = unsafe {
            Rc::increment_strong_count(ptr);
            Rc::from_raw(ptr)
        };
        #[cfg(feature = "desktop-test")]
        if message == WM_APP + 18 {
            if let Some(action) = action(w) {
                let _ = state.sender.send(action);
            }
            return 0;
        }
        if message == state.taskbar_created && state.taskbar_created != 0 {
            state.available.set(state.add());
            if !state.available.get() {
                let _ = state.sender.send(TrayAction::Unavailable);
            }
            return 0;
        }
        if message == CALLBACK {
            let event = (l as u32) & 0xffff;
            if event == WM_CONTEXTMENU {
                state.popup();
            } else if event == NIN_SELECT || event == (NIN_SELECT | 1) || event == WM_LBUTTONUP {
                let _ = state.sender.send(TrayAction::Open);
            }
            return 0;
        }
        if message == WM_NCDESTROY {
            // SAFETY: clear our pointer before allowing final native destruction.
            unsafe {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            }
        }
    }
    // SAFETY: forwards untouched native message arguments to the default procedure.
    unsafe { DefWindowProcW(hwnd, message, w, l) }
}

pub struct Tray {
    state: Rc<CallbackState>,
    receiver: Receiver<TrayAction>,
    // Native callbacks and RefCell access must never move to another thread.
    _ui_thread: PhantomData<Rc<()>>,
}
impl Tray {
    pub fn new(model: TrayState) -> Result<Self> {
        let class = wide("NetLaneRust.Tray.v1");
        // SAFETY: null requests the current loaded executable module.
        let module = unsafe { GetModuleHandleW(null()) };
        ensure!(!module.is_null(), "Módulo da bandeja indisponível.");
        let descriptor = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: module,
            lpszClassName: class.as_ptr(),
            ..Default::default()
        };
        // SAFETY: descriptor/class stay alive for registration; Windows copies the name.
        let registered = unsafe { RegisterClassW(&descriptor) } != 0;
        // SAFETY: GetLastError reads this thread's last Win32 result immediately after registration.
        let existing = unsafe { GetLastError() } == ERROR_CLASS_ALREADY_EXISTS;
        ensure!(registered || existing, "Classe da bandeja indisponível.");
        // SAFETY: MAKEINTRESOURCE(1) names our embedded icon; LoadIcon returns a shared icon.
        let icon = unsafe { LoadIconW(module, std::ptr::without_provenance(1)) };
        ensure!(!icon.is_null(), "Ícone da bandeja indisponível.");
        let (sender, receiver) = mpsc::channel();
        // SAFETY: nul-terminated literal is read only during registration.
        let taskbar_created = unsafe { RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()) };
        ensure!(taskbar_created != 0, "Mensagem da bandeja indisponível.");
        let state = Rc::new(CallbackState {
            hwnd: Cell::new(null_mut()),
            icon,
            available: Cell::new(false),
            taskbar_created,
            model: RefCell::new(model),
            sender,
        });
        // SAFETY: stable Rc pointer outlives the window; top-level hidden window (not
        // HWND_MESSAGE) receives Explorer's broadcast, and has no taskbar button.
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_TOOLWINDOW,
                class.as_ptr(),
                wide("NetLane Rust Tray").as_ptr(),
                WS_POPUP,
                0,
                0,
                0,
                0,
                null_mut(),
                null_mut(),
                module,
                Rc::as_ptr(&state).cast(),
            )
        };
        ensure!(!hwnd.is_null(), "Janela da bandeja indisponível.");
        state.hwnd.set(hwnd);
        let tray = Self {
            state,
            receiver,
            _ui_thread: PhantomData,
        };
        tray.state.available.set(tray.state.add());
        ensure!(
            tray.available(),
            "O Windows não confirmou o ícone da bandeja."
        );
        Ok(tray)
    }
    pub fn available(&self) -> bool {
        self.state.available.get()
    }
    pub fn take_action(&self) -> Option<TrayAction> {
        self.receiver.try_recv().ok()
    }
    pub fn update(&self, model: TrayState) {
        *self.state.model.borrow_mut() = model;
    }
    pub fn check(&self) -> bool {
        if !self.available() {
            return false;
        }
        // SAFETY: identifies our own icon/window only; checks Explorer still accepts it.
        if unsafe { Shell_NotifyIconW(NIM_MODIFY, &self.state.data()) } == 0 {
            self.state.remove();
            return false;
        }
        true
    }
    #[cfg(feature = "desktop-test")]
    pub fn proof_post_action(&self, action: TrayAction) {
        let id = match action {
            TrayAction::Open => 1,
            TrayAction::Diagnostics => 2,
            TrayAction::Start => 3,
            TrayAction::Stop => 4,
            TrayAction::Exit => 5,
            TrayAction::Unavailable => 0,
        };
        // SAFETY: posts a fixture event only to this live tray window, not the desktop.
        unsafe {
            PostMessageW(self.state.hwnd.get(), WM_APP + 18, id, 0);
        }
    }
    #[cfg(feature = "desktop-test")]
    pub fn proof_recreate_icon(&self) {
        self.state.remove();
        // SAFETY: simulates TaskbarCreated only for this own window; Explorer is not restarted.
        unsafe {
            PostMessageW(self.state.hwnd.get(), self.state.taskbar_created, 0, 0);
        }
    }
    #[cfg(feature = "desktop-test")]
    pub fn proof_fail(&self) {
        self.state.remove();
        let _ = self.state.sender.send(TrayAction::Unavailable);
    }
}
impl Drop for Tray {
    fn drop(&mut self) {
        self.state.remove();
        // SAFETY: this UI thread owns the window and the callback Rc is still alive.
        unsafe {
            DestroyWindow(self.state.hwnd.get());
        }
        // LoadIcon's shared module resource must not be destroyed.
    }
}

/// Borrowed native panel handle, used only while its GPUI window is alive.
pub struct PanelWindow {
    hwnd: HWND,
    _ui_thread: PhantomData<Rc<()>>,
}
#[cfg(feature = "desktop-test")]
pub enum ProofPanelCommand {
    Close,
    Minimize,
    Maximize,
}
impl PanelWindow {
    /// Caller keeps the original GPUI window alive for this wrapper's lifetime.
    pub fn from_handle(hwnd: std::num::NonZeroIsize) -> Self {
        Self {
            hwnd: hwnd.get() as HWND,
            _ui_thread: PhantomData,
        }
    }
    pub fn hide(&self) {
        // SAFETY: ShowWindow accepts a window handle and never dereferences Rust memory.
        unsafe {
            ShowWindow(self.hwnd, SW_HIDE);
        }
    }
    pub fn restore(&self) {
        // SAFETY: operates only on the borrowed panel HWND; SW_RESTORE respects the
        // previous maximized placement of a minimized panel, SW_SHOW preserves hidden placement.
        unsafe {
            ShowWindow(
                self.hwnd,
                if IsIconic(self.hwnd) != 0 {
                    SW_RESTORE
                } else {
                    SW_SHOW
                },
            );
            SetForegroundWindow(self.hwnd);
        }
    }
    pub fn minimized(&self) -> bool {
        // SAFETY: IsIconic accepts the borrowed panel handle.
        unsafe { IsIconic(self.hwnd) != 0 }
    }
    pub fn visible(&self) -> bool {
        // SAFETY: IsWindowVisible accepts the borrowed panel handle.
        unsafe { IsWindowVisible(self.hwnd) != 0 }
    }
    #[cfg(feature = "desktop-test")]
    pub fn proof_post_command(&self, command: ProofPanelCommand) {
        let (message, value) = match command {
            ProofPanelCommand::Close => (WM_CLOSE, 0),
            ProofPanelCommand::Minimize => (WM_SYSCOMMAND, SC_MINIMIZE),
            ProofPanelCommand::Maximize => (WM_SYSCOMMAND, SC_MAXIMIZE),
        };
        // SAFETY: posts only to the panel borrowed by this instance, after the GPUI update returns.
        unsafe {
            PostMessageW(self.hwnd, message, value as usize, 0);
        }
    }
    #[cfg(feature = "desktop-test")]
    pub fn proof_maximized(&self) -> bool {
        // SAFETY: query only this borrowed panel handle.
        unsafe { IsZoomed(self.hwnd) != 0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn menu_keeps_stop_available_with_dirty_rules_and_blocks_start_and_exit_while_busy() {
        let state = TrayState {
            dirty: true,
            enabled_rules: 2,
            total_rules: 3,
            can_stop: true,
            ..Default::default()
        };
        let rows = state.menu();
        assert_eq!(rows[1].1, "Regras habilitadas: 2 de 3");
        assert_eq!(rows[2].1, "Alterações não salvas");
        assert!(!rows[5].2);
        assert!(rows[6].2);
        assert!(!rows[7].2);
        assert_eq!(action(5), Some(TrayAction::Exit));
        assert_eq!(action(0), None);
    }
}
