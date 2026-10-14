use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

fn read_request(stream: &mut TcpStream) -> String {
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).expect("read bridge request");
    String::from_utf8(bytes).expect("request is UTF-8")
}

#[test]
fn stdio_cli_process_forwards_requests_and_silences_notifications() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind mock MCP server");
    let port = listener.local_addr().unwrap().port();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let scratch =
        std::env::temp_dir().join(format!("quire-gpui-mcp-stdio-{}-{now}", std::process::id()));
    std::fs::create_dir_all(&scratch).expect("create isolated directory");
    let db_path = scratch.join("does-not-exist.db");
    std::fs::write(
        scratch.join("mcp.json"),
        format!(r#"{{"port":{port},"token":"process-test-token"}}"#),
    )
    .expect("write MCP pointer");

    let server = std::thread::spawn(move || {
        let mut requests = Vec::new();
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().expect("accept bridge connection");
            let request = read_request(&mut stream);
            assert!(request.starts_with("POST /mcp HTTP/1.1\r\n"));
            assert!(request.contains("Authorization: Bearer process-test-token\r\n"));
            let body = request.split_once("\r\n\r\n").unwrap().1;
            if body.contains("\"notifications/initialized\"") {
                stream
                    .write_all(
                        b"HTTP/1.1 202 Accepted\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                    )
                    .unwrap();
            } else {
                let response = r#"{"jsonrpc":"2.0","id":7,"result":{"text":"中文 MCP 正常"}}"#;
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    response.len(),
                    response
                )
                .unwrap();
            }
            requests.push(body.to_owned());
        }
        requests
    });

    let mut child = Command::new(env!("CARGO_BIN_EXE_quire-gpui"))
        .args(["--stdio", "--db"])
        .arg(&db_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start actual stdio bridge executable");
    {
        let mut stdin = child.stdin.take().unwrap();
        writeln!(
            stdin,
            r#"{{"jsonrpc":"2.0","id":7,"method":"tools/list","params":{{}}}}"#
        )
        .unwrap();
        writeln!(
            stdin,
            r#"{{"jsonrpc":"2.0","method":"notifications/initialized"}}"#
        )
        .unwrap();
    }
    let output = child.wait_with_output().expect("wait for stdio bridge");
    let requests = server.join().expect("mock MCP server completes");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    let lines = stdout.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 1, "notifications must not reach stdout");
    assert!(lines[0].contains("中文 MCP 正常"));
    assert_eq!(requests.len(), 2);
    assert!(requests[0].contains("tools/list"));
    assert!(requests[1].contains("notifications/initialized"));
    assert!(!db_path.exists(), "the bridge must not open the database");
    std::fs::remove_dir_all(scratch).expect("remove isolated directory");
}
