mod input;
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
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--db" => path = Some(PathBuf::from(args.next().context("--db 需要路径")?)),
            "--theme" => theme = Some(args.next().context("--theme 需要 ID")?),
            "--dump-state" => dump = true,
            "--smoke-test" => smoke = true,
            "--help" | "-h" => {
                println!(
                    "quire-gpui [--db PATH] [--theme ID] [--dump-state]\n--smoke-test: requires a NEW --db path; exercises application input callbacks, NOT real OS IME."
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
    let theme = theme
        .or_else(|| session.setting("gpui.theme").map(str::to_owned))
        .unwrap_or_else(|| "midnight".into());
    Application::new().run(move |cx: &mut App| {
        input::bind_keys(cx);
        view::bind_keys(cx);
        let bounds = Bounds::centered(None, size(px(1180.), px(800.)), cx);
        let window = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("Quire GPUI · 开发预览".into()),
                    appears_transparent: false,
                    ..Default::default()
                }),
                window_min_size: Some(size(px(680.), px(440.))),
                ..Default::default()
            },
            |window, cx| cx.new(|cx| view::Workspace::new(session, &theme, smoke, window, cx)),
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
                                view.update(cx, |view, cx| view.request_close(window, cx));
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
