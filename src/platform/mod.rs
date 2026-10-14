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

/// Keep the independent quick-note window above ordinary desktop windows.
/// GPUI's popup window kind already uses the corresponding elevated level on
/// macOS; its Windows backend creates a tool window but does not make it
/// topmost, so we apply the native z-order there.
pub fn set_window_always_on_top(window: &gpui::Window) {
    #[cfg(windows)]
    {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        if let Ok(handle) = HasWindowHandle::window_handle(window)
            && let RawWindowHandle::Win32(handle) = handle.as_raw()
        {
            const HWND_TOPMOST: isize = -1;
            const SWP_NOSIZE: u32 = 0x0001;
            const SWP_NOMOVE: u32 = 0x0002;
            const SWP_NOACTIVATE: u32 = 0x0010;
            unsafe extern "system" {
                fn SetWindowPos(
                    hwnd: isize,
                    insert_after: isize,
                    x: i32,
                    y: i32,
                    width: i32,
                    height: i32,
                    flags: u32,
                ) -> i32;
            }
            unsafe {
                SetWindowPos(
                    handle.hwnd.get(),
                    HWND_TOPMOST,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                );
            }
        }
    }
    #[cfg(not(windows))]
    let _ = window;
}
