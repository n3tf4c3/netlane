use anyhow::{Result, bail};
use gpui::Window;
use netlane_windows::desktop::{PanelWindow, Tray, TrayAction, TrayState};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::time::{Duration, Instant};
pub struct Desktop {
    panel: PanelWindow,
    tray: Option<Tray>,
    checked: Instant,
}
impl Desktop {
    pub fn new(window: &Window, state: TrayState) -> Result<Self> {
        let RawWindowHandle::Win32(handle) = HasWindowHandle::window_handle(window)?.as_raw()
        else {
            bail!("Janela Windows indisponível.");
        };
        let panel = PanelWindow::from_handle(handle.hwnd);
        let tray = match Tray::new(state) {
            Ok(tray) => tray,
            Err(error) => {
                panel.restore();
                return Err(error);
            }
        };
        Ok(Self {
            panel,
            tray: Some(tray),
            checked: Instant::now(),
        })
    }
    pub fn available(&self) -> bool {
        self.tray.as_ref().is_some_and(Tray::available)
    }
    pub fn minimized(&self) -> bool {
        self.panel.visible() && self.panel.minimized()
    }
    pub fn hide(&self) {
        self.panel.hide();
    }
    pub fn restore(&self) {
        self.panel.restore();
    }
    pub fn poll(&mut self, state: TrayState) -> Vec<TrayAction> {
        let Some(tray) = &self.tray else {
            return vec![];
        };
        tray.update(state);
        let mut actions = Vec::new();
        while let Some(action) = tray.take_action() {
            actions.push(action);
        }
        if self.checked.elapsed() >= Duration::from_secs(2) {
            self.checked = Instant::now();
            if !tray.check() {
                actions.push(TrayAction::Unavailable);
            }
        }
        if actions.contains(&TrayAction::Unavailable) {
            self.restore();
            self.tray = None;
        }
        actions
    }
    #[cfg(feature = "gui-test")]
    pub fn visible(&self) -> bool {
        self.panel.visible()
    }
    #[cfg(feature = "gui-test")]
    pub fn simulate_failure(&mut self) {
        self.tray.as_ref().unwrap().proof_fail();
    }
    #[cfg(feature = "gui-test")]
    pub fn proof_post_command(&self, command: netlane_windows::desktop::ProofPanelCommand) {
        self.panel.proof_post_command(command);
    }
    #[cfg(feature = "gui-test")]
    pub fn proof_post_action(&self, action: TrayAction) {
        self.tray.as_ref().unwrap().proof_post_action(action);
    }
    #[cfg(feature = "gui-test")]
    pub fn proof_recreate_icon(&self) {
        self.tray.as_ref().unwrap().proof_recreate_icon();
    }
    #[cfg(feature = "gui-test")]
    pub fn proof_maximized(&self) -> bool {
        self.panel.proof_maximized()
    }
}
