//! Bounded authenticated LAN transport. HTTP threads never access the controller.
use crate::{controller::AppController, library::song_id};
use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Read, Write},
    net::{TcpListener, TcpStream, ToSocketAddrs},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
pub struct RemoteCommand {
    pub value: Value,
    pub expired: Arc<AtomicBool>,
    pub reply: mpsc::SyncSender<Value>,
}
struct Shared {
    state: Vec<u8>,
    score: Vec<u8>,
    score_id: String,
    tokens: f64,
    refilled: Instant,
    last_client_at: Option<Instant>,
}
pub struct RemoteServer {
    stop: Arc<AtomicBool>,
    shared: Arc<Mutex<Shared>>,
    receiver: mpsc::Receiver<RemoteCommand>,
    thread: Option<JoinHandle<()>>,
    pub port: u16,
    pub token: String,
    pub addresses: Vec<String>,
    interface_addresses: Vec<(String, String)>,
}
impl RemoteServer {
    pub fn start(host: &str, port: u16) -> Result<Self> {
        let listener = TcpListener::bind((host, port)).or_else(|e| {
            if port != 0 {
                TcpListener::bind((host, 0))
            } else {
                Err(e)
            }
        })?;
        let port = listener.local_addr()?.port();
        listener.set_nonblocking(true)?;
        let token = secure_token()?;
        let interface_addresses = lan_addresses();
        let mut addresses = local_addresses();
        addresses.extend(
            interface_addresses
                .iter()
                .map(|(_, address)| address.clone()),
        );
        addresses.sort();
        addresses.dedup();
        let mut hosts: BTreeSet<String> = addresses.iter().cloned().collect();
        hosts.insert("localhost".into());
        if host != "0.0.0.0" && host != "::" {
            hosts.insert(host.to_lowercase());
        }
        let shared = Arc::new(Mutex::new(Shared {
            state: b"{}".to_vec(),
            score: b"{}".to_vec(),
            score_id: String::new(),
            tokens: 20.,
            refilled: Instant::now(),
            last_client_at: None,
        }));
        let stop = Arc::new(AtomicBool::new(false));
        let (tx, receiver) = mpsc::sync_channel(16);
        let active = Arc::new(AtomicUsize::new(0));
        let run = stop.clone();
        let data = shared.clone();
        let secret = token.clone();
        let handle = thread::Builder::new()
            .name("harmonica-remote".into())
            .spawn(move || {
                while !run.load(Ordering::Relaxed) {
                    match listener.accept() {
                        Ok((mut stream, _)) => {
                            if active.fetch_add(1, Ordering::Relaxed) >= 12 {
                                active.fetch_sub(1, Ordering::Relaxed);
                                let _ = respond(&mut stream, 503, b"{}", "application/json", false);
                                continue;
                            }
                            let active = active.clone();
                            let tx = tx.clone();
                            let data = data.clone();
                            let hosts = hosts.clone();
                            let secret = secret.clone();
                            let stopped = run.clone();
                            let guard = ClientGuard(active);
                            let _ = thread::Builder::new().name("harmonica-http".into()).spawn(
                                move || {
                                    let _guard = guard;
                                    let _ = stream.set_read_timeout(Some(Duration::from_secs(3)));
                                    let _ = stream.set_write_timeout(Some(Duration::from_secs(3)));
                                    if handle_request(
                                        &mut stream,
                                        &hosts,
                                        port,
                                        &secret,
                                        &data,
                                        &tx,
                                        &stopped,
                                    )
                                    .is_err()
                                    {
                                        let _ = respond(
                                            &mut stream,
                                            400,
                                            b"{\"ok\":false}",
                                            "application/json",
                                            false,
                                        );
                                    }
                                },
                            );
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(25))
                        }
                        Err(_) => break,
                    }
                }
            })?;
        Ok(Self {
            stop,
            shared,
            receiver,
            thread: Some(handle),
            port,
            token,
            addresses,
            interface_addresses,
        })
    }
    pub fn url(&self) -> String {
        let host = self
            .interface_addresses
            .first()
            .map(|(_, address)| address.as_str())
            .or_else(|| {
                self.addresses
                    .iter()
                    .find(|s| s.contains('.') && !s.starts_with("127."))
                    .map(String::as_str)
            })
            .unwrap_or("127.0.0.1");
        self.url_for(host)
            .expect("server URLs use an enumerated local address")
    }
    /// Original dialog contract: (adapter display name, local IP address).
    pub fn address_options(&self) -> Vec<(String, String)> {
        if !self.interface_addresses.is_empty() {
            return self.interface_addresses.clone();
        }
        let options: Vec<_> = self
            .addresses
            .iter()
            .filter(|address| {
                address
                    .parse::<std::net::Ipv4Addr>()
                    .is_ok_and(|ip| !ip.is_loopback())
            })
            .map(|address| ("局域网".to_owned(), address.clone()))
            .collect();
        if options.is_empty() {
            vec![("本机（未发现局域网地址）。".into(), "127.0.0.1".into())]
        } else {
            options
        }
    }
    pub fn url_for(&self, address: &str) -> Option<String> {
        let address = address.parse::<std::net::IpAddr>().ok()?.to_string();
        if !self.addresses.contains(&address) {
            return None;
        }
        let host = if address.contains(':') {
            format!("[{address}]")
        } else {
            address
        };
        Some(format!("http://{host}:{}/#token={}", self.port, self.token))
    }
    pub fn client_connected(&self) -> bool {
        if self.stop.load(Ordering::Relaxed) {
            return false;
        }
        self.shared
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .last_client_at
            .is_some_and(|last| last.elapsed() < Duration::from_secs(5))
    }
    pub fn publish(&self, state: &Value, score: &Value) -> Result<()> {
        let mut s = self.shared.lock().unwrap_or_else(|e| e.into_inner());
        s.state = serde_json::to_vec(state)?;
        let id = score["id"].as_str().unwrap_or("");
        if id != s.score_id {
            s.score = serde_json::to_vec(score)?;
            s.score_id = id.into();
        }
        Ok(())
    }
    pub fn take_commands(&self) -> Vec<RemoteCommand> {
        (0..4)
            .filter_map(|_| self.receiver.try_recv().ok())
            .filter(|r| !r.expired.load(Ordering::Relaxed))
            .collect()
    }
    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        while let Ok(c) = self.receiver.try_recv() {
            let _ = c
                .reply
                .try_send(json!({"ok":false,"message":"电脑已关闭遥控，请重新连接。"}));
        }
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}
impl Drop for RemoteServer {
    fn drop(&mut self) {
        self.stop();
    }
}
struct ClientGuard(Arc<AtomicUsize>);
impl Drop for ClientGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
}
pub fn local_addresses() -> Vec<String> {
    let mut set = BTreeSet::from(["127.0.0.1".to_string(), "::1".to_string()]);
    if let Ok(name) = std::env::var("COMPUTERNAME").or_else(|_| std::env::var("HOSTNAME")) {
        if let Ok(addrs) = (name.as_str(), 0).to_socket_addrs() {
            for a in addrs {
                set.insert(a.ip().to_string());
            }
        }
    }
    set.into_iter().collect()
}
/// Discover local adapter labels without contacting a public network service.
pub fn lan_addresses() -> Vec<(String, String)> {
    #[cfg(windows)]
    let mut found = windows_lan_addresses();
    #[cfg(not(windows))]
    let mut found = Vec::new();
    if found.is_empty() {
        found = local_addresses()
            .into_iter()
            .filter(|address| {
                address
                    .parse::<std::net::Ipv4Addr>()
                    .is_ok_and(|ip| ip.is_private() && !ip.is_loopback() && !ip.is_link_local())
            })
            .map(|address| ("局域网".into(), address))
            .collect();
    }
    let virtual_adapter = |name: &str| {
        let name = name.to_lowercase();
        ["virtual", "vmware", "vethernet", "vpn", "tun", "tailscale"]
            .iter()
            .any(|word| name.contains(word))
    };
    found.sort_by(|a, b| {
        virtual_adapter(&a.0)
            .cmp(&virtual_adapter(&b.0))
            .then(a.cmp(b))
    });
    found.dedup();
    found
}
#[cfg(windows)]
fn windows_lan_addresses() -> Vec<(String, String)> {
    use std::{ffi::c_void, mem::size_of, ptr};
    // ABI prefixes from the installed Windows SDK iptypes.h. The Windows API
    // owns each linked node inside the aligned buffer for this call's lifetime.
    #[repr(C)]
    struct SocketAddress {
        address: *const u8,
        length: i32,
    }
    #[repr(C)]
    struct Unicast {
        alignment: u64,
        next: *const Unicast,
        address: SocketAddress,
    }
    #[repr(C)]
    struct Adapter {
        alignment: u64,
        next: *const Adapter,
        adapter_name: *const i8,
        first_unicast: *const Unicast,
        first_anycast: *const c_void,
        first_multicast: *const c_void,
        first_dns: *const c_void,
        dns_suffix: *const u16,
        description: *const u16,
        friendly_name: *const u16,
        physical_address: [u8; 8],
        physical_length: u32,
        flags: u32,
        mtu: u32,
        if_type: u32,
        oper_status: i32,
    }
    #[link(name = "iphlpapi")]
    unsafe extern "system" {
        fn GetAdaptersAddresses(
            family: u32,
            flags: u32,
            reserved: *const c_void,
            adapters: *mut Adapter,
            size: *mut u32,
        ) -> u32;
    }
    let mut size = 15_000u32;
    for _ in 0..3 {
        if size as usize > 1024 * 1024 {
            return Vec::new();
        }
        let mut storage = vec![0u64; (size as usize).div_ceil(8)];
        let first = storage.as_mut_ptr().cast::<Adapter>();
        // SAFETY: storage is 8-byte aligned and contains at least size bytes;
        // the API writes only that buffer and returns its required size on 111.
        let result = unsafe { GetAdaptersAddresses(2, 2 | 4 | 8, ptr::null(), first, &mut size) };
        if result == 111 {
            continue;
        }
        if result != 0 {
            return Vec::new();
        }
        let mut found = Vec::new();
        let mut adapter = first.cast_const();
        for _ in 0..1024 {
            if adapter.is_null() {
                break;
            }
            // SAFETY: successful GetAdaptersAddresses returns a linked list of
            // IP_ADAPTER_ADDRESSES nodes in storage. Only the verified prefix is read.
            let current = unsafe { &*adapter };
            if current.alignment as u32 as usize >= size_of::<Adapter>()
                && current.oper_status == 1
                && current.if_type != 24
            {
                let mut name = String::new();
                if !current.friendly_name.is_null() {
                    let mut length = 0;
                    while length < 512 && unsafe { *current.friendly_name.add(length) } != 0 {
                        length += 1;
                    }
                    name = String::from_utf16_lossy(unsafe {
                        std::slice::from_raw_parts(current.friendly_name, length)
                    });
                }
                if name.is_empty() {
                    name = "局域网".into();
                }
                let mut unicast = current.first_unicast;
                for _ in 0..1024 {
                    if unicast.is_null() {
                        break;
                    }
                    let address = unsafe { &*unicast };
                    if !address.address.address.is_null() && address.address.length >= 16 {
                        let bytes =
                            unsafe { std::slice::from_raw_parts(address.address.address, 16) };
                        if u16::from_ne_bytes([bytes[0], bytes[1]]) == 2 {
                            let ip =
                                std::net::Ipv4Addr::new(bytes[4], bytes[5], bytes[6], bytes[7]);
                            if ip.is_private() && !ip.is_loopback() && !ip.is_link_local() {
                                found.push((name.clone(), ip.to_string()));
                            }
                        }
                    }
                    unicast = address.next;
                }
            }
            adapter = current.next;
        }
        return found;
    }
    Vec::new()
}
fn secure_token() -> Result<String> {
    let mut bytes = [0u8; 32];
    #[cfg(windows)]
    {
        #[link(name = "bcrypt")]
        unsafe extern "system" {
            fn BCryptGenRandom(algorithm: isize, buffer: *mut u8, count: u32, flags: u32) -> i32;
        }
        ensure!(
            unsafe { BCryptGenRandom(0, bytes.as_mut_ptr(), 32, 2) } >= 0,
            "无法产生安全的连接凭证。"
        );
    }
    #[cfg(not(windows))]
    {
        std::fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    }
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}
fn constant_equal(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |v, (a, b)| v | (a ^ b)) == 0
}
fn authority(value: &str) -> Option<(String, u16)> {
    if value
        .chars()
        .any(|c| c.is_whitespace() || ['/', '\\', '?', '#', '@'].contains(&c))
    {
        return None;
    }
    if value.starts_with('[') {
        let end = value.find(']')?;
        let host = value[1..end]
            .parse::<std::net::Ipv6Addr>()
            .ok()?
            .to_string();
        let port = if value.len() == end + 1 {
            80
        } else {
            value[end + 1..].strip_prefix(':')?.parse().ok()?
        };
        Some((host, port))
    } else {
        let mut parts = value.split(':');
        let host = parts.next()?.trim_end_matches('.').to_lowercase();
        if host.is_empty() {
            return None;
        }
        let port = parts
            .next()
            .map(|p| p.parse::<u16>())
            .transpose()
            .ok()?
            .unwrap_or(80);
        if parts.next().is_some() {
            return None;
        }
        Some((host, port))
    }
}
fn one<'a>(headers: &'a BTreeMap<String, Vec<String>>, name: &str) -> Option<&'a str> {
    let values = headers.get(name)?;
    if values.len() == 1 {
        Some(values[0].as_str())
    } else {
        None
    }
}
fn respond(
    stream: &mut TcpStream,
    status: u16,
    body: &[u8],
    kind: &str,
    head: bool,
) -> std::io::Result<()> {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        413 => "Content Too Large",
        415 => "Unsupported Media Type",
        429 => "Too Many Requests",
        _ => "Service Unavailable",
    };
    write!(
        stream,
        "HTTP/1.0 {status} {reason}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nReferrer-Policy: no-referrer\r\nX-Frame-Options: DENY\r\nContent-Security-Policy: default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'\r\nConnection: close\r\n\r\n",
        body.len()
    )?;
    if !head {
        stream.write_all(body)?;
    }
    Ok(())
}
fn error(stream: &mut TcpStream, status: u16, message: &str) -> Result<()> {
    respond(
        stream,
        status,
        &serde_json::to_vec(&json!({"ok":false,"message":message}))?,
        "application/json; charset=utf-8",
        false,
    )?;
    Ok(())
}
fn handle_request(
    stream: &mut TcpStream,
    hosts: &BTreeSet<String>,
    port: u16,
    token: &str,
    shared: &Arc<Mutex<Shared>>,
    queue: &mpsc::SyncSender<RemoteCommand>,
    stopped: &AtomicBool,
) -> Result<()> {
    let mut raw = Vec::new();
    let mut byte = [0u8; 1];
    while !raw.ends_with(b"\r\n\r\n") {
        ensure!(raw.len() < 16384, "请求头过大。");
        stream.read_exact(&mut byte)?;
        raw.push(byte[0]);
    }
    let text = std::str::from_utf8(&raw)?;
    let mut lines = text.split("\r\n");
    let request: Vec<_> = lines.next().context("请求缺少方法")?.split(' ').collect();
    ensure!(
        request.len() == 3 && request[2].starts_with("HTTP/1."),
        "请求格式错误"
    );
    let (method, path) = (request[0], request[1]);
    let mut headers: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for line in lines.filter(|s| !s.is_empty()) {
        let (k, v) = line.split_once(':').context("请求头格式错误。")?;
        headers
            .entry(k.to_ascii_lowercase())
            .or_default()
            .push(v.trim().into());
    }
    if stopped.load(Ordering::Relaxed) {
        return error(stream, 503, "手机遥控已关闭。");
    }
    let Some(auth) = one(&headers, "host").and_then(authority) else {
        return error(stream, 403, "访问地址不正确。");
    };
    if !hosts.contains(&auth.0) || auth.1 != port {
        return error(stream, 403, "访问地址不正确，请重新连接。");
    }
    if headers.contains_key("origin") {
        if one(&headers, "origin")
            .and_then(|s| s.strip_prefix("http://"))
            .and_then(authority)
            != Some(auth)
        {
            return error(stream, 403, "只允许当前遥控页面发送操作。");
        }
    }
    if path.starts_with("/api/") {
        if !constant_equal(
            one(&headers, "authorization").unwrap_or("").as_bytes(),
            format!("Bearer {token}").as_bytes(),
        ) {
            return error(stream, 401, "连接凭证已失效，请重新连接。");
        }
        shared
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .last_client_at = Some(Instant::now());
    }
    if method == "GET" || method == "HEAD" {
        let body = match path {
            "/api/state" => Some((
                shared
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .state
                    .clone(),
                "application/json; charset=utf-8",
            )),
            "/api/score" => Some((
                shared
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .score
                    .clone(),
                "application/json; charset=utf-8",
            )),
            "/" | "/remote.html" => Some((
                include_bytes!("../../assets/remote.html").to_vec(),
                "text/html; charset=utf-8",
            )),
            "/remote.css" => Some((
                include_bytes!("../../assets/remote.css").to_vec(),
                "text/css; charset=utf-8",
            )),
            "/remote.js" => Some((
                include_bytes!("../../assets/remote.js").to_vec(),
                "text/javascript; charset=utf-8",
            )),
            _ => None,
        };
        return if let Some((body, kind)) = body {
            respond(stream, 200, &body, kind, method == "HEAD")?;
            Ok(())
        } else {
            error(stream, 404, "没有这个页面。")
        };
    }
    if method != "POST" || path != "/api/command" {
        return error(stream, 404, "没有这个操作地址。");
    }
    if headers.contains_key("transfer-encoding") {
        return error(stream, 400, "不支持分块操作请求。");
    }
    let Some(length) = one(&headers, "content-length")
        .filter(|s| !s.is_empty() && s.bytes().all(|c| c.is_ascii_digit()))
        .and_then(|s| s.parse::<usize>().ok())
    else {
        return error(stream, 400, "缺少有效的长度。");
    };
    if length > 4096 {
        return error(stream, 413, "操作内容太大。");
    }
    if one(&headers, "content-type").map(|s| s.split(';').next().unwrap_or("").trim())
        != Some("application/json")
    {
        return error(stream, 415, "操作需要 JSON 格式。");
    }
    {
        let mut data = shared.lock().unwrap_or_else(|e| e.into_inner());
        data.tokens = (data.tokens + data.refilled.elapsed().as_secs_f64() * 8.).min(20.);
        data.refilled = Instant::now();
        if data.tokens < 1. {
            return error(stream, 429, "指令较多，请稍后再试。");
        }
        data.tokens -= 1.;
    }
    let mut body = vec![0u8; length];
    stream.read_exact(&mut body)?;
    let value = match serde_json::from_slice(&body)
        .map_err(anyhow::Error::from)
        .and_then(validate_command)
    {
        Ok(v) => v,
        Err(_) => return error(stream, 400, "操作格式不正确。"),
    };
    let (tx, rx) = mpsc::sync_channel(1);
    let expired = Arc::new(AtomicBool::new(false));
    if queue
        .try_send(RemoteCommand {
            value,
            expired: expired.clone(),
            reply: tx,
        })
        .is_err()
    {
        return error(stream, 503, "指令较多，请稍后再试。");
    }
    let reply = match rx.recv_timeout(Duration::from_secs(3)) {
        Ok(v) => v,
        Err(_) => {
            expired.store(true, Ordering::Relaxed);
            json!({"ok":false,"message":"电脑暂时没有响应，请稍后再试。"})
        }
    };
    respond(
        stream,
        200,
        &serde_json::to_vec(&reply)?,
        "application/json; charset=utf-8",
        false,
    )?;
    Ok(())
}
pub fn validate_command(value: Value) -> Result<Value> {
    let obj = value.as_object().context("操作内容必须是对象。")?;
    let action = obj
        .get("action")
        .and_then(Value::as_str)
        .context("操作缺少名称")?;
    ensure!(
        [
            "select",
            "play",
            "pause",
            "stop",
            "seek",
            "refresh",
            "game_play",
            "game_stop"
        ]
        .contains(&action),
        "不支持这个操作。"
    );
    let allowed = match action {
        "select" => vec!["action", "song_id", "autoplay"],
        "seek" => vec!["action", "position"],
        _ => vec!["action"],
    };
    ensure!(
        obj.keys().all(|k| allowed.contains(&k.as_str())),
        "操作包含不支持的参数"
    );
    match action {
        "select" => {
            ensure!(
                obj.get("song_id")
                    .and_then(Value::as_str)
                    .is_some_and(|s| !s.is_empty() && s.len() <= 512),
                "请选择有效歌曲"
            );
            ensure!(
                obj.get("autoplay").is_none_or(Value::is_boolean),
                "自动播放选项必须是开或关"
            );
        }
        "seek" => ensure!(
            obj.get("position")
                .and_then(Value::as_f64)
                .is_some_and(|n| n.is_finite() && n >= 0.),
            "位置必须是非负有限秒数。"
        ),
        _ => {}
    }
    Ok(value)
}
pub fn handle_command(c: &mut AppController, command: Value) -> Value {
    let result = (|| -> Result<()> {
        let command = validate_command(command)?;
        let action = command["action"].as_str().unwrap();
        ensure!(!c.state.closed, "电脑已关闭。");
        if !["stop", "game_stop", "refresh"].contains(&action) {
            ensure!(
                !c.busy() && c.state.transition.is_none(),
                "曲谱正在准备或保存，请稍候。"
            );
        }
        match action {
            "select" => {
                let id = command["song_id"].as_str().unwrap();
                let Some(entry) = c.library.iter().find(|e| song_id(&e.path) == id).cloned() else {
                    c.refresh_library()?;
                    bail!("这首歌已不在曲库中，请刷新。")
                };
                let options = entry.options.and_then(|v| serde_json::from_value(v).ok());
                c.load_file(
                    &entry.path,
                    options,
                    true,
                    command["autoplay"].as_bool().unwrap_or(false),
                )?;
            }
            "refresh" => c.refresh_library()?,
            "play" => c.listen()?,
            "pause" => c.pause()?,
            "seek" => c.seek_audio(command["position"].as_f64().unwrap())?,
            "stop" => c.stop()?,
            "game_play" => c.game_play(false)?,
            "game_stop" => c.stop_game_playback()?,
            _ => unreachable!(),
        }
        Ok(())
    })();
    match result {
        Ok(()) => json!({"ok":true,"message":c.state.message}),
        Err(e) => {
            c.report_error(e);
            json!({"ok":false,"message":"操作未完成，请检查电脑端状态。"})
        }
    }
}

#[cfg(test)]
mod pairing_status_tests {
    use super::*;
    fn request(
        server: &RemoteServer,
        host: &str,
        authorization: Option<&str>,
        path: &str,
    ) -> String {
        let mut stream = TcpStream::connect(("127.0.0.1", server.port)).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let auth = authorization
            .map(|value| format!("Authorization: {value}\r\n"))
            .unwrap_or_default();
        write!(
            stream,
            "GET {path} HTTP/1.1\r\nHost: {host}:{}\r\n{auth}\r\n",
            server.port
        )
        .unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        response
    }
    #[test]
    fn only_authenticated_local_api_requests_mark_recent_connection() {
        let mut server = RemoteServer::start("127.0.0.1", 0).unwrap();
        assert!(!server.client_connected());
        assert!(request(&server, "127.0.0.1", None, "/").starts_with("HTTP/1.0 200"));
        assert!(!server.client_connected());
        assert!(request(&server, "127.0.0.1", None, "/api/state").starts_with("HTTP/1.0 401"));
        assert!(!server.client_connected());
        let bearer = format!("Bearer {}", server.token);
        assert!(
            request(&server, "attacker.test", Some(&bearer), "/api/state")
                .starts_with("HTTP/1.0 403")
        );
        assert!(!server.client_connected());
        assert!(
            request(&server, "127.0.0.1", Some(&bearer), "/api/state").starts_with("HTTP/1.0 200")
        );
        assert!(server.client_connected());
        server.shared.lock().unwrap().last_client_at =
            Some(Instant::now() - Duration::from_secs(6));
        assert!(!server.client_connected());
        assert!(
            request(&server, "127.0.0.1", Some(&bearer), "/api/score").starts_with("HTTP/1.0 200")
        );
        assert!(server.client_connected());
        server.stop();
        assert!(!server.client_connected());
    }
    #[test]
    fn pairing_urls_are_limited_to_enumerated_local_addresses() {
        let server = RemoteServer::start("127.0.0.1", 0).unwrap();
        assert!(
            server
                .url_for("127.0.0.1")
                .unwrap()
                .starts_with(&format!("http://127.0.0.1:{}/#token=", server.port))
        );
        for invalid in [
            "attacker.test",
            "127.0.0.1@attacker.test",
            "127.0.0.1/path",
            "127.0.0.1:80",
            "[::1]",
        ] {
            assert!(server.url_for(invalid).is_none());
        }
        let options = server.address_options();
        assert!(!options.is_empty());
        for (name, address) in options {
            assert!(!name.is_empty());
            assert!(address.parse::<std::net::Ipv4Addr>().is_ok());
            assert!(server.url_for(&address).is_some());
        }
    }
    #[test]
    fn adapter_discovery_returns_named_private_ipv4_addresses_without_duplicates() {
        let addresses = lan_addresses();
        let mut unique = BTreeSet::new();
        for (name, address) in addresses {
            assert!(!name.is_empty());
            let ip = address.parse::<std::net::Ipv4Addr>().unwrap();
            assert!(ip.is_private() && !ip.is_loopback() && !ip.is_link_local());
            assert!(unique.insert((name, address)));
        }
    }
    #[test]
    fn command_validation_messages_keep_canonical_wording() {
        // Regression guard: user- and phone-visible wording. Encoding damage
        // anywhere in these literals changes the rendered text.
        let cases = [
            (
                serde_json::json!({"action": "bogus"}),
                "不支持这个操作。",
            ),
            (serde_json::json!({}), "操作缺少名称"),
            (
                serde_json::json!({"action": "select", "song_id": ""}),
                "请选择有效歌曲",
            ),
            (
                serde_json::json!({"action": "seek", "position": -1.0}),
                "位置必须是非负有限秒数。",
            ),
        ];
        for (command, message) in cases {
            assert_eq!(
                validate_command(command).unwrap_err().to_string(),
                message
            );
        }
        assert!(validate_command(serde_json::json!({"action": "stop"})).is_ok());
    }
}
