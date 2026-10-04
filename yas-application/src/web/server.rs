use super::{
    job::{self, SharedJob},
    platform, validate_origin, validate_token, Connection, ScanOptions, ADDRESS,
};
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    io::{Read, Write},
    net::TcpStream,
    time::{Duration, Instant},
};
use tiny_http::{Header, Method, Request, Response, Server};
use url::Url;

struct Session {
    origin: String,
    allowed: bool,
    touched: Instant,
}
struct Bridge {
    sessions: HashMap<String, Session>,
    jobs: Vec<SharedJob>,
    prefix: Vec<String>,
}

fn header<'a>(request: &'a Request, name: &'static str) -> Option<&'a str> {
    request
        .headers()
        .iter()
        .find(|header| header.field.equiv(name))
        .map(|header| header.value.as_str())
}

fn body<T: serde::de::DeserializeOwned>(request: &mut Request) -> Result<T> {
    if request.body_length().unwrap_or(0) > 4096 {
        bail!("请求过大");
    }
    let mut bytes = Vec::new();
    request.as_reader().take(4097).read_to_end(&mut bytes)?;
    if bytes.len() > 4096 {
        bail!("请求过大");
    }
    Ok(serde_json::from_slice(&bytes)?)
}

fn reply(request: Request, status: u16, value: Value, origin: Option<&str>) {
    let body = if status == 204 {
        String::new()
    } else {
        value.to_string()
    };
    let mut response = Response::from_string(body).with_status_code(status);
    for (key, value) in [
        ("Content-Type", "application/json; charset=utf-8"),
        ("Cache-Control", "no-store"),
        ("Vary", "Origin"),
        ("X-YAS-Web", "1"),
    ] {
        response.add_header(Header::from_bytes(key, value).unwrap());
    }
    if let Some(origin) = origin {
        response.add_header(Header::from_bytes("Access-Control-Allow-Origin", origin).unwrap());
        response.add_header(
            Header::from_bytes("Access-Control-Allow-Methods", "GET, POST, OPTIONS").unwrap(),
        );
        response.add_header(
            Header::from_bytes(
                "Access-Control-Allow-Headers",
                "Authorization, Content-Type",
            )
            .unwrap(),
        );
        response.add_header(
            Header::from_bytes("Access-Control-Allow-Private-Network", "true").unwrap(),
        );
    }
    let _ = request.respond(response);
}

impl Bridge {
    fn connect(&mut self, connection: Connection) -> Result<bool> {
        validate_origin(&connection.origin)?;
        validate_token(&connection.token)?;
        if let Some(session) = self.sessions.get_mut(&connection.token) {
            if session.origin != connection.origin {
                bail!("连接来源不匹配");
            }
            session.touched = Instant::now();
            return Ok(session.allowed);
        }
        if self.jobs.iter().any(|job| job.lock().unwrap().running()) {
            bail!("扫描正在运行，请完成或取消后再授权新连接");
        }
        self.sessions
            .retain(|_, session| session.touched.elapsed() < Duration::from_secs(8 * 3600));
        if self.sessions.len() >= 16 {
            bail!("连接数量已达上限，请关闭 YAS 网页服务后重试");
        }
        let allowed = platform::authorize(&connection);
        self.sessions.insert(
            connection.token,
            Session {
                origin: connection.origin,
                allowed,
                touched: Instant::now(),
            },
        );
        Ok(allowed)
    }

    fn token(&mut self, request: &Request, origin: &str) -> Result<String> {
        let token = header(request, "Authorization")
            .and_then(|value| value.strip_prefix("Bearer "))
            .context("请先连接并授权 YAS")?;
        let session = self.sessions.get_mut(token).context("请先连接并授权 YAS")?;
        if session.origin != origin
            || !session.allowed
            || session.touched.elapsed() > Duration::from_secs(8 * 3600)
        {
            bail!("YAS 授权已拒绝或过期，请重新连接");
        }
        session.touched = Instant::now();
        Ok(token.into())
    }

    fn job(&self, id: &str, owner: &str) -> Result<SharedJob> {
        self.jobs
            .iter()
            .find(|job| {
                let job = job.lock().unwrap();
                job.id == id && job.owner == owner
            })
            .cloned()
            .context("扫描任务不存在或不属于当前网页")
    }

    fn route(&mut self, request: &mut Request, origin: &str) -> Result<(u16, Value)> {
        let url = Url::parse(&format!("http://{ADDRESS}{}", request.url()))?;
        if url.path() == "/api/connect" && request.method() == &Method::Post {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct ConnectBody {
                token: String,
            }
            let body: ConnectBody = body(request)?;
            let allowed = self.connect(Connection {
                origin: origin.into(),
                token: body.token,
            })?;
            return Ok((
                if allowed { 200 } else { 403 },
                json!({"allowed":allowed,"product":"yas-web",
                    "error": if allowed { None } else { Some("YAS 授权被拒绝，请重新连接并在 Windows 提示中允许当前网站") }}),
            ));
        }
        if url.path() == "/api/session" && request.method() == &Method::Get {
            let pending =
                header(request, "Authorization").and_then(|value| value.strip_prefix("Bearer "));
            if pending.is_some_and(|token| !self.sessions.contains_key(token)) {
                return Ok((404, json!({"error":"正在等待授权"})));
            }
        }
        let owner = match self.token(request, origin) {
            Ok(token) => token,
            Err(error) => return Ok((403, json!({"error":error.to_string()}))),
        };
        match (request.method(), url.path()) {
            (&Method::Get, "/api/session") => {
                Ok((200, json!({"allowed":true,"product":"yas-web"})))
            },
            (&Method::Get, "/api/windows") => Ok((200, json!(platform::windows()))),
            (&Method::Post, "/api/scan") => {
                if self.jobs.iter().any(|job| job.lock().unwrap().running()) {
                    return Ok((409, json!({"error":"已有扫描正在运行"})));
                }
                while self.jobs.len() >= 8 {
                    self.jobs.remove(0);
                }
                let options: ScanOptions = body(request)?;
                let prefix: Vec<&str> = self.prefix.iter().map(String::as_str).collect();
                let job = job::start(options, owner, &prefix)?;
                let id = job.lock().unwrap().id.clone();
                self.jobs.push(job);
                Ok((200, json!({"job":id})))
            },
            (&Method::Get, "/api/status" | "/api/result") => {
                let params: HashMap<_, _> = url.query_pairs().into_owned().collect();
                let job = self.job(params.get("job").context("缺少任务编号")?, &owner)?;
                let mut job = job.lock().unwrap();
                job.touched = Instant::now();
                if url.path() == "/api/result" {
                    return match &job.result {
                        Some(result) if job.state == "completed" => Ok((200, result.clone())),
                        _ => Ok((409, json!({"error":"扫描尚未成功完成"}))),
                    };
                }
                let after = params
                    .get("after")
                    .map(|seq| seq.parse::<u64>())
                    .transpose()?
                    .unwrap_or(0);
                Ok((
                    200,
                    json!({"job":job.id,"state":job.state,"error":job.error,
                    "next":job.sequence,"logs":job.logs.iter().filter(|line| line.seq > after).collect::<Vec<_>>()}),
                ))
            },
            (&Method::Post, "/api/cancel") => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Cancel {
                    job: String,
                }
                let body: Cancel = body(request)?;
                self.job(&body.job, &owner)?.lock().unwrap().cancel()?;
                Ok((200, json!({"ok":true})))
            },
            _ => Ok((404, json!({"error":"接口不存在"}))),
        }
    }

    fn request(&mut self, mut request: Request) {
        // Reject alternate Host values to prevent DNS rebinding.
        if !matches!(
            header(&request, "Host"),
            Some("127.0.0.1:32334" | "localhost:32334")
        ) {
            reply(request, 403, json!({"error":"仅允许本机连接"}), None);
            return;
        }
        let origin = header(&request, "Origin").map(str::to_owned);
        let valid_origin = origin
            .as_deref()
            .filter(|origin| validate_origin(origin).is_ok());
        if request.method() == &Method::Get && request.url() == "/api/info" {
            reply(
                request,
                200,
                json!({"product":"yas-web","protocolVersion":1,"version":env!("CARGO_PKG_VERSION")}),
                valid_origin,
            );
            return;
        }
        let Some(origin) = valid_origin else {
            reply(request, 403, json!({"error":"缺少有效的网站来源"}), None);
            return;
        };
        if request.method() == &Method::Options {
            reply(request, 204, Value::Null, Some(origin));
            return;
        }
        match self.route(&mut request, origin) {
            Ok((status, data)) => reply(request, status, data, Some(origin)),
            Err(error) => reply(
                request,
                400,
                json!({"error":format!("{error:#}")}),
                Some(origin),
            ),
        }
    }
}

fn local_request(method: &str, path: &str, origin: &str, body: &str) -> Result<String> {
    let mut stream = TcpStream::connect_timeout(&ADDRESS.parse()?, Duration::from_secs(2))?;
    stream.set_read_timeout(Some(Duration::from_secs(120)))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;
    write!(stream, "{method} {path} HTTP/1.1\r\nHost: {ADDRESS}\r\nOrigin: {origin}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len())?;
    let mut response = String::new();
    stream.take(32768).read_to_string(&mut response)?;
    Ok(response)
}

pub fn serve(launch: Option<Connection>, prefix: &[&str]) -> Result<()> {
    if !yas::utils::is_admin() {
        bail!("请以管理员身份运行 YAS 网页服务");
    }
    let server = match Server::http(ADDRESS) {
        Ok(server) => server,
        Err(error) => {
            let Some(connection) = launch else {
                bail!("无法启动网页服务（可能已经运行）：{error}");
            };
            let info = local_request("GET", "/api/info", &connection.origin, "")?;
            if !info.contains("\"product\":\"yas-web\"") {
                bail!("网页连接端口被其他程序占用");
            }
            let response = local_request(
                "POST",
                "/api/connect",
                &connection.origin,
                &json!({"token":connection.token}).to_string(),
            )?;
            if !response.starts_with("HTTP/1.1 200") {
                bail!("未获准连接，请在网页中重试");
            }
            return Ok(());
        },
    };
    let mut bridge = Bridge {
        sessions: HashMap::new(),
        jobs: Vec::new(),
        prefix: prefix.iter().map(|arg| arg.to_string()).collect(),
    };
    if let Some(connection) = launch {
        bridge.connect(connection)?;
    }
    log::info!("YAS 网页服务已启动：{ADDRESS}。关闭此窗口即可停止服务。空闲 15 分钟会自动退出。");
    let mut activity = Instant::now();
    loop {
        if let Some(request) = server.recv_timeout(Duration::from_millis(500))? {
            activity = Instant::now();
            bridge.request(request);
        }
        if activity.elapsed() > Duration::from_secs(900)
            && !bridge.jobs.iter().any(|job| job.lock().unwrap().running())
        {
            break;
        }
    }
    Ok(())
}
