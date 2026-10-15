// MCP (Model Context Protocol) 接入：让外部 AI Agent 读写这份笔记库。
//
// 三部分，一个文件：
//   1. 协议核心 —— 纯函数，JSON-RPC 2.0 上的 initialize / tools/list /
//      tools/call / ping。`handle_message` 不碰任何 IO，测试直接喂 JSON。
//   2. HTTP 传输 —— `McpServer`，绑 127.0.0.1，Bearer token 鉴权，复用
//      `services::lan_server` 的无依赖手法（std::net，一线程一连接）。
//      tools/call 经由调用方注入的回调出去：应用层把它翻译成一个
//      channel 任务，丢给 UI 线程的泵执行（AppState 是 Rc，出不了线程），
//      再等回复。HTTP 线程自己不认识 AppState。
//   3. stdio 桥 —— `quire.exe mcp` 子命令。MCP 客户端最普遍的接入方式是
//      spawn 一个子进程走 stdin/stdout，而唯一写者是运行中的应用，所以
//      桥只做转发：一行 JSON 进来 → POST 到应用内嵌的 HTTP 端点 → 响应
//      写回 stdout。端口和 token 从库目录旁的 `mcp.json` 读（应用启动
//      MCP 服务时写入），`--port` 可覆盖前者。
//
// 为什么不用 stdio 直接开数据库：id 的 watermark 在应用的内存里
//（core ADR-0067 的caller-allocates 规则），第二个写者会撞号；而且防抖
// 持久化管线意味着 UI 不会看到外部写进来的行。所以应用是唯一写者，
// Agent 的每个调用都走和键盘输入完全相同的路径。

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::{Value, json};

/// 5878 is `services::sync::SYNC_PORT` — every Quire on the LAN listens there,
/// by design and by documentation (ADR-0128 counts a peer by whether 5878
/// answers). Double-booking it worked only because `controller::wire` binds the
/// wildcard before this module asks for the loopback address, and a later swap
/// of that order would fail the *sync* bind while the MCP page still looked
/// healthy. 5880 is next free: 5877 LAN share, 5878 sync, 5879 discovery.
pub const DEFAULT_PORT: u16 = 5880;
const MAX_HTTP_BODY_BYTES: usize = 1024 * 1024;

pub fn now_label() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let z = days + 719_468;
    let era = (if z >= 0 { z } else { z - 146_096 }) / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { y + 1 } else { y };
    format!(
        "{year:04}-{:02}-{:02} {:02}:{:02}",
        month,
        doy - (153 * mp + 2) / 5 + 1,
        rem / 3600,
        (rem % 3600) / 60
    )
}

/// 我们能原样回给客户端的协议版本。协商规则：请求带来的版本在这张表里
/// 就回它，否则回最新的——MCP 规范要求服务端这样降级。
const SUPPORTED_VERSIONS: [&str; 4] = ["2024-11-05", "2025-03-26", "2025-06-18", "2025-11-25"];
const LATEST_VERSION: &str = "2025-11-25";
const MODERN_VERSION: &str = "2026-07-28";

/// 一个工具的定义，原样进 `tools/list` 的响应。
#[derive(Debug, Clone)]
pub struct ToolDef {
    pub name: &'static str,
    pub description: &'static str,
    /// JSON Schema（`{"type":"object","properties":{…}}`）。
    pub input_schema: Value,
}

impl ToolDef {
    fn object(&self) -> Value {
        json!({
            "name": self.name,
            "description": self.description,
            "inputSchema": self.input_schema,
        })
    }
}

/// 应用层交给 `tools/call` 的执行者：`(工具名, 参数) -> 结果或错误`。
/// 应用侧的实现会跨线程到 UI 再回来，所以这里只要求 Send + Sync。
pub type CallFn = Arc<dyn Fn(&str, Value) -> Result<Value, String> + Send + Sync>;

/// 协议核心对一条消息的回答。`None` 是通知（不该有响应）。
pub enum Reply {
    /// JSON-RPC result。
    Result(Value),
    /// JSON-RPC error（code, message）。
    Error(i64, String),
    /// 通知或无需回答。
    Silent,
}

/// 处理一条已解析的 JSON-RPC 消息。`id` 是请求方的 id（通知为 None）。
pub fn handle_message(
    tools: &[ToolDef],
    method: &str,
    params: &Value,
    id: Option<&Value>,
    call: &CallFn,
) -> Reply {
    if id.is_none() {
        return Reply::Silent;
    }
    match method {
        "initialize" => {
            let asked = params
                .get("protocolVersion")
                .and_then(Value::as_str)
                .unwrap_or(LATEST_VERSION);
            let version = if SUPPORTED_VERSIONS.contains(&asked) {
                asked
            } else {
                LATEST_VERSION
            };
            Reply::Result(json!({
                "protocolVersion": version,
                "capabilities": { "tools": {} },
                "serverInfo": {
                    "name": "quire",
                    "version": env!("CARGO_PKG_VERSION"),
                },
            }))
        }
        "ping" => Reply::Result(json!({})),
        "tools/list" => Reply::Result(json!({
            "tools": tools.iter().map(ToolDef::object).collect::<Vec<_>>(),
        })),
        "tools/call" => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            match call(name, args) {
                Ok(v) => Reply::Result(json!({
                    "content": [{ "type": "text", "text": v.to_string() }],
                    "isError": false,
                })),
                Err(e) => Reply::Result(json!({
                    "content": [{ "type": "text", "text": e }],
                    "isError": true,
                })),
            }
        }
        _ => Reply::Error(-32601, format!("method not found: {method}")),
    }
}

/// 处理一个 HTTP body 里的 JSON-RPC 请求（MCP 允许单条；批量已从
/// 2025-06-18 起移除，见到数组按 -32600 拒绝）。返回要回给客户端的
/// JSON 文本；`None` 表示 202（通知，空响应体）。
pub fn handle_request(tools: &[ToolDef], body: &str, call: &CallFn) -> Option<String> {
    let msg: Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(_) => {
            return Some(err_text(None, -32700, "parse error"));
        }
    };
    if msg.is_array() {
        return Some(err_text(None, -32600, "batch requests are not supported"));
    }
    let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
    let id = msg.get("id").filter(|v| !v.is_null());
    match handle_message(
        tools,
        method,
        msg.get("params").unwrap_or(&Value::Null),
        id,
        call,
    ) {
        Reply::Result(v) => Some(
            json!({ "jsonrpc": "2.0", "id": id.cloned().unwrap_or(Value::Null), "result": v })
                .to_string(),
        ),
        Reply::Error(code, message) => Some(err_text(id.cloned(), code, &message)),
        Reply::Silent => None,
    }
}

fn err_text(id: Option<Value>, code: i64, message: &str) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": id.unwrap_or(Value::Null),
        "error": { "code": code, "message": message },
    })
    .to_string()
}

/// 内嵌在运行中的应用里的 HTTP 服务。默认关闭；绑定 127.0.0.1，
/// 每个请求都要带对 token（`Authorization: Bearer …` 或 `X-Quire-Token`）。
pub struct McpServer {
    port: u16,
    token: String,
    tools: Vec<ToolDef>,
    call: CallFn,
}

impl McpServer {
    pub fn new(port: u16, token: String, tools: Vec<ToolDef>, call: CallFn) -> Self {
        McpServer {
            port,
            token,
            tools,
            call,
        }
    }

    pub fn bind(&self) -> Result<TcpListener, String> {
        TcpListener::bind(("127.0.0.1", self.port)).map_err(|e| e.to_string())
    }

    /// Start a stoppable local HTTP server. tools/call is forwarded to the
    /// injected callback, which the desktop routes to its single UI-thread session.
    pub fn spawn(self) -> Result<McpServerHandle, String> {
        let listener = self.bind()?;
        listener
            .set_nonblocking(true)
            .map_err(|error| error.to_string())?;
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let worker_stop = stop.clone();
        let thread = std::thread::Builder::new()
            .name("quire-mcp-http".into())
            .spawn(move || {
                while !worker_stop.load(std::sync::atomic::Ordering::Acquire) {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            let server = McpServer {
                                port: self.port,
                                token: self.token.clone(),
                                tools: self.tools.clone(),
                                call: self.call.clone(),
                            };
                            let _ = std::thread::Builder::new()
                                .name("quire-mcp-request".into())
                                .spawn(move || {
                                    let _ = server.handle_connection(stream);
                                });
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            std::thread::sleep(std::time::Duration::from_millis(25));
                        }
                        Err(error) => {
                            eprintln!("Quire MCP listener stopped: {error}");
                            break;
                        }
                    }
                }
            })
            .map_err(|error| error.to_string())?;
        Ok(McpServerHandle {
            stop,
            thread: Some(thread),
        })
    }

    /// 在已绑定的 listener 上服务到永远（调用方开线程）。
    pub fn serve(&self, listener: TcpListener) {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            let this = McpServer {
                port: self.port,
                token: self.token.clone(),
                tools: self.tools.clone(),
                call: self.call.clone(),
            };
            std::thread::spawn(move || {
                let _ = this.handle_connection(stream);
            });
        }
    }

    fn handle_connection(&self, stream: TcpStream) -> std::io::Result<()> {
        stream.set_read_timeout(Some(std::time::Duration::from_secs(15)))?;
        let mut reader = BufReader::new(stream.try_clone()?);
        let mut request_line = String::new();
        reader.read_line(&mut request_line)?;
        let mut parts = request_line.split_whitespace();
        let method = parts.next().unwrap_or("");
        let path = parts.next().unwrap_or("");
        let mut headers: HashMap<String, String> = HashMap::new();
        let mut content_length = 0usize;
        loop {
            let mut line = String::new();
            reader.read_line(&mut line)?;
            let trimmed = line.trim_end();
            if trimmed.is_empty() {
                break;
            }
            if let Some((name, value)) = trimmed.split_once(':') {
                let name = name.trim().to_ascii_lowercase();
                let value = value.trim().to_string();
                if name == "content-length" {
                    content_length = value.parse().unwrap_or(0);
                }
                headers.insert(name, value);
            }
        }
        let authorized = headers
            .get("authorization")
            .map(|v| v.trim_start_matches("Bearer ").trim() == self.token)
            .unwrap_or(false)
            || headers
                .get("x-quire-token")
                .map(|v| v.as_str() == self.token)
                .unwrap_or(false);

        // Streamable HTTP must validate browser origins even when the service
        // is loopback-only: DNS rebinding can still target a local listener.
        if headers
            .get("origin")
            .is_some_and(|origin| !is_local_origin(origin))
        {
            return respond(
                &stream,
                403,
                "application/json",
                "{\"error\":\"origin not allowed\"}",
            );
        }

        let path = path.split('?').next().unwrap_or(path);
        if path != "/mcp" {
            return respond(
                &stream,
                404,
                "application/json",
                "{\"error\":\"not found\"}",
            );
        }
        if !authorized {
            return respond(
                &stream,
                401,
                "application/json",
                "{\"error\":\"unauthorized\"}",
            );
        }
        if method != "POST" {
            // SSE 长连接（GET）这个实现不支持：客户端按规范的回退
            // （每请求一条 POST）照常工作。
            return respond(
                &stream,
                405,
                "application/json",
                "{\"error\":\"POST only\"}",
            );
        }
        let accept = headers.get("accept").map(String::as_str).unwrap_or("");
        if !accepts_mcp_json(accept) {
            return respond(
                &stream,
                400,
                "application/json",
                "{\"error\":\"Accept must include application/json and text/event-stream\"}",
            );
        }
        let protocol_version = headers
            .get("mcp-protocol-version")
            .map(String::as_str)
            // MCP 2025-11-25 specifies this fallback for stateless servers.
            .unwrap_or("2025-03-26");
        if !SUPPORTED_VERSIONS.contains(&protocol_version) {
            if protocol_version != MODERN_VERSION {
                return respond(
                    &stream,
                    400,
                    "application/json",
                    "{\"error\":\"unsupported MCP protocol version\"}",
                );
            }
        }
        if content_length > MAX_HTTP_BODY_BYTES {
            return respond(
                &stream,
                413,
                "application/json",
                "{\"error\":\"request body too large\"}",
            );
        }
        let mut body = vec![0u8; content_length];
        reader.read_exact(&mut body)?;
        let body = String::from_utf8_lossy(&body).to_string();
        if protocol_version == MODERN_VERSION {
            if let Err(error) = validate_modern_http_headers(&headers, &body) {
                return respond(&stream, 400, "application/json", &error);
            }
            let requested_method = serde_json::from_str::<Value>(&body)
                .ok()
                .and_then(|message| {
                    message
                        .get("method")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                });
            if requested_method.as_deref().is_some_and(|method| {
                !matches!(
                    method,
                    "server/discover" | "ping" | "tools/list" | "tools/call"
                )
            }) {
                let id = serde_json::from_str::<Value>(&body)
                    .ok()
                    .and_then(|message| message.get("id").cloned());
                let error = modern_http_error(
                    id.as_ref(),
                    -32601,
                    &format!("method not found: {}", requested_method.unwrap()),
                );
                return respond(&stream, 404, "application/json", &error);
            }
            return match handle_modern_request(&self.tools, &body, &self.call) {
                Some(text) => respond_with_headers(
                    &stream,
                    200,
                    "application/json",
                    &text,
                    &[("MCP-Protocol-Version", MODERN_VERSION)],
                ),
                None => respond(&stream, 202, "text/plain", ""),
            };
        }
        let response_version = serde_json::from_str::<Value>(&body)
            .ok()
            .filter(|message| message.get("method").and_then(Value::as_str) == Some("initialize"))
            .map(|message| {
                let requested = message
                    .pointer("/params/protocolVersion")
                    .and_then(Value::as_str)
                    .unwrap_or(LATEST_VERSION);
                if SUPPORTED_VERSIONS.contains(&requested) {
                    requested.to_owned()
                } else {
                    LATEST_VERSION.to_owned()
                }
            })
            .unwrap_or_else(|| protocol_version.to_owned());
        match handle_request(&self.tools, &body, &self.call) {
            Some(text) => respond_with_headers(
                &stream,
                200,
                "application/json",
                &text,
                &[("MCP-Protocol-Version", &response_version)],
            ),
            None => respond(&stream, 202, "text/plain", ""),
        }
    }
}

fn validate_modern_http_headers(
    headers: &HashMap<String, String>,
    body: &str,
) -> Result<(), String> {
    let message: Value =
        serde_json::from_str(body).map_err(|_| modern_http_error(None, -32700, "Parse error"))?;
    let method = message.get("method").and_then(Value::as_str).unwrap_or("");
    let id = message.get("id").filter(|value| !value.is_null());
    let params = message.get("params").unwrap_or(&Value::Null);
    let protocol = params
        .pointer("/_meta/io.modelcontextprotocol~1protocolVersion")
        .and_then(Value::as_str);
    let method_header = headers.get("mcp-method").map(String::as_str);
    let expected_name = match method {
        "tools/call" => params.get("name").and_then(Value::as_str),
        "resources/read" => params.get("uri").and_then(Value::as_str),
        "prompts/get" => params.get("name").and_then(Value::as_str),
        _ => None,
    };
    let name_header = headers.get("mcp-name").map(String::as_str);

    if protocol != Some(MODERN_VERSION)
        || method_header != Some(method)
        || expected_name.is_some_and(|name| name_header != Some(name))
        || (expected_name.is_none() && name_header.is_some())
    {
        return Err(modern_http_error(
            id,
            -32020,
            "Header mismatch: modern MCP metadata does not match the request",
        ));
    }
    Ok(())
}

fn modern_http_error(id: Option<&Value>, code: i64, message: &str) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": id.cloned().unwrap_or(Value::Null),
        "error": { "code": code, "message": message },
        "_meta": { "io.modelcontextprotocol/serverInfo": {
            "name": "quire-gpui",
            "version": env!("CARGO_PKG_VERSION"),
        }}
    })
    .to_string()
}

fn handle_modern_request(tools: &[ToolDef], body: &str, call: &CallFn) -> Option<String> {
    let message: Value = match serde_json::from_str(body) {
        Ok(value) => value,
        Err(_) => return Some(modern_http_error(None, -32700, "Parse error")),
    };
    if message.is_array() {
        return Some(modern_http_error(None, -32600, "Invalid Request"));
    }
    let method = message.get("method").and_then(Value::as_str).unwrap_or("");
    let id = message.get("id").filter(|value| !value.is_null());
    if id.is_none() {
        return None;
    }
    let params = message.get("params").unwrap_or(&Value::Null);
    let result = if method == "server/discover" {
        Reply::Result(json!({
            "supportedVersions": [MODERN_VERSION],
            "capabilities": { "tools": {} },
            "instructions": "Quire 笔记与任务工具服务。",
            "ttlMs": 0,
            "cacheScope": "private",
        }))
    } else if method == "initialize" || method == "notifications/initialized" {
        Reply::Error(-32601, format!("method not found: {method}"))
    } else {
        handle_message(tools, method, params, id, call)
    };
    let mut response = match result {
        Reply::Result(mut result) => {
            if let Some(object) = result.as_object_mut() {
                if matches!(method, "tools/list" | "tools/call") {
                    object.insert("resultType".into(), json!("complete"));
                }
                if method == "tools/list" {
                    object.insert("ttlMs".into(), json!(0));
                    object.insert("cacheScope".into(), json!("private"));
                }
            }
            json!({ "jsonrpc": "2.0", "id": id, "result": result })
        }
        Reply::Error(code, text) => {
            json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": text } })
        }
        Reply::Silent => return None,
    };
    response["_meta"] = json!({ "io.modelcontextprotocol/serverInfo": {
        "name": "quire-gpui",
        "version": env!("CARGO_PKG_VERSION"),
    }});
    Some(response.to_string())
}

fn accepts_mcp_json(accept: &str) -> bool {
    let mut json = false;
    let mut event_stream = false;
    for media_type in accept.split(',').map(|value| value.trim()) {
        let media_type = media_type.split(';').next().unwrap_or("").trim();
        json |= media_type.eq_ignore_ascii_case("application/json");
        event_stream |= media_type.eq_ignore_ascii_case("text/event-stream");
    }
    json && event_stream
}

fn is_local_origin(origin: &str) -> bool {
    let Ok(url) = url::Url::parse(origin) else {
        return false;
    };
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return false;
    }
    matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "::1"))
}

pub struct McpServerHandle {
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl McpServerHandle {
    pub fn stop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for McpServerHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

fn respond(stream: &TcpStream, code: u16, ctype: &str, body: &str) -> std::io::Result<()> {
    respond_with_headers(stream, code, ctype, body, &[])
}

fn respond_with_headers(
    mut stream: &TcpStream,
    code: u16,
    ctype: &str,
    body: &str,
    headers: &[(&str, &str)],
) -> std::io::Result<()> {
    let reason = match code {
        200 => "OK",
        202 => "Accepted",
        401 => "Unauthorized",
        403 => "Forbidden",
        400 => "Bad Request",
        413 => "Payload Too Large",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "Internal Server Error",
    };
    let mut head = format!(
        "HTTP/1.1 {code} {reason}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    );
    for (name, value) in headers {
        head.push_str(name);
        head.push_str(": ");
        head.push_str(value);
        head.push_str("\r\n");
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes())?;
    stream.write_all(body.as_bytes())?;
    stream.flush()
}

// ─── mcp.json 指针文件 ───────────────────────────────────────────────────────
//
// 桥进程要知道两件事：端口和 token。都写在库目录旁的 `mcp.json`（应用开
// 服务时写），桥用同一套 data_location 规则解析目录，不碰 SQLite。

/// 指针文件：`{"port": 5880, "token": "…"}`。
pub fn pointer_path(dir: &Path) -> PathBuf {
    dir.join("mcp.json")
}

pub fn write_pointer(dir: &Path, port: u16, token: &str) -> std::io::Result<()> {
    std::fs::write(
        pointer_path(dir),
        json!({ "port": port, "token": token }).to_string(),
    )
}

pub fn read_pointer(dir: &Path) -> Option<(u16, String)> {
    let text = std::fs::read_to_string(pointer_path(dir)).ok()?;
    let v: Value = serde_json::from_str(&text).ok()?;
    let port = u16::try_from(v.get("port")?.as_u64()?).ok()?;
    if port == 0 {
        return None;
    }
    let token = v.get("token")?.as_str()?.to_string();
    if token.is_empty() {
        return None;
    }
    Some((port, token))
}

/// Forward MCP's line-delimited stdio transport to the one live HTTP server.
/// The bridge intentionally never opens the database; `--db` only locates the
/// adjacent pointer written by the desktop process.
pub fn run_stdio_bridge(db_path: &Path) -> Result<(), String> {
    let directory = db_path.parent().unwrap_or_else(|| Path::new("."));
    let (port, token) = read_pointer(directory).ok_or_else(|| {
        format!(
            "MCP pointer not found: {}",
            pointer_path(directory).display()
        )
    })?;
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut output = stdout.lock();
    for line in stdin.lock().lines() {
        let line = line.map_err(|error| format!("read MCP stdin: {error}"))?;
        if line.trim().is_empty() {
            continue;
        }
        if let Some(response) = forward_stdio_message(port, &token, &line)? {
            writeln!(output, "{response}").map_err(|error| format!("write MCP stdout: {error}"))?;
            output
                .flush()
                .map_err(|error| format!("flush MCP stdout: {error}"))?;
        }
    }
    Ok(())
}

fn forward_stdio_message(port: u16, token: &str, body: &str) -> Result<Option<String>, String> {
    let mut stream = TcpStream::connect(("127.0.0.1", port))
        .map_err(|error| format!("connect to Quire MCP at 127.0.0.1:{port}: {error}"))?;
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(30)))
        .map_err(|error| error.to_string())?;
    let metadata_headers = transport_http_metadata_headers(body)?;
    write!(
        stream,
        "POST /mcp HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nAccept: application/json, text/event-stream\r\n{metadata_headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .map_err(|error| format!("send MCP request: {error}"))?;
    stream
        .shutdown(std::net::Shutdown::Write)
        .map_err(|error| format!("finish MCP request: {error}"))?;
    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .map_err(|error| format!("read MCP response: {error}"))?;
    let response =
        String::from_utf8(response).map_err(|error| format!("invalid HTTP response: {error}"))?;
    let (headers, payload) = response
        .split_once("\r\n\r\n")
        .ok_or_else(|| "invalid HTTP response from Quire MCP".to_owned())?;
    let status = headers.lines().next().unwrap_or_default();
    if status.starts_with("HTTP/1.1 202 ") {
        return Ok(None);
    }
    if !status.starts_with("HTTP/1.1 200 ") {
        return Err(format!("Quire MCP returned {status}: {payload}"));
    }
    Ok(Some(payload.to_owned()))
}

pub(crate) fn transport_http_metadata_headers(body: &str) -> Result<String, String> {
    let Ok(message) = serde_json::from_str::<Value>(body) else {
        return Ok(String::new());
    };
    let method = message.get("method").and_then(Value::as_str).unwrap_or("");
    let params = message.get("params").unwrap_or(&Value::Null);
    let modern_version = params
        .pointer("/_meta/io.modelcontextprotocol~1protocolVersion")
        .and_then(Value::as_str);
    let version = if modern_version == Some(MODERN_VERSION) {
        Some(MODERN_VERSION)
    } else {
        message
            .pointer("/params/protocolVersion")
            .and_then(Value::as_str)
    };
    let Some(version) = version else {
        return Ok(String::new());
    };
    let mut headers = format!("MCP-Protocol-Version: {version}\r\n");
    if version == MODERN_VERSION {
        if method.is_empty() || !header_safe(method) {
            return Err("invalid modern MCP method for HTTP metadata".into());
        }
        headers.push_str(&format!("Mcp-Method: {method}\r\n"));
        let name = match method {
            "tools/call" | "prompts/get" => params.get("name").and_then(Value::as_str),
            "resources/read" => params.get("uri").and_then(Value::as_str),
            _ => None,
        };
        if let Some(name) = name {
            if !header_safe(name) {
                return Err("modern MCP name must be visible ASCII in the stdio bridge".into());
            }
            headers.push_str(&format!("Mcp-Name: {name}\r\n"));
        }
    }
    Ok(headers)
}

fn header_safe(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|byte| (0x21..=0x7e).contains(&byte))
}

/// 没有依赖可用的随机源：拿 `RandomState` 的种子哈希两次进程状态。
/// 127.0.0.1 上的本地 token，够用。
pub fn mint_token() -> String {
    use std::hash::{BuildHasher, Hash, Hasher};
    let mut a = std::collections::hash_map::RandomState::new().build_hasher();
    std::time::SystemTime::now().hash(&mut a);
    std::process::id().hash(&mut a);
    let hi = a.finish();
    let mut b = std::collections::hash_map::RandomState::new().build_hasher();
    hi.hash(&mut b);
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0)
        .hash(&mut b);
    let lo = b.finish();
    format!("{hi:016x}{lo:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    #[test]
    fn json_rpc_negotiates_lists_tools_and_returns_tool_errors_as_mcp_results() {
        let tools = vec![ToolDef {
            name: "test_tool",
            description: "test",
            input_schema: json!({"type":"object"}),
        }];
        let call: CallFn = Arc::new(|name, _| {
            if name == "test_tool" {
                Err("rejected".into())
            } else {
                Ok(json!({}))
            }
        });
        let init = handle_request(&tools, r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26"}}"#, &call).unwrap();
        let init: Value = serde_json::from_str(&init).unwrap();
        assert_eq!(init["result"]["protocolVersion"], "2025-03-26");
        assert_eq!(init["result"]["capabilities"]["tools"], json!({}));
        let listed = handle_request(
            &tools,
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
            &call,
        )
        .unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&listed).unwrap()["result"]["tools"][0]["name"],
            "test_tool"
        );
        let called = handle_request(&tools, r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"test_tool","arguments":{}}}"#, &call).unwrap();
        let called: Value = serde_json::from_str(&called).unwrap();
        assert_eq!(called["result"]["isError"], true);
        assert_eq!(called["result"]["content"][0]["text"], "rejected");
        assert!(handle_request(&tools, r#"{"jsonrpc":"2.0","method":"ping"}"#, &call).is_none());
    }

    #[test]
    fn http_pointer_round_trips_port_and_token() {
        let scratch = quire_core::testing::ScratchDir::new("gpui-mcp-pointer");
        write_pointer(scratch.path(), 5880, "secret").unwrap();
        assert_eq!(read_pointer(scratch.path()), Some((5880, "secret".into())));
    }

    #[test]
    fn http_pointer_rejects_invalid_ports_and_empty_tokens() {
        let scratch = quire_core::testing::ScratchDir::new("gpui-mcp-invalid-pointer");
        for pointer in [
            r#"{"port":0,"token":"secret"}"#,
            r#"{"port":70000,"token":"secret"}"#,
            r#"{"port":5880,"token":""}"#,
            "not json",
        ] {
            std::fs::write(pointer_path(scratch.path()), pointer).unwrap();
            assert_eq!(read_pointer(scratch.path()), None, "accepted {pointer}");
        }
    }

    #[test]
    fn loopback_http_requires_token_and_dispatches_tools() {
        fn post_with_headers(port: u16, token: Option<&str>, extra: &str, body: &str) -> String {
            let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
            let auth = token
                .map(|token| format!("Authorization: Bearer {token}\r\n"))
                .unwrap_or_default();
            write!(
                stream,
                "POST /mcp HTTP/1.1\r\nHost: 127.0.0.1\r\n{auth}Content-Type: application/json\r\nAccept: application/json, text/event-stream\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
            let mut response = String::new();
            stream.read_to_string(&mut response).unwrap();
            response
        }
        fn post(port: u16, token: Option<&str>, body: &str) -> String {
            post_with_headers(port, token, "", body)
        }

        let port = TcpListener::bind(("127.0.0.1", 0))
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let tools = vec![ToolDef {
            name: "echo",
            description: "echo input",
            input_schema: json!({"type":"object"}),
        }];
        let call: CallFn = Arc::new(|name, args| {
            assert_eq!(name, "echo");
            Ok(json!({"echo": args["value"]}))
        });
        let mut server = McpServer::new(port, "secret".into(), tools, call)
            .spawn()
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        let unauthorized = loop {
            match TcpStream::connect(("127.0.0.1", port)) {
                Ok(_) => break post(port, None, "{}"),
                Err(_) if std::time::Instant::now() < deadline => {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                Err(error) => panic!("loopback MCP server never bound: {error}"),
            }
        };
        assert!(unauthorized.starts_with("HTTP/1.1 401 Unauthorized"));
        let forbidden_origin = post_with_headers(
            port,
            Some("secret"),
            "Origin: https://attacker.example\r\n",
            r#"{"jsonrpc":"2.0","id":1,"method":"ping","params":{}}"#,
        );
        assert!(forbidden_origin.starts_with("HTTP/1.1 403 Forbidden"));
        let unsupported_version = post_with_headers(
            port,
            Some("secret"),
            "MCP-Protocol-Version: 2026-07-28\r\n",
            r#"{"jsonrpc":"2.0","id":1,"method":"ping","params":{}}"#,
        );
        assert!(unsupported_version.starts_with("HTTP/1.1 400 Bad Request"));
        let initialize = post(
            port,
            Some("secret"),
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"smoke","version":"1"}}}"#,
        );
        assert!(initialize.starts_with("HTTP/1.1 200 OK"));
        assert!(initialize.contains("MCP-Protocol-Version: 2025-11-25\r\n"));
        assert!(initialize.contains("\"protocolVersion\":\"2025-11-25\""));
        let modern_discover = post_with_headers(
            port,
            Some("secret"),
            "MCP-Protocol-Version: 2026-07-28\r\nMcp-Method: server/discover\r\n",
            r#"{"jsonrpc":"2.0","id":10,"method":"server/discover","params":{"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientInfo":{"name":"smoke","version":"1"},"io.modelcontextprotocol/clientCapabilities":{}}}}"#,
        );
        assert!(modern_discover.starts_with("HTTP/1.1 200 OK"));
        let modern_discover: Value =
            serde_json::from_str(modern_discover.split_once("\r\n\r\n").unwrap().1).unwrap();
        assert_eq!(
            modern_discover["result"]["supportedVersions"][0],
            MODERN_VERSION
        );
        assert_eq!(
            modern_discover["_meta"]["io.modelcontextprotocol/serverInfo"]["name"],
            "quire-gpui"
        );
        let modern_tool_call = r#"{"jsonrpc":"2.0","id":11,"method":"tools/call","params":{"name":"echo","arguments":{"value":"modern"},"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientInfo":{"name":"smoke","version":"1"},"io.modelcontextprotocol/clientCapabilities":{}}}}"#;
        let modern_result = post_with_headers(
            port,
            Some("secret"),
            "MCP-Protocol-Version: 2026-07-28\r\nMcp-Method: tools/call\r\nMcp-Name: echo\r\n",
            modern_tool_call,
        );
        assert!(modern_result.starts_with("HTTP/1.1 200 OK"));
        let modern_result: Value =
            serde_json::from_str(modern_result.split_once("\r\n\r\n").unwrap().1).unwrap();
        assert_eq!(modern_result["result"]["resultType"], "complete");
        assert_eq!(
            modern_result["result"]["content"][0]["text"],
            "{\"echo\":\"modern\"}"
        );
        let mismatched_modern_name = post_with_headers(
            port,
            Some("secret"),
            "MCP-Protocol-Version: 2026-07-28\r\nMcp-Method: tools/call\r\nMcp-Name: wrong\r\n",
            modern_tool_call,
        );
        assert!(mismatched_modern_name.starts_with("HTTP/1.1 400 Bad Request"));
        assert!(mismatched_modern_name.contains("-32020"));
        let modern_unknown_method = post_with_headers(
            port,
            Some("secret"),
            "MCP-Protocol-Version: 2026-07-28\r\nMcp-Method: resources/list\r\n",
            r#"{"jsonrpc":"2.0","id":12,"method":"resources/list","params":{"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28"}}}"#,
        );
        assert!(modern_unknown_method.starts_with("HTTP/1.1 404 Not Found"));
        assert!(modern_unknown_method.contains("-32601"));
        assert!(accepts_mcp_json("application/json, text/event-stream"));
        assert!(!accepts_mcp_json("application/json"));
        assert!(is_local_origin("http://localhost:5880"));
        assert!(!is_local_origin("https://localhost.attacker.example"));
        let mut oversized = TcpStream::connect(("127.0.0.1", port)).unwrap();
        write!(
            oversized,
            "POST /mcp HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer secret\r\nAccept: application/json, text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            MAX_HTTP_BODY_BYTES + 1
        )
        .unwrap();
        let mut oversized_response = String::new();
        oversized.read_to_string(&mut oversized_response).unwrap();
        assert!(oversized_response.starts_with("HTTP/1.1 413 Payload Too Large"));
        let response = post(
            port,
            Some("secret"),
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"echo","arguments":{"value":"ok"}}}"#,
        );
        assert!(response.starts_with("HTTP/1.1 200 OK"));
        let payload = response.split_once("\r\n\r\n").unwrap().1;
        let value: Value = serde_json::from_str(payload).unwrap();
        assert_eq!(value["result"]["isError"], false);
        assert_eq!(value["result"]["content"][0]["text"], "{\"echo\":\"ok\"}");
        let bridged = forward_stdio_message(
            port,
            "secret",
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"echo","arguments":{"value":"中文"}}}"#,
        )
        .unwrap()
        .unwrap();
        let value: Value = serde_json::from_str(&bridged).unwrap();
        assert_eq!(value["result"]["content"][0]["text"], "{\"echo\":\"中文\"}");
        let modern_bridged = forward_stdio_message(port, "secret", modern_tool_call)
            .unwrap()
            .unwrap();
        let modern_bridged: Value = serde_json::from_str(&modern_bridged).unwrap();
        assert_eq!(modern_bridged["result"]["resultType"], "complete");
        assert_eq!(
            modern_bridged["result"]["content"][0]["text"],
            "{\"echo\":\"modern\"}"
        );
        let notification = forward_stdio_message(
            port,
            "secret",
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        )
        .unwrap();
        assert!(notification.is_none());
        server.stop();

        // The settings UI's restart path stops the old listener before
        // starting another server on the same configured port. Verify that
        // shutdown releases the socket and that a fresh MCP session answers
        // requests after the restart.
        let rebound = TcpListener::bind(("127.0.0.1", port))
            .expect("stopping MCP must release its loopback port");
        drop(rebound);
        let mut restarted = McpServer::new(
            port,
            "secret-rotated".into(),
            vec![ToolDef {
                name: "echo",
                description: "echo input",
                input_schema: json!({"type":"object"}),
            }],
            Arc::new(|_, _| Ok(json!({"restarted": true}))),
        )
        .spawn()
        .expect("MCP must restart on its released port");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        let restarted_response = loop {
            match TcpStream::connect(("127.0.0.1", port)) {
                Ok(_) => {
                    break post(
                        port,
                        Some("secret-rotated"),
                        r#"{"jsonrpc":"2.0","id":3,"method":"ping","params":{}}"#,
                    );
                }
                Err(_) if std::time::Instant::now() < deadline => {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                Err(error) => panic!("restarted MCP listener never bound: {error}"),
            }
        };
        assert!(restarted_response.starts_with("HTTP/1.1 200 OK"));
        assert!(restarted_response.contains("\"id\":3"));
        restarted.stop();
    }
}
