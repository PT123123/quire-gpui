use std::sync::mpsc::Sender;

pub enum ShellEvent {
    ShowMain,
    QuickNote,
    Quit,
    McpCall(crate::mcp_tools::McpJob),
}

pub type ServiceHandle = Box<dyn std::any::Any + Send>;
pub const HOTKEY_ALT_N: u8 = 1;
pub const HOTKEY_ALT_M: u8 = 2;

pub struct Services {
    pub handles: Vec<ServiceHandle>,
    pub tray_available: bool,
    pub event_tx: Sender<ShellEvent>,
    pub set_hotkey: std::sync::Arc<dyn Fn(u8, bool) -> Result<(), String> + Send + Sync>,
}

#[cfg(windows)]
pub fn set_theme(theme: &str) {
    tray::set_theme_mode(tray::theme_mode_of(theme));
}

#[cfg(not(windows))]
pub fn set_theme(_theme: &str) {}

#[cfg(windows)]
mod hotkeys;
#[cfg(windows)]
mod tray;

#[cfg(windows)]
pub fn install(tx: Sender<ShellEvent>) -> Services {
    let event_tx = tx.clone();
    let mut services: Vec<ServiceHandle> = Vec::new();
    let show = tx.clone();
    let tray_available = match tray::install(
        Box::new(move || {
            let _ = show.send(ShellEvent::ShowMain);
        }),
        Box::new({
            let quit = tx.clone();
            move || {
                let _ = quit.send(ShellEvent::Quit);
            }
        }),
    ) {
        Ok(tray) => {
            services.push(Box::new(tray));
            true
        }
        Err(error) => {
            eprintln!("Quire GPUI: tray unavailable: {error}");
            false
        }
    };
    let mut set_hotkey: std::sync::Arc<dyn Fn(u8, bool) -> Result<(), String> + Send + Sync> =
        std::sync::Arc::new(|_, _| Err("global hotkeys are unavailable".into()));
    match hotkeys::install(
        Box::new({
            let quick = tx.clone();
            move || {
                let _ = quick.send(ShellEvent::QuickNote);
            }
        }),
        Box::new(move || {
            let _ = tx.send(ShellEvent::ShowMain);
        }),
    ) {
        Ok(hotkeys) => {
            let hotkeys = std::sync::Arc::new(hotkeys);
            let control = hotkeys.clone();
            set_hotkey = std::sync::Arc::new(move |id, on| control.set_enabled(id, on));
            services.push(Box::new(hotkeys));
        }
        Err(error) => eprintln!("Quire GPUI: global hotkeys unavailable: {error}"),
    }
    Services {
        handles: services,
        tray_available,
        event_tx,
        set_hotkey,
    }
}

#[cfg(not(windows))]
pub fn install(_tx: Sender<ShellEvent>) -> Services {
    Services {
        handles: Vec::new(),
        tray_available: false,
        event_tx: _tx,
        set_hotkey: std::sync::Arc::new(|_, _| Err("global hotkeys are Windows-only".into())),
    }
}

pub fn set_main_window_visible(window: &gpui::Window, visible: bool) {
    #[cfg(windows)]
    {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        if let Ok(handle) = HasWindowHandle::window_handle(window)
            && let RawWindowHandle::Win32(handle) = handle.as_raw()
        {
            let command = if visible { 5 } else { 0 };
            unsafe extern "system" {
                fn ShowWindow(hwnd: isize, command: i32) -> i32;
            }
            unsafe {
                ShowWindow(handle.hwnd.get(), command);
            }
        }
    }
    #[cfg(not(windows))]
    let _ = (window, visible);
}
