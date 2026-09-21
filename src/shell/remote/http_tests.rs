//! HTTP compatibility cases carried over from the original project's remote tests.
use super::*;

#[test]
fn idle_browser_preconnection_closes_without_an_unsolicited_error_page() {
    let mut server = RemoteServer::start("127.0.0.1", 0).unwrap();
    let mut stream = TcpStream::connect(("127.0.0.1", server.port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(6)))
        .unwrap();
    // Edge may preconnect while the user is still entering the URL. An error
    // queued before any request can become the subsequent navigation's page.
    let mut response = Vec::new();
    stream.read_to_end(&mut response).unwrap();
    assert!(
        response.is_empty(),
        "idle connection received an HTTP response"
    );
    assert!(get(&server, "/", false).starts_with("HTTP/1.0 200"));
    server.stop();
}

#[test]
fn incomplete_command_disconnect_never_executes_or_returns_a_page() {
    let mut server = RemoteServer::start("127.0.0.1", 0).unwrap();
    let mut stream = TcpStream::connect(("127.0.0.1", server.port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(6)))
        .unwrap();
    write!(stream, "POST /api/command HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nContent-Length: 40\r\n\r\n{{", server.port, server.token).unwrap();
    stream.shutdown(std::net::Shutdown::Write).unwrap();
    let mut response = Vec::new();
    stream.read_to_end(&mut response).unwrap();
    assert!(response.is_empty());
    assert!(server.take_commands().is_empty());
    server.stop();
}

#[test]
fn malformed_request_still_returns_a_readable_bad_request() {
    let mut server = RemoteServer::start("127.0.0.1", 0).unwrap();
    let mut stream = TcpStream::connect(("127.0.0.1", server.port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(6)))
        .unwrap();
    stream.write_all(b"INVALID\r\n\r\n").unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    assert!(response.starts_with("HTTP/1.0 400"));
    assert!(response.contains("请求格式不正确，请刷新页面重试。"));
    server.stop();
}

#[test]
fn browser_request_headers_can_arrive_in_separate_network_packets() {
    let mut server = RemoteServer::start("127.0.0.1", 0).unwrap();
    for path in ["/", "/remote.js"] {
        let mut stream = TcpStream::connect(("127.0.0.1", server.port)).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        // Browsers may open a speculative connection before sending a request.
        thread::sleep(Duration::from_millis(100));
        write!(stream, "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:").unwrap();
        // Mobile/Wi-Fi delivery does not guarantee that an entire HTTP header
        // is already available when the server accepts the connection.
        thread::sleep(Duration::from_millis(150));
        let tail = write!(stream, "{}\r\n\r\n", server.port);
        let mut response = String::new();
        let read = stream.read_to_string(&mut response);
        assert!(
            tail.is_ok() && read.is_ok() && response.starts_with("HTTP/1.0 200"),
            "split {path}: tail={tail:?}, read={read:?}, response={response}"
        );
    }
    server.stop();
}

#[test]
fn delayed_command_body_is_queued_only_after_all_bytes_arrive() {
    let mut server = RemoteServer::start("127.0.0.1", 0).unwrap();
    let mut stream = TcpStream::connect(("127.0.0.1", server.port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let body = br#"{"action":"seek","position":0.5}"#;
    write!(stream, "POST /api/command HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n", server.port, server.token, body.len()).unwrap();
    stream.write_all(&body[..10]).unwrap();
    thread::sleep(Duration::from_millis(150));
    assert!(server.take_commands().is_empty());
    stream.write_all(&body[10..]).unwrap();
    let started = Instant::now();
    let command = loop {
        if let Some(command) = server.take_commands().pop() {
            break command;
        }
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "complete command was not queued"
        );
        thread::sleep(Duration::from_millis(5));
    };
    assert_eq!(command.value, json!({"action":"seek","position":0.5}));
    command.reply.send(json!({"ok":true})).unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    assert!(response.starts_with("HTTP/1.0 200"));
    assert!(response.ends_with(r#"{"ok":true}"#));
    server.stop();
}

#[test]
fn repeated_parallel_refreshes_return_complete_page_script_and_styles() {
    let mut server = RemoteServer::start("127.0.0.1", 0).unwrap();
    let port = server.port;
    let workers: Vec<_> = (0..6)
        .map(|worker| {
            thread::spawn(move || {
                for round in 0..10 {
                    let (path, expected) = match (worker + round) % 3 {
                        0 => ("/", remote_page()),
                        1 => ("/remote.js", include_str!("../../../assets/remote.js")),
                        _ => ("/remote.css", include_str!("../../../assets/remote.css")),
                    };
                    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
                    stream
                        .set_read_timeout(Some(Duration::from_secs(3)))
                        .unwrap();
                    write!(stream, "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n").unwrap();
                    thread::sleep(Duration::from_millis(15));
                    stream.write_all(b"\r\n").unwrap();
                    let mut response = String::new();
                    stream.read_to_string(&mut response).unwrap();
                    assert!(response.starts_with("HTTP/1.0 200"));
                    let (headers, body) = response.split_once("\r\n\r\n").unwrap();
                    assert!(headers.contains(&format!("Content-Length: {}", expected.len())));
                    assert_eq!(body, expected, "truncated resource {path}");
                }
            })
        })
        .collect();
    for worker in workers {
        worker.join().unwrap();
    }
    server.stop();
}

fn get(server: &RemoteServer, target: &str, authenticated: bool) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", server.port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let auth = if authenticated {
        format!("Authorization: Bearer {}\r\n", server.token)
    } else {
        String::new()
    };
    write!(
        stream,
        "GET {target} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\n{auth}\r\n",
        server.port
    )
    .unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    response
}

#[test]
fn asset_query_parameters_do_not_change_routes_or_mime_types() {
    let mut server = RemoteServer::start("127.0.0.1", 0).unwrap();
    for (path, mime) in [
        ("/", "text/html"),
        ("/remote.html", "text/html"),
        ("/remote.css", "text/css"),
        ("/remote.js", "text/javascript"),
    ] {
        let plain = get(&server, path, false);
        let versioned = get(&server, &format!("{path}?v=1&cache=mobile"), false);
        assert!(versioned.starts_with("HTTP/1.0 200"), "{path}: {versioned}");
        assert!(versioned.contains(&format!("Content-Type: {mime}; charset=utf-8")));
        assert_eq!(
            plain.split_once("\r\n\r\n").unwrap().1,
            versioned.split_once("\r\n\r\n").unwrap().1
        );
    }
    server.stop();
}

#[test]
fn api_queries_preserve_authentication_and_score_parameter_rejection() {
    let mut server = RemoteServer::start("127.0.0.1", 0).unwrap();
    let token_in_url = format!("/api/state?token={}", server.token);
    assert!(get(&server, &token_in_url, false).starts_with("HTTP/1.0 401"));
    assert!(get(&server, "/api/state?v=1", true).starts_with("HTTP/1.0 200"));
    assert!(get(&server, "/api/score?path=anything", true).starts_with("HTTP/1.0 400"));
    assert!(get(&server, "/api/score?", true).starts_with("HTTP/1.0 200"));
    assert!(get(&server, "/unknown?v=1", false).starts_with("HTTP/1.0 404"));
    server.stop();
}
