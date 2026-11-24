use crate::domain::app::AppIdentity;
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::Mutex,
    time::{Duration, Instant},
};

// Client ports get reused by other processes, so a known answer only lasts this long.
const TTL: Duration = Duration::from_secs(60);

/// Process name and, once resolved, its identity. `None` means the lookup found no process.
pub type Found = Option<(String, Option<AppIdentity>)>;

/// A flow waiting on a lookup, with the platform its User-Agent suggests as a fallback.
pub type Waiting = (u64, Option<&'static str>);

enum Slot {
    Pending(Vec<Waiting>),
    Done(Found, Instant),
}

pub enum Join {
    /// first flow from this client: the caller runs the lookup
    Start,
    /// a lookup is running and will report this flow too
    Wait,
    Known(Found),
}

/// One process lookup per client connection. hudsucker clones the handler for every
/// request, so this has to live behind a shared pointer rather than in the handler.
#[derive(Default)]
pub struct AppCache(Mutex<HashMap<SocketAddr, Slot>>);

impl AppCache {
    pub fn join(&self, addr: SocketAddr, flow: Waiting, now: Instant) -> Join {
        let mut m = self.0.lock().unwrap();
        m.retain(|_, s| !matches!(s, Slot::Done(_, t) if now.duration_since(*t) > TTL));
        match m.get_mut(&addr) {
            Some(Slot::Done(found, _)) => Join::Known(found.clone()),
            Some(Slot::Pending(ids)) => {
                ids.push(flow);
                Join::Wait
            }
            None => {
                m.insert(addr, Slot::Pending(vec![flow]));
                Join::Start
            }
        }
    }

    pub fn waiting(&self, addr: SocketAddr) -> Vec<Waiting> {
        match self.0.lock().unwrap().get(&addr) {
            Some(Slot::Pending(ids)) => ids.clone(),
            _ => vec![],
        }
    }

    /// Stores the answer and returns every flow that waited for it.
    pub fn finish(&self, addr: SocketAddr, found: Found, now: Instant) -> Vec<Waiting> {
        match self.0.lock().unwrap().insert(addr, Slot::Done(found, now)) {
            Some(Slot::Pending(ids)) => ids,
            _ => vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addr(port: u16) -> SocketAddr {
        SocketAddr::from(([127, 0, 0, 1], port))
    }

    #[test]
    fn one_lookup_per_client_and_waiters_are_returned() {
        let (c, now) = (AppCache::default(), Instant::now());
        assert!(matches!(c.join(addr(1), (1, None), now), Join::Start));
        assert!(matches!(c.join(addr(1), (2, Some("iOS")), now), Join::Wait));
        assert!(matches!(c.join(addr(2), (3, None), now), Join::Start));
        assert_eq!(c.waiting(addr(1)).len(), 2);
        let found: Found = Some(("curl".into(), None));
        assert_eq!(c.finish(addr(1), found, now), vec![(1, None), (2, Some("iOS"))]);
        assert!(matches!(c.join(addr(1), (4, None), now), Join::Known(Some((n, _))) if n == "curl"));
    }

    #[test]
    fn known_answers_expire() {
        let (c, now) = (AppCache::default(), Instant::now());
        c.join(addr(1), (1, None), now);
        c.finish(addr(1), None, now);
        assert!(matches!(c.join(addr(1), (2, None), now + TTL * 2), Join::Start));
    }
}
