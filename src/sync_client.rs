//! GPUI's outbound LAN sync worker.
//!
//! The shared core engine owns discovery and the inbound sync server. Its
//! outbound transport currently caps socket I/O at 30 seconds even when a
//! snapshot or attachment call requests 60–120 seconds. Keep the same wire
//! protocol and UI-thread jobs, but apply each operation's timeout to both
//! connection establishment and socket I/O here.

use quire_core::services::sync::{
    engine::{DeviceInfo, Job, PeerRecord},
    model::SyncSnapshot,
};
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::{TcpStream, ToSocketAddrs},
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::Duration,
};

pub enum Cmd {
    SyncWith(PeerRecord),
}

pub fn start(jobs: Sender<Job>) -> Sender<Cmd> {
    let (tx, rx) = mpsc::channel();
    thread::Builder::new()
        .name("gpui-sync-client".into())
        .spawn(move || run(rx, jobs))
        .expect("sync client worker thread should start");
    tx
}

fn run(rx: Receiver<Cmd>, jobs: Sender<Job>) {
    while let Ok(command) = rx.recv() {
        match command {
            Cmd::SyncWith(peer) => sync_with(&peer, &jobs),
        }
    }
}

fn sync_with(peer: &PeerRecord, jobs: &Sender<Job>) {
    let fail = |message: String| {
        let _ = jobs.send(Job::SyncDone {
            peer_id: peer.id.clone(),
            ok: false,
            message,
        });
    };

    let (status, info_body) =
        match http_get(&peer.ip, peer.port, "/sync/info", Duration::from_secs(5)) {
            Ok(response) => response,
            Err(error) => return fail(format!("{}: {error}", peer.name)),
        };
    if status != 200 {
        return fail(format!("{}: info answered {status}", peer.name));
    }
    let info = match DeviceInfo::from_json(&String::from_utf8_lossy(&info_body)) {
        Ok(info) => info,
        Err(error) => return fail(error),
    };
    if info.id != peer.id {
        return fail(format!(
            "{}: answered as {} — the address moved to another device",
            peer.name, info.name
        ));
    }

    let (reply_tx, reply_rx) = mpsc::channel();
    if jobs
        .send(Job::ListLocalAttachments { reply: reply_tx })
        .is_err()
    {
        return fail("the app is shutting down".into());
    }
    let local_ids = match reply_rx.recv() {
        Ok(ids) => ids,
        Err(_) => return fail("the app is shutting down".into()),
    };

    let (status, snapshot_body) = match http_get(
        &peer.ip,
        peer.port,
        "/sync/snapshot",
        Duration::from_secs(60),
    ) {
        Ok(response) => response,
        Err(error) => return fail(format!("{}: {error}", peer.name)),
    };
    if status != 200 {
        return fail(format!(
            "{}: snapshot answered {status}: {}",
            peer.name,
            String::from_utf8_lossy(&snapshot_body)
        ));
    }
    let remote = match SyncSnapshot::from_json(&String::from_utf8_lossy(&snapshot_body)) {
        Ok(snapshot) => snapshot,
        Err(error) => return fail(error),
    };

    let mut attachment_bytes = Vec::new();
    for attachment in &remote.attachments {
        if local_ids.contains(&attachment.id) {
            continue;
        }
        match http_get(
            &peer.ip,
            peer.port,
            &format!("/sync/attachment/{}", attachment.id),
            Duration::from_secs(120),
        ) {
            Ok((200, bytes)) => attachment_bytes.push((attachment.id, bytes)),
            Ok((status, _)) => {
                return fail(format!(
                    "{}: attachment {} answered {status}",
                    peer.name, attachment.id
                ));
            }
            Err(error) => return fail(format!("{}: {error}", peer.name)),
        }
    }

    let (reply_tx, reply_rx) = mpsc::channel();
    if jobs
        .send(Job::ApplyRemote {
            peer: peer.clone(),
            snapshot: remote,
            bytes: attachment_bytes,
            reply: reply_tx,
        })
        .is_err()
    {
        return fail("the app is shutting down".into());
    }
    let merged = match reply_rx.recv() {
        Ok(Ok(snapshot)) => snapshot,
        Ok(Err(error)) => return fail(format!("{}: {error}", peer.name)),
        Err(_) => return fail("the app is shutting down".into()),
    };

    let (status, body) = match http_post(
        &peer.ip,
        peer.port,
        "/sync/snapshot",
        &merged.to_json(),
        Duration::from_secs(120),
    ) {
        Ok(response) => response,
        Err(error) => return fail(format!("{}: {error}", peer.name)),
    };
    if status != 200 {
        return fail(format!("{}: push answered {status}: {body}", peer.name));
    }

    let _ = jobs.send(Job::SyncDone {
        peer_id: peer.id.clone(),
        ok: true,
        message: format!(
            "Synced with {}: {} pages, {} blocks, {} databases, {} notes, {} tasks in the merged workspace",
            peer.name,
            merged.pages.len(),
            merged.blocks.len(),
            merged.databases.len(),
            merged.notes.len(),
            merged.tasks.len()
        ),
    });
}

fn connect(host: &str, port: u16, timeout: Duration) -> Result<TcpStream, String> {
    let address = (host, port)
        .to_socket_addrs()
        .map_err(|error| format!("resolve {host}:{port}: {error}"))?
        .next()
        .ok_or_else(|| format!("{host}:{port} resolved to nothing"))?;
    let stream = TcpStream::connect_timeout(&address, timeout)
        .map_err(|error| format!("connect {host}:{port}: {error}"))?;
    stream
        .set_read_timeout(Some(timeout))
        .map_err(|error| format!("set read timeout: {error}"))?;
    stream
        .set_write_timeout(Some(timeout))
        .map_err(|error| format!("set write timeout: {error}"))?;
    Ok(stream)
}

fn http_get(
    host: &str,
    port: u16,
    path: &str,
    timeout: Duration,
) -> Result<(u16, Vec<u8>), String> {
    let mut stream = connect(host, port, timeout)?;
    write!(
        stream,
        "GET {path} HTTP/1.0\r\nHost: {host}\r\nConnection: close\r\n\r\n"
    )
    .map_err(|error| format!("write request: {error}"))?;
    read_response(stream)
}

fn http_post(
    host: &str,
    port: u16,
    path: &str,
    body: &str,
    timeout: Duration,
) -> Result<(u16, String), String> {
    let mut stream = connect(host, port, timeout)?;
    write!(
        stream,
        "POST {path} HTTP/1.0\r\nHost: {host}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .map_err(|error| format!("write request: {error}"))?;
    let (status, body) = read_response(stream)?;
    Ok((status, String::from_utf8_lossy(&body).into_owned()))
}

fn read_response(stream: TcpStream) -> Result<(u16, Vec<u8>), String> {
    let mut reader = BufReader::new(stream);
    let mut status_line = String::new();
    reader
        .read_line(&mut status_line)
        .map_err(|error| format!("read status: {error}"))?;
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|value| value.parse().ok())
        .ok_or_else(|| format!("bad status line: {status_line:?}"))?;
    let mut content_length = None;
    loop {
        let mut line = String::new();
        reader
            .read_line(&mut line)
            .map_err(|error| format!("read header: {error}"))?;
        if line.trim_end().is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':')
            && name.eq_ignore_ascii_case("content-length")
        {
            content_length = Some(
                value
                    .trim()
                    .parse::<usize>()
                    .map_err(|error| format!("invalid Content-Length: {error}"))?,
            );
        }
    }
    let mut body = Vec::new();
    if let Some(length) = content_length {
        reader
            .take(length as u64)
            .read_to_end(&mut body)
            .map_err(|error| format!("read body: {error}"))?;
        if body.len() != length {
            return Err(format!(
                "short response body: expected {length} bytes, received {}",
                body.len()
            ));
        }
    } else {
        reader
            .read_to_end(&mut body)
            .map_err(|error| format!("read body: {error}"))?;
    }
    Ok((status, body))
}

#[cfg(test)]
mod tests {
    use super::*;
    use quire_core::services::sync::engine::Job;
    use std::{
        io::{BufRead, BufReader, Read, Write},
        net::TcpListener,
        sync::mpsc,
        thread,
    };

    #[test]
    fn connect_applies_the_requested_timeout_to_socket_io() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let accept = thread::spawn(move || listener.accept().unwrap());
        let timeout = Duration::from_secs(120);
        let stream = connect("127.0.0.1", port, timeout).unwrap();
        accept.join().unwrap();
        assert_eq!(stream.read_timeout().unwrap(), Some(timeout));
        assert_eq!(stream.write_timeout().unwrap(), Some(timeout));
    }

    #[test]
    fn client_worker_completes_the_pull_merge_push_wire_cycle() {
        fn response(stream: &mut TcpStream, status: u16, body: &[u8]) {
            write!(
                stream,
                "HTTP/1.0 {status} OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .unwrap();
            stream.write_all(body).unwrap();
        }

        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let peer = PeerRecord {
            id: "remote-test-device".into(),
            name: "isolated peer".into(),
            kind: "windows".into(),
            ip: "127.0.0.1".into(),
            port,
            paired: true,
            last_seen: 1,
            last_sync: String::new(),
        };
        let mut remote = SyncSnapshot::default();
        remote.device_id = peer.id.clone();
        remote.device = peer.name.clone();
        remote
            .attachments
            .push(quire_core::services::sync::model::SAttachment {
                id: 9,
                name: "attachment.bin".into(),
                file: "attachment.bin".into(),
                thumb: String::new(),
                mime: "application/octet-stream".into(),
                bytes: 4,
                width: 0,
                height: 0,
            });
        let remote_json = remote.to_json();
        let server = thread::spawn(move || {
            for index in 0..4 {
                let (mut stream, _) = listener.accept().unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut request_line = String::new();
                reader.read_line(&mut request_line).unwrap();
                let mut content_length = 0usize;
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line.trim_end().is_empty() {
                        break;
                    }
                    if let Some((name, value)) = line.split_once(':')
                        && name.eq_ignore_ascii_case("content-length")
                    {
                        content_length = value.trim().parse().unwrap();
                    }
                }
                match (index, request_line.split_whitespace().nth(1).unwrap()) {
                    (0, "/sync/info") => response(
                        &mut stream,
                        200,
                        DeviceInfo {
                            id: "remote-test-device".into(),
                            name: "isolated peer".into(),
                            kind: "windows".into(),
                            port,
                        }
                        .to_json()
                        .as_bytes(),
                    ),
                    (1, "/sync/snapshot") => response(&mut stream, 200, remote_json.as_bytes()),
                    (2, "/sync/attachment/9") => response(&mut stream, 200, b"data"),
                    (3, "/sync/snapshot") => {
                        let mut body = vec![0; content_length];
                        reader.read_exact(&mut body).unwrap();
                        assert!(String::from_utf8_lossy(&body).contains("remote-test-device"));
                        response(&mut stream, 200, b"{}");
                    }
                    _ => panic!("unexpected sync request: {request_line}"),
                }
            }
        });
        let (jobs_tx, jobs_rx) = mpsc::channel();
        let worker = thread::spawn(move || sync_with(&peer, &jobs_tx));
        let mut finished = false;
        for _ in 0..3 {
            match jobs_rx.recv_timeout(Duration::from_secs(5)).unwrap() {
                Job::ListLocalAttachments { reply } => reply.send(Vec::new()).unwrap(),
                Job::ApplyRemote {
                    snapshot,
                    bytes,
                    reply,
                    ..
                } => {
                    assert_eq!(bytes, [(9, b"data".to_vec())]);
                    reply.send(Ok(snapshot)).unwrap();
                }
                Job::SyncDone { ok, message, .. } => {
                    assert!(ok, "{message}");
                    assert!(message.contains("Synced with isolated peer"));
                    finished = true;
                }
                _ => panic!("unexpected sync job"),
            }
        }
        worker.join().unwrap();
        server.join().unwrap();
        assert!(finished, "the pull-merge-push cycle must finish");
    }
}
