use quire_core::services::sync::{
    engine::{DeviceInfo, Job, PeerRecord},
    model::SyncSnapshot,
    server::SyncServer,
    transport,
};
use quire_core::{core::command::Command, core::types::AttachmentId};
use quire_gpui::session::Session;
use std::net::TcpListener;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

fn start_node(session: &mut Session) -> (u16, Receiver<Job>) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind isolated sync endpoint");
    let port = listener.local_addr().unwrap().port();
    let mut info = session.sync_self_info();
    info.port = port;
    let (job_tx, job_rx) = mpsc::channel();
    let server = SyncServer {
        self_info: info,
        tx: job_tx,
    };
    std::thread::spawn(move || server.serve(listener));
    (port, job_rx)
}

fn answer_job(session: &mut Session, job: Job) {
    match job {
        Job::ExportSnapshot { reply } => {
            reply
                .send(session.sync_export().expect("export complete snapshot"))
                .unwrap();
        }
        Job::ApplyRemote {
            peer,
            snapshot,
            bytes,
            reply,
        } => {
            let result = session
                .sync_apply_remote(&snapshot, &bytes, &peer)
                .map_err(|error| error.to_string());
            reply.send(result).unwrap();
        }
        Job::ListLocalAttachments { reply } => {
            reply
                .send(session.attachments.keys().copied().collect())
                .unwrap();
        }
        Job::AttachmentBytes { id, reply } => {
            let bytes = session
                .attachment_path(AttachmentId(id))
                .and_then(|path| std::fs::read(path).ok())
                .unwrap_or_default();
            reply.send(bytes).unwrap();
        }
        Job::InboundPair { device, ip, reply } => {
            session.sync_note_device(&device.id, &device.name, &device.kind, &ip, device.port);
            reply.send(true).unwrap();
        }
        _ => panic!("unexpected sync job in loopback test"),
    }
}

fn request_while_pumping<T: Send + 'static>(
    jobs: &Receiver<Job>,
    session: &mut Session,
    request: impl FnOnce(Sender<Result<T, String>>) + Send + 'static,
) -> Result<T, String> {
    let (done_tx, done_rx) = mpsc::channel();
    std::thread::spawn(move || request(done_tx));
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match done_rx.try_recv() {
            Ok(result) => return result,
            Err(mpsc::TryRecvError::Disconnected) => {
                return Err("loopback request worker ended without a response".into());
            }
            Err(mpsc::TryRecvError::Empty) => {}
        }
        match jobs.recv_timeout(Duration::from_millis(10)) {
            Ok(job) => answer_job(session, job),
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err("sync server closed its job channel".into());
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        if Instant::now() >= deadline {
            return Err("loopback sync endpoint timed out".into());
        }
    }
}

#[test]
fn two_isolated_sessions_exchange_changes_over_sync_http_and_reopen() -> anyhow::Result<()> {
    let scratch_a = quire_core::testing::ScratchDir::new("gpui-sync-node-a");
    let scratch_b = quire_core::testing::ScratchDir::new("gpui-sync-node-b");
    let path_a = scratch_a.path().join("node-a.db");
    let path_b = scratch_b.path().join("node-b.db");
    let mut a = Session::open(&path_a)?;
    let mut b = Session::open(&path_b)?;

    let note_a = a.create_note();
    a.update_note_editor(
        note_a,
        "来自 A 的笔记".into(),
        "经由同步 HTTP 往返".into(),
        vec!["loopback".into()],
    );
    let page_b = b.create_page(None);
    b.set_page_title("来自 B 的页面".into());
    let database_block_b = b.blocks().first().unwrap().id;
    let database_b = b
        .make_database(database_block_b, "来自 B 的数据库".into())
        .unwrap();
    let record_b = b.add_database_record(database_block_b).unwrap();
    let property_b = b
        .add_database_property(
            database_block_b,
            "备注".into(),
            quire_core::core::database::PropertyKind::Text,
        )
        .unwrap();
    let files_property_b = b
        .add_database_property(
            database_block_b,
            "附件".into(),
            quire_core::core::database::PropertyKind::Files,
        )
        .unwrap();
    b.set_database_cell(database_block_b, record_b, property_b, "数据库单元同步")?;
    let database_attachment_fixture = scratch_b.path().join("database-attachment.bin");
    std::fs::write(
        &database_attachment_fixture,
        b"database loopback file\0\xff",
    )?;
    assert!(b.import_database_files(
        database_block_b,
        record_b,
        files_property_b,
        std::slice::from_ref(&database_attachment_fixture)
    )?);
    let database_attachment_id = match b.repo.cell(record_b, files_property_b)? {
        quire_core::core::database::CellValue::Items(ids) if ids.len() == 1 => {
            ids[0].parse::<u64>().expect("database attachment id")
        }
        value => panic!("unexpected database files cell: {value:?}"),
    };
    b.execute(Command::AppendBlock {
        kind: quire_core::core::types::BlockKind::Paragraph,
        text: "来自节点 B 的正文".into(),
    });
    let block_b = b
        .blocks()
        .iter()
        .find(|block| block.text == "来自节点 B 的正文")
        .unwrap()
        .id;
    let attachment_fixture = scratch_b.path().join("sync-attachment.txt");
    std::fs::write(&attachment_fixture, b"loopback attachment bytes")?;
    let attachment_block_b = b.import_attachment(block_b, &attachment_fixture, false)?;
    let attachment_b = b
        .blocks()
        .iter()
        .find(|block| block.id == attachment_block_b)
        .and_then(|block| block.attachment)
        .expect("imported file block references its attachment");

    let (port_a, _jobs_a) = start_node(&mut a);
    let (port_b, jobs_b) = start_node(&mut b);
    let (status, info_bytes) =
        transport::http_get("127.0.0.1", port_b, "/sync/info", Duration::from_secs(3))
            .map_err(anyhow::Error::msg)?;
    assert_eq!(status, 200);
    let info_b =
        DeviceInfo::from_json(&String::from_utf8(info_bytes)?).map_err(anyhow::Error::msg)?;
    assert_eq!(info_b.port, port_b);
    let peer_b = PeerRecord {
        id: info_b.id.clone(),
        name: info_b.name.clone(),
        kind: info_b.kind.clone(),
        ip: "127.0.0.1".into(),
        port: port_b,
        paired: true,
        last_seen: 0,
        last_sync: String::new(),
    };

    let jobs_b_snapshot = jobs_b;
    let (status, info_a_bytes) =
        transport::http_get("127.0.0.1", port_a, "/sync/info", Duration::from_secs(3))
            .map_err(anyhow::Error::msg)?;
    assert_eq!(status, 200);
    let info_a =
        DeviceInfo::from_json(&String::from_utf8(info_a_bytes)?).map_err(anyhow::Error::msg)?;
    let pair_body = info_a.to_json();
    let (status, body) = request_while_pumping(&jobs_b_snapshot, &mut b, move |reply| {
        let _ = reply.send(transport::http_post(
            "127.0.0.1",
            port_b,
            "/sync/pair",
            &pair_body,
            Duration::from_secs(3),
        ));
    })
    .map_err(anyhow::Error::msg)?;
    assert_eq!(status, 200, "{body}");
    assert!(
        b.sync_peers()
            .iter()
            .any(|peer| peer.id == info_a.id && peer.paired)
    );

    let (status, snapshot_bytes) = request_while_pumping(&jobs_b_snapshot, &mut b, move |reply| {
        let _ = reply.send(transport::http_get(
            "127.0.0.1",
            port_b,
            "/sync/snapshot",
            Duration::from_secs(3),
        ));
    })
    .map_err(anyhow::Error::msg)?;
    assert_eq!(status, 200);
    let remote_b =
        SyncSnapshot::from_json(&String::from_utf8(snapshot_bytes)?).map_err(anyhow::Error::msg)?;
    let mut attachment_bytes = Vec::new();
    for attachment in &remote_b.attachments {
        let attachment_id = attachment.id;
        let attachment_path = format!("/sync/attachment/{attachment_id}");
        let (status, bytes) = request_while_pumping(&jobs_b_snapshot, &mut b, move |reply| {
            let _ = reply.send(transport::http_get(
                "127.0.0.1",
                port_b,
                &attachment_path,
                Duration::from_secs(3),
            ));
        })
        .map_err(anyhow::Error::msg)?;
        assert_eq!(status, 200);
        attachment_bytes.push((attachment_id, bytes));
    }
    let merged_a = a.sync_apply_remote(&remote_b, &attachment_bytes, &peer_b)?;
    assert!(a.pages.iter().any(|page| page.id == page_b));
    assert!(
        a.databases
            .databases
            .iter()
            .any(|database| database.id == database_b)
    );
    assert_eq!(
        a.repo.cell(record_b, property_b)?,
        quire_core::core::database::CellValue::Text("数据库单元同步".into())
    );
    assert_eq!(
        std::fs::read(a.attachment_path(attachment_b).unwrap())?,
        b"loopback attachment bytes"
    );
    assert_eq!(
        a.repo.cell(record_b, files_property_b)?,
        quire_core::core::database::CellValue::Items(vec![database_attachment_id.to_string()])
    );
    assert_eq!(
        std::fs::read(
            a.attachment_path(AttachmentId(database_attachment_id))
                .unwrap()
        )?,
        b"database loopback file\0\xff"
    );
    assert!(
        a.document
            .page_blocks(page_b)
            .iter()
            .any(|block| block.text == "来自节点 B 的正文")
    );

    let body = merged_a.to_json();
    let jobs_b_push = jobs_b_snapshot;
    let (status, response) = request_while_pumping(&jobs_b_push, &mut b, move |reply| {
        let _ = reply.send(transport::http_post(
            "127.0.0.1",
            port_b,
            "/sync/snapshot",
            &body,
            Duration::from_secs(3),
        ));
    })
    .map_err(anyhow::Error::msg)?;
    assert_eq!(status, 200, "{response}");
    let merged_b = SyncSnapshot::from_json(&response).map_err(anyhow::Error::msg)?;
    assert!(
        merged_b
            .notes
            .iter()
            .any(|note| note.title == "来自 A 的笔记")
    );
    assert!(b.organizer.note(note_a).is_some());
    assert!(b.pages.iter().any(|page| page.title == "来自 B 的页面"));
    assert!(
        b.databases
            .databases
            .iter()
            .any(|database| database.id == database_b)
    );
    assert_eq!(
        b.repo.cell(record_b, property_b)?,
        quire_core::core::database::CellValue::Text("数据库单元同步".into())
    );
    assert_eq!(
        b.repo.cell(record_b, files_property_b)?,
        quire_core::core::database::CellValue::Items(vec![database_attachment_id.to_string()])
    );

    // Database file removals travel as normal value tombstones. The attachment
    // bytes stay available for recovery; re-adding the reference after the
    // delete must make it visible in a later snapshot again.
    assert!(b.set_database_cell_many(database_block_b, record_b, files_property_b, &[])?);
    let deleted_snapshot = b.sync_export()?;
    assert!(
        deleted_snapshot
            .attachments
            .iter()
            .any(|attachment| attachment.id == attachment_b.0)
    );
    assert!(
        !deleted_snapshot
            .attachments
            .iter()
            .any(|attachment| attachment.id == database_attachment_id)
    );
    let deleted_on_a = a.sync_apply_remote(&deleted_snapshot, &[], &peer_b)?;
    assert_eq!(
        a.repo.cell(record_b, files_property_b)?,
        quire_core::core::database::CellValue::Empty
    );
    let peer_a = PeerRecord {
        id: info_a.id.clone(),
        name: info_a.name.clone(),
        kind: info_a.kind.clone(),
        ip: "127.0.0.1".into(),
        port: port_a,
        paired: true,
        last_seen: 0,
        last_sync: String::new(),
    };
    b.sync_apply_remote(&deleted_on_a, &[], &peer_a)?;
    assert!(b.set_database_cell_many(
        database_block_b,
        record_b,
        files_property_b,
        &[database_attachment_id.to_string()]
    )?);
    let restored_snapshot = b.sync_export()?;
    assert!(
        restored_snapshot
            .attachments
            .iter()
            .any(|attachment| attachment.id == database_attachment_id)
    );
    a.sync_apply_remote(&restored_snapshot, &[], &peer_b)?;
    assert_eq!(
        a.repo.cell(record_b, files_property_b)?,
        quire_core::core::database::CellValue::Items(vec![database_attachment_id.to_string()])
    );

    // The first server is also a real protocol endpoint; GET its info after
    // the exchange to prove the two nodes kept distinct, stable identities.
    let (status, info_a_bytes) =
        transport::http_get("127.0.0.1", port_a, "/sync/info", Duration::from_secs(3))
            .map_err(anyhow::Error::msg)?;
    assert_eq!(status, 200);
    let info_a =
        DeviceInfo::from_json(&String::from_utf8(info_a_bytes)?).map_err(anyhow::Error::msg)?;
    assert_ne!(info_a.id, info_b.id);
    a.persistence.force_flush()?;
    b.persistence.force_flush()?;
    drop(a);
    drop(b);

    let reopened_a = Session::open(&path_a)?;
    let reopened_b = Session::open(&path_b)?;
    assert!(reopened_a.organizer.note(note_a).is_some());
    assert!(
        reopened_a
            .pages
            .iter()
            .any(|page| page.title == "来自 B 的页面")
    );
    assert!(reopened_b.organizer.note(note_a).is_some());
    assert!(
        reopened_b
            .document
            .page_blocks(page_b)
            .iter()
            .any(|block| block.text == "来自节点 B 的正文")
    );
    assert!(
        reopened_a
            .databases
            .databases
            .iter()
            .any(|database| database.id == database_b)
    );
    assert_eq!(
        reopened_a.repo.cell(record_b, property_b)?,
        quire_core::core::database::CellValue::Text("数据库单元同步".into())
    );
    assert_eq!(
        std::fs::read(reopened_a.attachment_path(attachment_b).unwrap())?,
        b"loopback attachment bytes"
    );
    assert_eq!(
        reopened_a.repo.cell(record_b, files_property_b)?,
        quire_core::core::database::CellValue::Items(vec![database_attachment_id.to_string()])
    );
    assert_eq!(
        std::fs::read(
            reopened_a
                .attachment_path(AttachmentId(database_attachment_id))
                .unwrap()
        )?,
        b"database loopback file\0\xff"
    );
    assert!(
        reopened_b
            .databases
            .databases
            .iter()
            .any(|database| database.id == database_b)
    );
    assert_eq!(
        reopened_b.repo.cell(record_b, property_b)?,
        quire_core::core::database::CellValue::Text("数据库单元同步".into())
    );
    assert_eq!(
        reopened_b.repo.cell(record_b, files_property_b)?,
        quire_core::core::database::CellValue::Items(vec![database_attachment_id.to_string()])
    );
    assert_eq!(
        std::fs::read(
            reopened_b
                .attachment_path(AttachmentId(database_attachment_id))
                .unwrap()
        )?,
        b"database loopback file\0\xff"
    );
    assert_eq!(
        std::fs::read(reopened_b.attachment_path(attachment_b).unwrap())?,
        b"loopback attachment bytes"
    );
    assert!(
        reopened_a
            .setting(&format!("sync.shadow.{}", info_b.id))
            .is_some()
    );
    assert!(
        reopened_b
            .setting(&format!("sync.shadow.{}", info_a.id))
            .is_some()
    );
    Ok(())
}
