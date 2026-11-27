//! End-to-end tests: a real proxy and REST API on loopback ports, wiremock origins, and
//! reqwest clients. No window is opened.
//! They share the on-disk CA with the app, and each rig leaves its threads running until the process exits.

use crate::domain::config::{Config, Upstream, UpstreamMode};
use crate::proxy::{self, sink::ChannelSink};
use crate::services::{api, flow_store::FlowStore};
use crate::viewmodels::status::NoRepaint;
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex, Once};
use std::time::{Duration, Instant};
use wiremock::matchers::any;
use wiremock::{Mock, MockServer, ResponseTemplate};

const WAIT: Duration = Duration::from_secs(5);

async fn wait_for(what: &str, mut ok: impl FnMut() -> bool) {
    let t = Instant::now();
    while !ok() {
        assert!(t.elapsed() < WAIT, "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

// Below the OS ephemeral range, so mock servers binding port 0 can't take a port a rig has picked but not yet bound.
fn free_port_pair() -> u16 {
    static NEXT: std::sync::atomic::AtomicU16 = std::sync::atomic::AtomicU16::new(0);
    loop {
        let n = NEXT.fetch_add(2, std::sync::atomic::Ordering::Relaxed);
        let port = 20_000 + (std::process::id() as u16 % 2_000) * 4 + n % 4_000;
        if TcpListener::bind(("127.0.0.1", port)).is_ok()
            && TcpListener::bind(("127.0.0.1", port + 1)).is_ok()
        {
            return port;
        }
    }
}

// ---- origins ----

/// An origin answering every request with `status` and `body`.
async fn origin(status: u16, body: &str) -> MockServer {
    let s = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(status).set_body_string(body))
        .mount(&s)
        .await;
    s
}

fn port(s: &MockServer) -> u16 {
    s.address().port()
}

fn url(s: &MockServer, path: &str) -> String {
    format!("http://127.0.0.1:{}{path}", port(s))
}

async fn hits(s: &MockServer) -> usize {
    s.received_requests().await.unwrap().len()
}

async fn last(s: &MockServer) -> wiremock::Request {
    s.received_requests()
        .await
        .unwrap()
        .pop()
        .expect("the origin got no request")
}

// ---- the app under test ----

struct Rig {
    api: u16,
    store: FlowStore,
    /// Sends through the proxy, like a client with `HTTP_PROXY` set.
    client: reqwest::Client,
    /// Talks to the REST API directly.
    direct: reqwest::Client,
}

/// MCP is off by default; scenarios that edit rules or replay through it opt in.
fn mcp_config() -> Config {
    Config {
        mcp_enabled: true,
        ..Config::default()
    }
}

async fn rig(config: Config) -> Rig {
    static CA: Once = Once::new();
    // concurrent first runs would race to write the CA files
    CA.call_once(|| {
        proxy::tls::load_or_create().unwrap();
    });
    let port = free_port_pair();
    let store = FlowStore::default();
    let cfg = Arc::new(Mutex::new(config));
    let (tx, rx) = std::sync::mpsc::channel();
    let sink = Arc::new(ChannelSink::new(tx, Arc::new(NoRepaint)));
    let proxy_cfg = cfg.clone();
    let pump_store = store.clone();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let _ = rt.block_on(proxy::run(port, sink, proxy_cfg));
    });
    std::thread::spawn(move || {
        loop {
            pump_store.pump(&rx, |_| {});
            std::thread::sleep(Duration::from_millis(5));
        }
    });
    api::serve(api::port_for(port), store.clone(), cfg);
    wait_for("the proxy to listen", || {
        TcpStream::connect(("127.0.0.1", port)).is_ok()
    })
    .await;
    let proxy_url = format!("http://127.0.0.1:{port}");
    Rig {
        api: api::port_for(port),
        store,
        client: reqwest::Client::builder()
            .proxy(reqwest::Proxy::http(proxy_url).unwrap())
            .build()
            .unwrap(),
        direct: reqwest::Client::builder().no_proxy().build().unwrap(),
    }
}

impl Rig {
    async fn get(&self, path: &str) -> Value {
        let r = self
            .direct
            .get(format!("http://127.0.0.1:{}{path}", self.api))
            .send()
            .await
            .unwrap();
        serde_json::from_slice(&r.bytes().await.unwrap()).unwrap()
    }

    async fn mcp(&self, tool: &str, arguments: Value) -> Value {
        let body = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": tool, "arguments": arguments}});
        let r = self
            .direct
            .post(format!("http://127.0.0.1:{}/mcp", self.api))
            .header("content-type", "application/json")
            .body(body.to_string())
            .send()
            .await
            .unwrap();
        let v: Value = serde_json::from_slice(&r.bytes().await.unwrap()).unwrap();
        assert!(v["result"]["isError"].is_null(), "{v}");
        v["result"]["structuredContent"].clone()
    }

    async fn add_rule(&self, pattern: &str, action: Value) -> u64 {
        self.mcp(
            "proxyproxy_add_rule",
            json!({"pattern": pattern, "action": action}),
        )
        .await["id"]
            .as_u64()
            .unwrap()
    }

    async fn fetch(&self, url: &str) -> reqwest::Response {
        self.client.get(url).send().await.unwrap()
    }

    /// Flow responses are recorded after the client already has its reply.
    async fn answered(&self, n: usize) {
        wait_for("flows to be recorded", || {
            self.store
                .lock()
                .flows
                .iter()
                .filter(|f| f.status.is_some())
                .count()
                >= n
        })
        .await;
    }
}

// ---- scenarios ----

#[tokio::test(flavor = "multi_thread")]
async fn proxied_request_is_recorded_and_readable_through_the_rest_api() {
    let up = origin(200, "hello").await;
    let rig = rig(Config::default()).await;
    let r = rig.fetch(&url(&up, "/greet?x=1")).await;
    assert_eq!(
        (r.status().as_u16(), r.text().await.unwrap()),
        (200, "hello".into())
    );
    rig.answered(1).await;

    let list = rig.get("/flows").await;
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(list[0]["status"], 200);
    assert!(list[0]["url"].as_str().unwrap().ends_with("/greet?x=1"));
    let detail = rig.get(&format!("/flows/{}", list[0]["id"])).await;
    assert_eq!(detail["response"]["body"], "hello");
    assert_eq!(
        rig.get("/flows?q=status%3A%3E%3D500")
            .await
            .as_array()
            .unwrap()
            .len(),
        0
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn gzip_responses_are_recorded_decoded() {
    use flate2::{Compression, write::GzEncoder};
    let mut e = GzEncoder::new(vec![], Compression::default());
    e.write_all(b"squeezed text").unwrap();
    let up = MockServer::start().await;
    Mock::given(any())
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-encoding", "gzip")
                .set_body_bytes(e.finish().unwrap()),
        )
        .mount(&up)
        .await;
    let rig = rig(Config::default()).await;
    rig.fetch(&url(&up, "/z")).await;
    rig.answered(1).await;
    let id = rig.get("/flows").await[0]["id"].clone();
    assert_eq!(
        rig.get(&format!("/flows/{id}")).await["response"]["body"],
        "squeezed text"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn rule_edits_apply_to_the_next_request() {
    let up = origin(200, "real").await;
    let rig = rig(mcp_config()).await;
    let pattern = format!("127.0.0.1:{}/api*", port(&up));
    assert_eq!(rig.fetch(&url(&up, "/api")).await.status(), 200);

    let id = rig
        .add_rule(&pattern, json!({"Block": {"status": 403}}))
        .await;
    assert_eq!(rig.fetch(&url(&up, "/api")).await.status(), 403);
    assert_eq!(
        hits(&up).await,
        1,
        "a blocked request never reaches the origin"
    );
    assert_eq!(rig.fetch(&url(&up, "/other")).await.status(), 200);

    rig.mcp("proxyproxy_delete_rule", json!({"id": id})).await;
    assert_eq!(rig.fetch(&url(&up, "/api")).await.status(), 200);
    rig.answered(4).await;
    let flows = rig.get("/flows").await;
    let blocked = flows
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["tag"] == "Blocked")
        .count();
    assert_eq!(blocked, 1, "{flows}");
}

#[tokio::test(flavor = "multi_thread")]
async fn mock_and_header_rules_rewrite_traffic() {
    let up = origin(200, "real").await;
    let rig = rig(mcp_config()).await;
    let host = format!("127.0.0.1:{}", port(&up));
    rig.add_rule(
        &format!("{host}/mock"),
        json!({"Mock": {"status": 201, "content_type": "text/plain", "body": "fake"}}),
    )
    .await;
    rig.add_rule(
        &format!("{host}/hdr"),
        json!({"SetHeader": {"response": false, "name": "x-e2e", "value": "1", "remove": false}}),
    )
    .await;
    rig.add_rule(
        &format!("{host}/hdr"),
        json!({"SetHeader": {"response": true, "name": "x-back", "value": "2", "remove": false}}),
    )
    .await;

    let m = rig.fetch(&url(&up, "/mock")).await;
    assert_eq!(m.status(), 201);
    assert_eq!(m.text().await.unwrap(), "fake");
    assert_eq!(hits(&up).await, 0);

    let h = rig.fetch(&url(&up, "/hdr")).await;
    assert_eq!(last(&up).await.headers.get("x-e2e").unwrap(), "1");
    assert_eq!(h.headers().get("x-back").unwrap(), "2");
}

#[tokio::test(flavor = "multi_thread")]
async fn map_remote_sends_the_request_to_another_origin() {
    let (old, new) = (origin(200, "old").await, origin(200, "new").await);
    let rig = rig(mcp_config()).await;
    rig.add_rule(
        &format!("127.0.0.1:{}/*", port(&old)),
        json!({"MapRemote": {"target": format!("http://127.0.0.1:{}", port(&new))}}),
    )
    .await;
    let r = rig.fetch(&url(&old, "/x?y=1")).await;
    assert_eq!(r.text().await.unwrap(), "new");
    assert_eq!((hits(&old).await, hits(&new).await), (0, 1));
    let seen = last(&new).await;
    assert_eq!((seen.url.path(), seen.url.query()), ("/x", Some("y=1")));
}

#[tokio::test(flavor = "multi_thread")]
async fn mute_and_scope_only_change_what_is_recorded() {
    let up = origin(200, "ok").await;
    let overrides = ["muted.test", "shown.test", "other.test"]
        .map(|h| (h.to_string(), "127.0.0.1".to_string()));
    let rig = rig(Config {
        dns_overrides: overrides.to_vec(),
        muted: vec!["muted.test".into()],
        scope_only: true,
        scope: vec!["muted.test".into(), "shown.test".into()],
        ..Config::default()
    })
    .await;
    for host in ["muted.test", "other.test", "shown.test"] {
        let r = rig.fetch(&format!("http://{host}:{}/", port(&up))).await;
        assert_eq!(r.status(), 200, "{host} must still be proxied");
    }
    rig.answered(1).await;
    assert_eq!(hits(&up).await, 3);
    let hosts: Vec<String> = rig
        .store
        .lock()
        .flows
        .iter()
        .map(|f| f.host.clone())
        .collect();
    assert_eq!(hosts.len(), 1, "{hosts:?}");
    assert!(hosts[0].starts_with("shown.test"));
}

#[tokio::test(flavor = "multi_thread")]
async fn manual_upstream_proxy_receives_plain_http_requests() {
    let via = origin(200, "from the upstream proxy").await;
    let rig = rig(Config {
        upstream: Upstream {
            mode: UpstreamMode::Manual,
            url: format!("127.0.0.1:{}", port(&via)),
            ..Default::default()
        },
        ..Config::default()
    })
    .await;
    let r = rig.fetch("http://example.test/x").await;
    assert_eq!(r.text().await.unwrap(), "from the upstream proxy");
    let seen = last(&via).await;
    assert_eq!(
        (seen.url.host_str(), seen.url.path()),
        (Some("example.test"), "/x")
    );
}

// No crate fakes a SOCKS5 proxy, so this one is by hand.
#[tokio::test(flavor = "multi_thread")]
async fn socks5_upstream_gets_the_destination_and_credentials() {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let socks_port = l.local_addr().unwrap().port();
    let server = std::thread::spawn(move || {
        fn rd(s: &mut TcpStream, n: usize) -> Vec<u8> {
            let mut b = vec![0; n];
            s.read_exact(&mut b).unwrap();
            b
        }
        let (mut s, _) = l.accept().unwrap();
        assert_eq!(rd(&mut s, 4), [5, 2, 0, 2]);
        s.write_all(&[5, 2]).unwrap();
        assert_eq!(rd(&mut s, 3), [1, 1, b'u']);
        assert_eq!(rd(&mut s, 2), [1, b'p']);
        s.write_all(&[1, 0]).unwrap();
        assert_eq!(rd(&mut s, 4), [5, 1, 0, 3]);
        let n = rd(&mut s, 1)[0] as usize;
        let host = rd(&mut s, n + 2);
        assert_eq!(&host[..n], b"example.test");
        assert_eq!(u16::from_be_bytes([host[n], host[n + 1]]), 8443);
        s.write_all(&[5, 0, 0, 1, 127, 0, 0, 1, 0, 80]).unwrap();
        let mut head = Vec::new();
        while !head.ends_with(b"\r\n\r\n") {
            head.push(rd(&mut s, 1)[0]);
        }
        s.write_all(
            b"HTTP/1.1 200 OK\r\ncontent-length: 13\r\nconnection: close\r\n\r\nthrough socks",
        )
        .unwrap();
    });
    let rig = rig(Config {
        upstream: Upstream {
            mode: UpstreamMode::Socks,
            url: format!("socks5://u:p@127.0.0.1:{socks_port}"),
            ..Default::default()
        },
        ..Config::default()
    })
    .await;
    let r = rig.fetch("http://example.test:8443/").await;
    assert_eq!(r.text().await.unwrap(), "through socks");
    server.join().unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn replay_tool_sends_directly_and_records_a_replayed_flow() {
    let up = origin(200, "pong").await;
    let rig = rig(mcp_config()).await;
    let sent = rig
        .mcp("proxyproxy_replay", json!({"method": "POST", "url": url(&up, "/echo"), "headers": {"accept-encoding": "gzip, br"}, "body": "ping"}))
        .await;
    assert_eq!(sent["status"], 200);
    let enc = last(&up)
        .await
        .headers
        .get("accept-encoding")
        .map(|v| v.to_str().unwrap().to_string());
    assert!(!enc.unwrap_or_default().contains("gzip"));
    let flows = rig.get("/flows").await;
    assert_eq!(
        (flows.as_array().unwrap().len(), flows[0]["tag"].as_str()),
        (1, Some("Replayed"))
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn har_export_from_the_api_reimports_the_captured_traffic() {
    let up = origin(200, "body").await;
    let rig = rig(Config::default()).await;
    rig.fetch(&url(&up, "/a")).await;
    rig.client
        .post(url(&up, "/b"))
        .header("content-type", "text/plain")
        .body("sent")
        .send()
        .await
        .unwrap();
    rig.answered(2).await;
    let har = rig.get("/har").await;
    let back = crate::services::importer::import_bytes(har.to_string().as_bytes()).unwrap();
    let mut got: Vec<(String, String, Option<u16>)> = back
        .iter()
        .map(|f| (f.method.clone(), f.path.clone(), f.status))
        .collect();
    got.sort();
    assert_eq!(
        got,
        [
            ("GET".into(), "/a".into(), Some(200)),
            ("POST".into(), "/b".into(), Some(200))
        ]
    );
    assert_eq!(
        back.iter()
            .find(|f| f.method == "POST")
            .unwrap()
            .request
            .body,
        &b"sent"[..]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn rest_reads_keep_working_while_mcp_is_refused() {
    let rig = rig(Config::default()).await;
    let r = rig
        .direct
        .post(format!("http://127.0.0.1:{}/mcp", rig.api))
        .header("content-type", "application/json")
        .body(json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"}).to_string())
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 403);
    assert_eq!(rig.get("/flows").await, json!([]));
}
