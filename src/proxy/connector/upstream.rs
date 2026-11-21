use super::dns::Resolver;
use crate::domain::config::{Shared, UpstreamMode};
use crate::domain::policy::glob;
use base64::Engine;
use hudsucker::hyper::{Uri, rt};
use hudsucker::hyper_util::{
    client::legacy::connect::{Connected, Connection, HttpConnector},
    rt::TokioIo,
};
use rquickjs::{Context, Function, Runtime};
use std::{
    future::Future,
    net::{IpAddr, ToSocketAddrs},
    pin::Pin,
    task::{Context as TaskCx, Poll},
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};
use tower_service::Service;

type BoxErr = Box<dyn std::error::Error + Send + Sync>;

#[derive(Clone, Debug, PartialEq)]
pub enum ProxyKind {
    Http,
    Socks5,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProxyAddr {
    pub kind: ProxyKind,
    pub host: String,
    pub port: u16,
    pub auth: Option<String>,
}

pub fn parse_proxy(s: &str) -> Option<ProxyAddr> {
    let s = s.trim();
    let (kind, s) = if let Some(s) = s
        .strip_prefix("socks5://")
        .or_else(|| s.strip_prefix("socks://"))
    {
        (ProxyKind::Socks5, s)
    } else {
        (ProxyKind::Http, s.strip_prefix("http://").unwrap_or(s))
    };
    let s = s.trim_end_matches('/');
    if s.is_empty() || s.contains("://") {
        return None;
    }
    let (cred, hp) = match s.rsplit_once('@') {
        Some((c, h)) => (Some(c), h),
        None => (None, s),
    };
    let (host, port) = match hp.rsplit_once(':') {
        Some((h, p)) if !p.ends_with(']') => (h, p.parse().ok()?),
        _ => (hp, 8080),
    };
    if host.is_empty() {
        return None;
    }
    let auth = cred.map(|c| {
        if kind == ProxyKind::Http {
            format!(
                "Basic {}",
                base64::engine::general_purpose::STANDARD.encode(c)
            )
        } else {
            c.to_string()
        }
    });
    Some(ProxyAddr {
        kind,
        host: host.to_string(),
        port,
        auth,
    })
}

// First usable entry wins; DIRECT (or nothing usable) means no proxy.
pub fn parse_pac_result(s: &str) -> Option<ProxyAddr> {
    for entry in s.split(';') {
        let mut it = entry.split_whitespace();
        match (
            it.next().map(|k| k.to_ascii_uppercase()).as_deref(),
            it.next(),
        ) {
            (Some("DIRECT"), _) => return None,
            (Some("PROXY" | "HTTP"), Some(hp)) => {
                if let Some(p) = parse_proxy(hp) {
                    return Some(p);
                }
            }
            (Some("SOCKS" | "SOCKS5"), Some(hp)) => {
                if let Some(p) = parse_proxy(&format!("socks5://{hp}")) {
                    return Some(p);
                }
            }
            _ => {}
        }
    }
    None
}

const PRELUDE: &str = r#"
function dnsResolve(h){var r=__dns(h);return r===''?null:r}
function isResolvable(h){return dnsResolve(h)!==null}
function isPlainHostName(h){return h.indexOf('.')<0}
function dnsDomainIs(h,d){return h.length>=d.length&&h.substring(h.length-d.length)===d}
function localHostOrDomainIs(h,hd){return h===hd||hd.indexOf(h+'.')===0}
function shExpMatch(s,p){return __glob(s,p)}
function dnsDomainLevels(h){return h.split('.').length-1}
function myIpAddress(){return '127.0.0.1'}
function __n(s){var p=s.split('.');if(p.length!==4)return null;var n=0;for(var i=0;i<4;i++){var x=parseInt(p[i],10);if(!(x>=0&&x<=255))return null;n=n*256+x}return n}
function isInNet(h,pat,mask){var ip=/^[0-9.]+$/.test(h)?h:dnsResolve(h);if(!ip)return false;var a=__n(ip),b=__n(pat),m=__n(mask);if(a===null||b===null||m===null)return false;return ((a&m)>>>0)===((b&m)>>>0)}
"#;

fn resolve_v4(h: &str) -> String {
    if let Ok(ip) = h.parse::<IpAddr>() {
        return ip.to_string();
    }
    (h, 0)
        .to_socket_addrs()
        .ok()
        .and_then(|mut a| a.find(|a| a.is_ipv4()))
        .map(|a| a.ip().to_string())
        .unwrap_or_default()
}

pub fn eval_pac(code: &str, url: &str, host: &str) -> Result<String, String> {
    let rt = Runtime::new().map_err(|e| e.to_string())?;
    rt.set_memory_limit(32 << 20);
    let deadline = Instant::now() + Duration::from_secs(3);
    rt.set_interrupt_handler(Some(Box::new(move || Instant::now() > deadline)));
    let js = Context::full(&rt).map_err(|e| e.to_string())?;
    js.with(|c| {
        let g = c.globals();
        g.set(
            "__dns",
            Function::new(c.clone(), |h: String| resolve_v4(&h)).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        g.set(
            "__glob",
            Function::new(c.clone(), |s: String, p: String| {
                glob(p.as_bytes(), s.as_bytes())
            })
            .map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        let lit = |s: &str| serde_json::to_string(s).unwrap_or_default();
        let prog = format!(
            "{PRELUDE}\n{code}\n;String(FindProxyForURL({}, {}))",
            lit(url),
            lit(host)
        );
        c.eval::<String, _>(prog).map_err(|e| match e {
            rquickjs::Error::Exception => format!("{:?}", c.catch()),
            e => e.to_string(),
        })
    })
}

fn pick(cfg: &Shared, uri: &Uri) -> Result<Option<ProxyAddr>, BoxErr> {
    let up = cfg.lock().unwrap().upstream.clone();
    match up.mode {
        UpstreamMode::Off => Ok(None),
        UpstreamMode::Manual | UpstreamMode::Socks => {
            let url = if up.mode == UpstreamMode::Socks && !up.url.contains("://") {
                format!("socks5://{}", up.url)
            } else {
                up.url.clone()
            };
            let proxy = parse_proxy(&url).ok_or("invalid upstream proxy URL")?;
            if up.mode == UpstreamMode::Socks && proxy.kind != ProxyKind::Socks5 {
                return Err("SOCKS5 mode requires a SOCKS proxy URL".into());
            }
            Ok(Some(proxy))
        }
        UpstreamMode::Pac => {
            let code =
                std::fs::read_to_string(&up.pac_path).map_err(|e| format!("PAC file: {e}"))?;
            let host = uri.host().unwrap_or("");
            let url = match uri.scheme_str() {
                Some("https") => format!(
                    "https://{}{}",
                    uri.authority().map(|a| a.as_str()).unwrap_or(host),
                    uri.path_and_query().map(|p| p.as_str()).unwrap_or("/")
                ),
                _ => uri.to_string(),
            };
            let r = eval_pac(&code, &url, host).map_err(|e| format!("PAC: {e}"))?;
            Ok(parse_pac_result(&r))
        }
    }
}

pub struct Conn {
    io: TokioIo<TcpStream>,
    proxied: bool,
}

impl rt::Read for Conn {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut TaskCx<'_>,
        buf: rt::ReadBufCursor<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.io).poll_read(cx, buf)
    }
}

impl rt::Write for Conn {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut TaskCx<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.io).poll_write(cx, buf)
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut TaskCx<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.io).poll_flush(cx)
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut TaskCx<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.io).poll_shutdown(cx)
    }
}

impl Connection for Conn {
    fn connected(&self) -> Connected {
        Connected::new().proxy(self.proxied)
    }
}

// Limitation: passthrough CONNECT tunnels and WebSocket upstream bypass DNS overrides and the upstream proxy.
#[derive(Clone)]
pub struct UpstreamConnector {
    cfg: Shared,
    inner: HttpConnector<Resolver>,
}

impl UpstreamConnector {
    pub fn new(cfg: Shared) -> Self {
        let mut inner = HttpConnector::new_with_resolver(Resolver::new(cfg.clone()));
        inner.enforce_http(false);
        Self { cfg, inner }
    }
}

async fn tunnel(io: &mut TcpStream, target: &str, p: &ProxyAddr) -> Result<(), BoxErr> {
    let auth = p
        .auth
        .as_ref()
        .map(|a| format!("Proxy-Authorization: {a}\r\n"))
        .unwrap_or_default();
    io.write_all(format!("CONNECT {target} HTTP/1.1\r\nHost: {target}\r\n{auth}\r\n").as_bytes())
        .await?;
    let mut buf = Vec::with_capacity(256);
    while !buf.ends_with(b"\r\n\r\n") {
        if buf.len() > 16 * 1024 {
            return Err("proxy CONNECT response too large".into());
        }
        let mut b = [0u8; 1];
        if io.read(&mut b).await? == 0 {
            return Err("proxy closed connection during CONNECT".into());
        }
        buf.push(b[0]);
    }
    let line = String::from_utf8_lossy(&buf)
        .lines()
        .next()
        .unwrap_or("")
        .to_string();
    match line.split_whitespace().nth(1) {
        Some("200") => Ok(()),
        _ => Err(format!("proxy CONNECT refused: {line}").into()),
    }
}

async fn socks5(
    io: &mut TcpStream,
    host: &str,
    port: u16,
    auth: Option<&str>,
) -> Result<(), BoxErr> {
    let methods = if auth.is_some() {
        &[0, 2][..]
    } else {
        &[0][..]
    };
    io.write_all(&[5, methods.len() as u8]).await?;
    io.write_all(methods).await?;
    let mut response = [0; 2];
    io.read_exact(&mut response).await?;
    if response[0] != 5 {
        return Err("invalid SOCKS5 handshake".into());
    }
    if response[1] == 2 {
        let credentials = auth.ok_or("SOCKS5 proxy requested authentication")?;
        let (user, password) = credentials
            .split_once(':')
            .ok_or("SOCKS5 credentials must be user:password")?;
        if user.len() > u8::MAX as usize || password.len() > u8::MAX as usize {
            return Err("SOCKS5 credentials are too long".into());
        }
        io.write_all(&[1, user.len() as u8]).await?;
        io.write_all(user.as_bytes()).await?;
        io.write_all(&[password.len() as u8]).await?;
        io.write_all(password.as_bytes()).await?;
        let mut auth_response = [0; 2];
        io.read_exact(&mut auth_response).await?;
        if auth_response != [1, 0] {
            return Err("SOCKS5 proxy authentication failed".into());
        }
    } else if response[1] != 0 {
        return Err("SOCKS5 proxy does not support an offered authentication method".into());
    }

    let host_bytes = host.as_bytes();
    if host_bytes.len() > u8::MAX as usize {
        return Err("SOCKS5 target host is too long".into());
    }
    io.write_all(&[5, 1, 0, 3, host_bytes.len() as u8]).await?;
    io.write_all(host_bytes).await?;
    io.write_all(&port.to_be_bytes()).await?;
    let mut reply = [0; 4];
    io.read_exact(&mut reply).await?;
    if reply[0] != 5 || reply[1] != 0 {
        return Err(format!("SOCKS5 proxy refused connection (code {})", reply[1]).into());
    }
    let address_len = match reply[3] {
        1 => 4,
        3 => {
            let mut len = [0; 1];
            io.read_exact(&mut len).await?;
            len[0] as usize
        }
        4 => 16,
        _ => return Err("SOCKS5 proxy returned an invalid address type".into()),
    };
    let mut address = vec![0; address_len + 2];
    io.read_exact(&mut address).await?;
    Ok(())
}

impl Service<Uri> for UpstreamConnector {
    type Response = Conn;
    type Error = BoxErr;
    type Future = Pin<Box<dyn Future<Output = Result<Conn, BoxErr>> + Send>>;

    fn poll_ready(&mut self, _: &mut TaskCx<'_>) -> Poll<Result<(), BoxErr>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, uri: Uri) -> Self::Future {
        let cfg = self.cfg.clone();
        let mut inner = self.inner.clone();
        Box::pin(async move {
            let chosen = {
                let u = uri.clone();
                tokio::task::spawn_blocking(move || pick(&cfg, &u))
                    .await
                    .map_err(|e| e.to_string())??
            };
            let Some(p) = chosen else {
                let io = inner.call(uri).await?;
                return Ok(Conn { io, proxied: false });
            };
            let tls = uri.scheme_str() == Some("https");
            let proxy_uri: Uri = format!("http://{}:{}", p.host, p.port).parse()?;
            let mut io = inner.call(proxy_uri).await?;
            let host = uri.host().ok_or("missing host")?;
            let port = uri.port_u16().unwrap_or(if tls { 443 } else { 80 });
            match p.kind {
                ProxyKind::Http if tls => {
                    tunnel(io.inner_mut(), &format!("{host}:{port}"), &p).await?
                }
                ProxyKind::Http => {}
                ProxyKind::Socks5 => socks5(io.inner_mut(), host, port, p.auth.as_deref()).await?,
            }
            Ok(Conn {
                io,
                proxied: matches!(p.kind, ProxyKind::Http) && !tls,
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::config::{Config, Upstream};
    use std::sync::{Arc, Mutex};

    #[test]
    fn manual_parse() {
        let a = |kind, h: &str, p| {
            Some(ProxyAddr {
                kind,
                host: h.into(),
                port: p,
                auth: None,
            })
        };
        assert_eq!(
            parse_proxy("http://127.0.0.1:8888/"),
            a(ProxyKind::Http, "127.0.0.1", 8888)
        );
        assert_eq!(
            parse_proxy(" proxy.local "),
            a(ProxyKind::Http, "proxy.local", 8080)
        );
        assert_eq!(parse_proxy("proxy:3128"), a(ProxyKind::Http, "proxy", 3128));
        assert_eq!(
            parse_proxy("u:p@h:1").unwrap().auth.as_deref(),
            Some("Basic dTpw")
        );
        assert_eq!(parse_proxy("socks5://h:1").unwrap().kind, ProxyKind::Socks5);
        assert_eq!(
            parse_proxy("socks5://u:p@h:1").unwrap().auth.as_deref(),
            Some("u:p")
        );
        assert_eq!(parse_proxy(""), None);
        assert_eq!(parse_proxy("h:notaport"), None);
    }

    #[test]
    fn pac_result_parse() {
        assert_eq!(parse_pac_result("DIRECT"), None);
        assert_eq!(parse_pac_result("PROXY a:1; DIRECT").unwrap().port, 1);
        assert_eq!(
            parse_pac_result("SOCKS s:9").unwrap().kind,
            ProxyKind::Socks5
        );
        assert_eq!(parse_pac_result("proxy b:2").unwrap().host, "b");
        assert_eq!(parse_pac_result("DIRECT; PROXY a:1"), None);
        assert_eq!(parse_pac_result(""), None);
    }

    const PAC: &str = r#"function FindProxyForURL(url, host) {
        if (isPlainHostName(host) || dnsDomainIs(host, ".internal.test")) return "DIRECT";
        if (shExpMatch(url, "https://*.corp.test/*")) return "PROXY corp:3128";
        if (isInNet(host, "10.0.0.0", "255.0.0.0")) return "PROXY ten:8080";
        return "PROXY def:80; DIRECT";
    }"#;

    #[test]
    fn pac_eval() {
        let f = |u: &str, h: &str| parse_pac_result(&eval_pac(PAC, u, h).unwrap());
        assert_eq!(f("http://intranet/", "intranet"), None);
        assert_eq!(f("http://a.internal.test/", "a.internal.test"), None);
        assert_eq!(
            f("https://x.corp.test/p", "x.corp.test").unwrap().host,
            "corp"
        );
        assert_eq!(f("http://10.2.3.4/", "10.2.3.4").unwrap().host, "ten");
        assert_eq!(f("http://8.8.8.8/", "8.8.8.8").unwrap().host, "def");
        assert!(
            eval_pac(
                "function FindProxyForURL(){ throw new Error('x') }",
                "http://a/",
                "a"
            )
            .is_err()
        );
        assert!(
            eval_pac(
                "function FindProxyForURL(){ while(true){} }",
                "http://a/",
                "a"
            )
            .is_err()
        );
    }

    #[tokio::test]
    async fn connect_through_manual_proxy() {
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = l.local_addr().unwrap().port();
        let srv = tokio::spawn(async move {
            let (mut s, _) = l.accept().await.unwrap();
            let mut buf = vec![0u8; 512];
            let n = s.read(&mut buf).await.unwrap();
            s.write_all(b"HTTP/1.1 200 Connection established\r\n\r\n")
                .await
                .unwrap();
            String::from_utf8_lossy(&buf[..n]).into_owned()
        });
        let cfg: Shared = Arc::new(Mutex::new(Config {
            upstream: Upstream {
                mode: UpstreamMode::Manual,
                url: format!("127.0.0.1:{port}"),
                ..Default::default()
            },
            ..Default::default()
        }));
        let mut c = UpstreamConnector::new(cfg);
        assert!(
            c.call("https://example.test".parse().unwrap())
                .await
                .is_ok()
        );
        assert!(
            srv.await
                .unwrap()
                .starts_with("CONNECT example.test:443 HTTP/1.1\r\n")
        );
    }

    #[tokio::test]
    async fn connect_through_socks5_proxy_with_auth() {
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = l.local_addr().unwrap().port();
        let srv = tokio::spawn(async move {
            let (mut s, _) = l.accept().await.unwrap();
            let mut greeting = [0; 4];
            s.read_exact(&mut greeting).await.unwrap();
            assert_eq!(greeting, [5, 2, 0, 2]);
            s.write_all(&[5, 2]).await.unwrap();
            let mut auth = [0; 3];
            s.read_exact(&mut auth).await.unwrap();
            assert_eq!(auth, [1, 1, b'u']);
            let mut password = [0; 2];
            s.read_exact(&mut password).await.unwrap();
            assert_eq!(password, [1, b'p']);
            s.write_all(&[1, 0]).await.unwrap();
            let mut request = [0; 5];
            s.read_exact(&mut request).await.unwrap();
            assert_eq!(request[..4], [5, 1, 0, 3]);
            let host_len = request[4] as usize;
            let mut host = vec![0; host_len + 2];
            s.read_exact(&mut host).await.unwrap();
            assert_eq!(&host[..host_len], b"example.test");
            assert_eq!(
                u16::from_be_bytes([host[host_len], host[host_len + 1]]),
                8443
            );
            s.write_all(&[5, 0, 0, 1, 127, 0, 0, 1, 0, 80])
                .await
                .unwrap();
        });
        let cfg: Shared = Arc::new(Mutex::new(Config {
            upstream: Upstream {
                mode: UpstreamMode::Socks,
                url: format!("socks5://u:p@127.0.0.1:{port}"),
                ..Default::default()
            },
            ..Default::default()
        }));
        let mut c = UpstreamConnector::new(cfg);
        assert!(
            c.call("https://example.test:8443".parse().unwrap())
                .await
                .is_ok()
        );
        srv.await.unwrap();
    }
}
