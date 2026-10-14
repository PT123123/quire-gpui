use std::sync::mpsc::Sender;

#[derive(Debug)]
pub enum ShellEvent {
    ShowMain,
    QuickNote,
    Quit,
    McpCall(crate::mcp_tools::McpJob),
}

pub type ServiceHandle = Box<dyn std::any::Any + Send>;

pub struct Services {
    pub handles: Vec<ServiceHandle>,
    pub tray_available: bool,
    pub event_tx: Sender<ShellEvent>,
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
            if let Err(error) = hotkeys.set_enabled(hotkeys::ALT_N, true) {
                eprintln!("Quire GPUI: global Alt+N unavailable: {error}");
            }
            services.push(Box::new(hotkeys));
        }
        Err(error) => eprintln!("Quire GPUI: global hotkeys unavailable: {error}"),
    }
    Services {
        handles: services,
        tray_available,
        event_tx,
    }
}

#[cfg(not(windows))]
pub fn install(_tx: Sender<ShellEvent>) -> Services {
    Services {
        handles: Vec::new(),
        tray_available: false,
        event_tx: _tx,
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
