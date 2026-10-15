use std::path::Path;
use std::sync::mpsc::Sender;

pub enum ShellEvent {
    ShowMain,
    QuickNote,
    Quit,
    McpCall(crate::mcp_tools::McpJob),
}

pub fn open_folder(path: &Path) -> Result<(), String> {
    #[cfg(windows)]
    let mut command = std::process::Command::new("explorer.exe");
    #[cfg(target_os = "macos")]
    let mut command = std::process::Command::new("open");
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = std::process::Command::new("xdg-open");
    command
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("无法打开目录 {}：{error}", path.display()))
}

pub type ServiceHandle = Box<dyn std::any::Any + Send>;
pub const HOTKEY_ALT_N: u8 = 1;
pub const HOTKEY_ALT_M: u8 = 2;

/// Make fatal database startup failures visible when the application is
/// launched without a console. The detailed anyhow chain is still printed by
/// `main` for diagnostics.
pub fn show_startup_error(message: &str) {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        let text = std::ffi::OsStr::new(message)
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>();
        let title = std::ffi::OsStr::new("Quire GPUI — 无法启动")
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>();
        unsafe extern "system" {
            fn MessageBoxW(hwnd: isize, text: *const u16, title: *const u16, kind: u32) -> i32;
        }
        const MB_OK: u32 = 0x0000;
        const MB_ICONERROR: u32 = 0x0010;
        const MB_TASKMODAL: u32 = 0x2000;
        unsafe {
            MessageBoxW(
                0,
                text.as_ptr(),
                title.as_ptr(),
                MB_OK | MB_ICONERROR | MB_TASKMODAL,
            );
        }
    }
    #[cfg(not(windows))]
    eprintln!("Quire GPUI startup error: {message}");
}

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

/// Give the GPUI build a stable Windows taskbar identity separate from Slint Quire.
#[cfg(windows)]
pub fn set_application_id() {
    use std::os::windows::ffi::OsStrExt;

    let app_id = std::ffi::OsStr::new("PT123123.Quire.GPUI")
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    #[link(name = "shell32")]
    unsafe extern "system" {
        fn SetCurrentProcessExplicitAppUserModelID(app_id: *const u16) -> i32;
    }
    let result = unsafe { SetCurrentProcessExplicitAppUserModelID(app_id.as_ptr()) };
    if result < 0 {
        eprintln!("Quire GPUI: could not set Windows AppUserModelID (HRESULT {result:#x})");
    }
}

#[cfg(not(windows))]
pub fn set_application_id() {}

#[cfg(windows)]
mod hotkeys;
#[cfg(windows)]
mod tray;

#[cfg(windows)]
pub fn install(tx: Sender<ShellEvent>) -> Services {
    let event_tx = tx.clone();
    let mut services: Vec<ServiceHandle> = Vec::new();
    let show = tx.clone();
    let quick_note = tx.clone();
    let tray_available = match tray::install(
        Box::new(move || {
            let _ = show.send(ShellEvent::ShowMain);
        }),
        Box::new(move || {
            let _ = quick_note.send(ShellEvent::QuickNote);
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

/// Drive the native maximize/restore state, primarily for the isolated desktop smoke path.
pub fn set_main_window_maximized(window: &gpui::Window, maximized: bool) {
    #[cfg(windows)]
    {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        if let Ok(handle) = HasWindowHandle::window_handle(window)
            && let RawWindowHandle::Win32(handle) = handle.as_raw()
        {
            const SW_RESTORE: i32 = 9;
            const SW_MAXIMIZE: i32 = 3;
            unsafe extern "system" {
                fn ShowWindow(hwnd: isize, command: i32) -> i32;
            }
            unsafe {
                ShowWindow(
                    handle.hwnd.get(),
                    if maximized { SW_MAXIMIZE } else { SW_RESTORE },
                );
            }
        }
    }
    #[cfg(not(windows))]
    let _ = (window, maximized);
}

pub fn main_window_visible(window: &gpui::Window) -> bool {
    #[cfg(windows)]
    {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        if let Ok(handle) = HasWindowHandle::window_handle(window)
            && let RawWindowHandle::Win32(handle) = handle.as_raw()
        {
            unsafe extern "system" {
                fn IsWindowVisible(hwnd: isize) -> i32;
            }
            return unsafe { IsWindowVisible(handle.hwnd.get()) != 0 };
        }
        false
    }
    #[cfg(not(windows))]
    {
        let _ = window;
        true
    }
}

/// Open a local file with the operating system's registered default handler.
pub fn open_with_default(path: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        let path = path
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>();
        let operation = "open".encode_utf16().chain(Some(0)).collect::<Vec<_>>();
        unsafe extern "system" {
            fn ShellExecuteW(
                hwnd: isize,
                operation: *const u16,
                file: *const u16,
                parameters: *const u16,
                directory: *const u16,
                show: i32,
            ) -> isize;
        }
        let result = unsafe {
            ShellExecuteW(
                0,
                operation.as_ptr(),
                path.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                1,
            )
        };
        if result as usize <= 32 {
            Err(format!(
                "Windows 无法打开文件（错误码 {}）",
                result as usize
            ))
        } else {
            Ok(())
        }
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
    #[cfg(all(not(windows), not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
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
