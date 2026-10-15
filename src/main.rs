mod input;
mod mcp;
mod mcp_tools;
mod platform;
mod view;

use anyhow::{Context, Result, bail};
use gpui::{prelude::*, *};
use quire_core::core::command::Command;
use quire_core::core::types::BlockId;
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
    let mut smoke_recovery = false;
    let mut smoke_blocks = 0usize;
    let mut stdio = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--db" => path = Some(PathBuf::from(args.next().context("--db 需要路径")?)),
            "--theme" => theme = Some(args.next().context("--theme 需要 ID")?),
            "--dump-state" => dump = true,
            "--smoke-test" => smoke = true,
            "--smoke-recovery" => smoke_recovery = true,
            "--smoke-blocks" => {
                smoke_blocks = args
                    .next()
                    .context("--smoke-blocks 需要数量")?
                    .parse()
                    .context("--smoke-blocks 需要正整数")?;
            }
            "--stdio" | "mcp" => stdio = true,
            "--help" | "-h" => {
                println!(
                    "quire-gpui [--db PATH] [--theme ID] [--dump-state]\nquire-gpui --stdio [--db PATH]: bridge MCP stdio to the running Quire HTTP service. The database path locates its adjacent mcp.json pointer.\n--smoke-test: requires a NEW --db path and refuses an existing adjacent mcp.json; exercises isolated UI/MCP paths, NOT real OS IME.\n--smoke-recovery: with --smoke-test only, requires an existing isolated database that recovers from a backup.\n--smoke-blocks N: with --smoke-test, add N synthetic paragraph blocks (max 10000) to the isolated test page."
                );
                return Ok(());
            }
            _ if arg.starts_with("--db=") => path = Some(PathBuf::from(&arg[5..])),
            _ => bail!("未知参数：{arg}"),
        }
    }
    if smoke_recovery && !smoke {
        bail!("--smoke-recovery 只能与 --smoke-test 同用");
    }
    if smoke
        && (path.is_none()
            || path
                .as_ref()
                .is_some_and(|path| path.exists() != smoke_recovery))
    {
        bail!(
            "--smoke-test 只允许显式隔离 --db；恢复模式须指定已存在的测试库，其余 smoke 须指定新路径"
        );
    }
    if smoke_blocks > 10_000 {
        bail!("--smoke-blocks 最多支持 10000 块");
    }
    if smoke_blocks > 0 && !smoke {
        bail!("--smoke-blocks 只能与 --smoke-test 同用");
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
    if smoke
        && path
            .parent()
            .is_some_and(|parent| crate::mcp::pointer_path(parent).exists())
    {
        bail!("--smoke-test 拒绝覆盖数据库目录中已有的 MCP mcp.json 指针");
    }
    if stdio {
        return mcp::run_stdio_bridge(&path).map_err(anyhow::Error::msg);
    }
    eprintln!("Quire GPUI database: {}", path.display());
    if dump {
        use quire_core::core::persistence::Repository;
        let repo = quire_core::storage::SqliteRepository::open(&path)?;
        let state = repo.load()?;
        let organizer = repo.load_organizer()?;
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "pages": state.pages.iter().map(|p| serde_json::json!({"id":p.id.0,"title":p.title})).collect::<Vec<_>>(),
                "blocks": state.blocks.iter().map(|b| serde_json::json!({"id":b.id.0,"page":b.page.0,"kind":b.kind.as_str(),"text":b.text})).collect::<Vec<_>>(),
                "notes": organizer.live_notes().map(|note| serde_json::json!({"id":note.id.0,"title":note.title,"body":note.body,"tags":note.tags})).collect::<Vec<_>>()
            }))?
        );
        return Ok(());
    }
    let mut session = match Session::open(&path) {
        Ok(session) => session,
        Err(error) => {
            crate::platform::show_startup_error(&format!(
                "无法打开 Quire 数据库：\n{}\n\n程序不会覆盖或重建该文件。请检查文件权限、磁盘状态，或从备份恢复。",
                error
            ));
            return Err(error);
        }
    };
    if smoke_recovery && session.startup_notice.is_none() {
        bail!("--smoke-recovery 数据库没有触发备份恢复");
    }
    if smoke_recovery {
        eprintln!("SMOKE: isolated database recovery warning was received");
    }
    if smoke {
        // Exercise the persisted startup preference in the same isolated
        // database as the rest of the window smoke. This also makes the later
        // platform quick-note event verify that repeated activation reuses the
        // startup-created popup instead of opening a second draft.
        session.set_setting("gpui.quick-note-on-start", "true");
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0))?;
        let mcp_port = listener.local_addr()?.port();
        drop(listener);
        session.set_setting("gpui.mcp.port", mcp_port.to_string());
        session.set_setting("gpui.mcp.token", "quire-gpui-isolated-smoke-token");
        session.persistence.force_flush()?;
    }
    let mut smoke_board_block = None;
    if smoke && !smoke_recovery {
        let database_command = Command::AppendBlock {
            kind: quire_core::core::types::BlockKind::Database,
            text: String::new(),
        };
        session.execute(database_command);
        let board_block = session
            .blocks()
            .last()
            .context("cannot create isolated board smoke block")?
            .id;
        smoke_board_block = Some(board_block);
        let database = session
            .make_database(board_block, "看板渲染回归".into())
            .context("cannot seed isolated board database")?;
        let title_property = session
            .databases
            .title_property(database)
            .context("board smoke database is missing its title property")?
            .id;
        let checkbox_property = session
            .add_database_property(
                board_block,
                "完成".into(),
                quire_core::core::database::PropertyKind::Checkbox,
            )
            .context("cannot add the board smoke group property")?;
        let date_property = session
            .add_database_property(
                board_block,
                "日期".into(),
                quire_core::core::database::PropertyKind::Date,
            )
            .context("cannot add the calendar smoke property")?;
        let view = session
            .databases
            .views_of(database)
            .next()
            .context("board smoke database is missing its view")?
            .id;
        let mut definition = quire_core::core::database_view::ViewDefinition::new();
        definition.set_group(Some(checkbox_property));
        session.set_database_view_definition(board_block, view, definition.to_text())?;
        session.set_database_view_layout(
            board_block,
            view,
            quire_core::core::database::ViewLayout::Board,
        )?;
        for (title, checked) in [("未完成示例", "false"), ("已完成示例", "true")] {
            let record = session
                .add_database_record(board_block)
                .context("cannot add a board smoke record")?;
            session.set_database_cell(board_block, record, title_property, title)?;
            session.set_database_cell(board_block, record, checkbox_property, checked)?;
            session.set_database_cell(board_block, record, date_property, "2026-10-15T14:30")?;
        }
        session.persistence.force_flush()?;
        eprintln!("Seeded a grouped, two-column board in the isolated smoke database");
    }
    if smoke_blocks > 0 {
        let anchor = smoke_board_block
            .or_else(|| session.blocks().first().map(|block| block.id))
            .context("测试页没有可用的插入位置")?;
        let template = session
            .blocks()
            .iter()
            .find(|block| block.kind == quire_core::core::types::BlockKind::Paragraph)
            .context("测试页没有可用的段落模板")?
            .clone();
        let mut template = template;
        template.parent = None;
        template.marks.clear();
        let blocks = (0..smoke_blocks)
            .map(|index| {
                let mut block = template.clone();
                block.id = BlockId(u64::MAX - index as u64);
                block.text = format!("Synthetic virtualized paragraph {}", index + 1);
                block
            })
            .collect();
        session.execute(Command::InsertForest {
            anchor: Some(anchor),
            blocks,
        });
        eprintln!("Seeded {smoke_blocks} synthetic blocks in the isolated test database");
    } else if smoke_board_block.is_some() {
        session.execute(Command::AppendBlock {
            kind: quire_core::core::types::BlockKind::Paragraph,
            text: "隔离看板 smoke 锚点".into(),
        });
    }
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
                        tray_available,
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
