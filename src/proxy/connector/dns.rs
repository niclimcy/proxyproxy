use crate::domain::config::Shared;
use crate::domain::policy::glob;
use hudsucker::hyper_util::client::legacy::connect::dns::Name;
use std::{
    future::Future,
    io,
    net::{IpAddr, SocketAddr},
    pin::Pin,
    task::{Context, Poll},
};
use tower_service::Service;

#[derive(Clone)]
pub struct Resolver {
    cfg: Shared,
}

impl Resolver {
    pub fn new(cfg: Shared) -> Self {
        Self { cfg }
    }
}

pub fn lookup_override(overrides: &[(String, String)], host: &str) -> Option<IpAddr> {
    let h = host.trim_end_matches('.').to_lowercase();
    overrides.iter().find_map(|(pat, ip)| {
        let p = pat.trim().to_lowercase();
        let hit = !p.is_empty() && if p.contains('*') || p.contains('?') { glob(p.as_bytes(), h.as_bytes()) } else { p == h };
        if hit { ip.trim().parse().ok() } else { None }
    })
}

impl Service<Name> for Resolver {
    type Response = std::vec::IntoIter<SocketAddr>;
    type Error = io::Error;
    type Future = Pin<Box<dyn Future<Output = io::Result<Self::Response>> + Send>>;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, name: Name) -> Self::Future {
        let host = name.as_str().to_string();
        let hit = lookup_override(&self.cfg.lock().unwrap().dns_overrides, &host);
        Box::pin(async move {
            if let Some(ip) = hit {
                return Ok(vec![SocketAddr::new(ip, 0)].into_iter());
            }
            Ok(tokio::net::lookup_host((host.as_str(), 0)).await?.collect::<Vec<_>>().into_iter())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ov(v: &[(&str, &str)]) -> Vec<(String, String)> {
        v.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect()
    }

    #[test]
    fn overrides_match() {
        let o = ov(&[("api.test", "10.0.0.1"), ("*.dev.test", " ::1 "), ("bad.test", "nope")]);
        assert_eq!(lookup_override(&o, "API.test"), Some("10.0.0.1".parse().unwrap()));
        assert_eq!(lookup_override(&o, "a.dev.test."), Some("::1".parse().unwrap()));
        assert_eq!(lookup_override(&o, "other.test"), None);
        assert_eq!(lookup_override(&o, "bad.test"), None);
    }

    #[tokio::test]
    async fn resolver_uses_override() {
        let cfg: Shared = Default::default();
        cfg.lock().unwrap().dns_overrides = ov(&[("x.invalid", "127.0.0.9")]);
        let mut r = Resolver::new(cfg);
        let addrs: Vec<_> = r.call("x.invalid".parse().unwrap()).await.unwrap().collect();
        assert_eq!(addrs, vec![SocketAddr::from(([127, 0, 0, 9], 0))]);
    }
}
