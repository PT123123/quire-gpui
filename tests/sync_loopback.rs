use quire_core::services::sync::{
    engine::{DeviceInfo, Job, PeerRecord},
    model::SyncSnapshot,
    server::SyncServer,
    transport,
};
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
        Job::AttachmentBytes { reply, .. } => {
            reply.send(Vec::new()).unwrap();
        }
        Job::InboundPair { reply, .. } => {
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
    let block_b = b.blocks().first().unwrap().id;
    b.replace_plain_page_block_text(page_b, block_b, "来自节点 B 的正文".into())?;

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
    let merged_a = a.sync_apply_remote(&remote_b, &[], &peer_b)?;
    assert!(a.pages.iter().any(|page| page.id == page_b));
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
