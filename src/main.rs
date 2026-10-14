mod input;
mod mcp;
mod mcp_tools;
mod platform;
mod view;

use anyhow::{Context, Result, bail};
use gpui::{prelude::*, *};
use quire_gpui::session::Session;
use std::path::PathBuf;

fn main() {
    if let Err(error) = run() {
        eprintln!("Quire GPUI: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let mut path = None;
    let mut theme: Option<String> = None;
    let mut dump = false;
    let mut smoke = false;
    let mut stdio = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--db" => path = Some(PathBuf::from(args.next().context("--db 需要路径")?)),
            "--theme" => theme = Some(args.next().context("--theme 需要 ID")?),
            "--dump-state" => dump = true,
            "--smoke-test" => smoke = true,
            "--stdio" | "mcp" => stdio = true,
            "--help" | "-h" => {
                println!(
                    "quire-gpui [--db PATH] [--theme ID] [--dump-state]\nquire-gpui --stdio [--db PATH]: bridge MCP stdio to the running Quire HTTP service. The database path locates its adjacent mcp.json pointer.\n--smoke-test: requires a NEW --db path; exercises application input callbacks, NOT real OS IME."
                );
                return Ok(());
            }
            _ if arg.starts_with("--db=") => path = Some(PathBuf::from(&arg[5..])),
            _ => bail!("未知参数：{arg}"),
        }
    }
    if smoke && (path.is_none() || path.as_ref().is_some_and(|p| p.exists())) {
        bail!("--smoke-test 只允许显式指定尚不存在的 --db 测试库");
    }
    if dump && path.as_ref().is_none_or(|p| !p.exists()) {
        bail!("--dump-state 需要已存在的 --db 库");
    }
    let path = path.unwrap_or_else(|| {
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join("QuireGPUI")
            .join("quire.db")
    });
    let path = std::path::absolute(path)?;
    if stdio {
        return mcp::run_stdio_bridge(&path).map_err(anyhow::Error::msg);
    }
    eprintln!("Quire GPUI database: {}", path.display());
    if dump {
        use quire_core::core::persistence::Repository;
        let repo = quire_core::storage::SqliteRepository::open(&path)?;
        let state = repo.load()?;
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "pages": state.pages.iter().map(|p| serde_json::json!({"id":p.id.0,"title":p.title})).collect::<Vec<_>>(),
                "blocks": state.blocks.iter().map(|b| serde_json::json!({"id":b.id.0,"page":b.page.0,"kind":b.kind.as_str(),"text":b.text})).collect::<Vec<_>>()
            }))?
        );
        return Ok(());
    }
    let session = Session::open(&path)?;
    let sync_listener_warning = sync_listener_preflight(quire_core::services::sync::SYNC_PORT);
    let theme = theme
        .or_else(|| session.setting("gpui.theme").map(str::to_owned))
        .unwrap_or_else(|| "midnight".into());
    platform::set_theme(&theme);
    let (shell_tx, shell_rx) = std::sync::mpsc::channel();
    let platform_services = platform::install(shell_tx);
    let tray_available = platform_services.tray_available;
    Application::new().run(move |cx: &mut App| {
        input::bind_keys(cx);
        view::bind_keys(cx);
        // The Slint reference's window is 1280×800. GPUI's custom frame paints
        // through the invisible resize edge, so the client surface uses that
        // same full window footprint.
        let bounds = restored_window_bounds(&session, cx);
        let window = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("Quire".into()),
                    // The reference uses its own 40px titlebar and window
                    // controls. GPUI's transparent titlebar enables the same
                    // client-side titlebar on Windows while preserving resize.
                    appears_transparent: true,
                    ..Default::default()
                }),
                window_min_size: Some(size(px(924.), px(591.))),
                ..Default::default()
            },
            |window, cx| {
                cx.new(|cx| {
                    view::Workspace::new(
                        session,
                        &theme,
                        smoke,
                        sync_listener_warning.clone(),
                        shell_rx,
                        platform_services,
                        window,
                        cx,
                    )
                })
            },
        );
        match window {
            Ok(window) => {
                let weak = window
                    .update(cx, |view, window, cx| {
                        view.focus_first(window, cx);
                        cx.activate(true);
                        cx.entity().downgrade()
                    })
                    .expect("initial window update");
                window
                    .update(cx, |_, window, cx| {
                        window.on_window_should_close(cx, move |window, cx| {
                            if let Some(view) = weak.upgrade() {
                                view.update(cx, |view, cx| {
                                    if tray_available {
                                        view.hide_to_tray(window, cx);
                                    } else {
                                        view.request_close(window, cx);
                                    }
                                });
                                false
                            } else {
                                true
                            }
                        });
                    })
                    .expect("close handler");
            }
            Err(error) => {
                eprintln!("无法创建窗口：{error:#}");
                cx.quit();
            }
        }
        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
    });
    Ok(())
}

fn restored_window_bounds(session: &Session, cx: &App) -> Bounds<Pixels> {
    let fallback = || Bounds::centered(None, size(px(1280.), px(800.)), cx);
    let Some(bounds) = session
        .setting("window.x")
        .zip(session.setting("window.y"))
        .zip(session.setting("window.w"))
        .zip(session.setting("window.h"))
        .and_then(|(((x, y), width), height)| parse_window_bounds(x, y, width, height))
    else {
        return fallback();
    };
    let visible = cx.displays().into_iter().any(|display| {
        let intersection = bounds.intersect(&display.bounds());
        f32::from(intersection.size.width) >= 160. && f32::from(intersection.size.height) >= 40.
    });
    if visible { bounds } else { fallback() }
}

fn sync_listener_preflight(port: u16) -> Option<String> {
    std::net::TcpListener::bind(("0.0.0.0", port))
        .err()
        .map(|error| {
            format!(
                "同步监听端口 {port} 启动时已被占用：{}。本机可能无法接收其他设备发起的同步，请关闭占用程序后重启 Quire。",
                error
            )
        })
}

fn parse_window_bounds(x: &str, y: &str, width: &str, height: &str) -> Option<Bounds<Pixels>> {
    let (x, y, width, height) = (
        x.parse::<f32>().ok()?,
        y.parse::<f32>().ok()?,
        width.parse::<f32>().ok()?,
        height.parse::<f32>().ok()?,
    );
    if ![x, y, width, height].into_iter().all(f32::is_finite)
        || width < 924.
        || height < 591.
        || width > 16_384.
        || height > 16_384.
    {
        return None;
    }
    Some(Bounds::new(
        point(px(x), px(y)),
        size(px(width), px(height)),
    ))
}

#[cfg(test)]
mod window_geometry_tests {
    use super::{parse_window_bounds, sync_listener_preflight};

    #[test]
    fn saved_window_bounds_accept_valid_position_and_size() {
        let bounds = parse_window_bounds("120", "80", "1440", "900").unwrap();
        assert_eq!(f32::from(bounds.origin.x), 120.);
        assert_eq!(f32::from(bounds.origin.y), 80.);
        assert_eq!(f32::from(bounds.size.width), 1440.);
        assert_eq!(f32::from(bounds.size.height), 900.);
    }

    #[test]
    fn saved_window_bounds_reject_invalid_or_unusable_geometry() {
        for values in [
            ("NaN", "80", "1440", "900"),
            ("0", "0", "923", "900"),
            ("0", "0", "1440", "590"),
            ("0", "0", "20000", "900"),
            ("0", "0", "1440", "bad"),
        ] {
            assert!(parse_window_bounds(values.0, values.1, values.2, values.3).is_none());
        }
    }

    #[test]
    fn sync_listener_preflight_reports_an_occupied_port() {
        let listener = std::net::TcpListener::bind(("0.0.0.0", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let warning = sync_listener_preflight(port).unwrap();
        assert!(warning.contains(&port.to_string()));
        assert!(warning.contains("已被占用"));
    }

    #[test]
    fn sync_listener_preflight_accepts_a_free_port() {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        assert!(sync_listener_preflight(port).is_none());
    }
}
