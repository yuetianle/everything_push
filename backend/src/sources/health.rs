use chrono::{DateTime, Utc};
use serde::Serialize;
use std::collections::BTreeMap;
use std::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthEvent {
    WentDown,
    Recovered,
}

#[derive(Debug, Clone, Serialize)]
pub struct SourceSnapshot {
    pub name: String,
    pub is_down: bool,
    pub consecutive_fails: u32,
    pub last_ok_at: Option<DateTime<Utc>>,
}

pub struct HealthTracker {
    consecutive_fails: u32,
    is_down: bool,
}

impl HealthTracker {
    pub fn new() -> Self {
        Self { consecutive_fails: 0, is_down: false }
    }

    pub fn report(&mut self, ok: bool) -> Option<HealthEvent> {
        if ok {
            self.consecutive_fails = 0;
            if self.is_down {
                self.is_down = false;
                return Some(HealthEvent::Recovered);
            }
            None
        } else {
            self.consecutive_fails += 1;
            if !self.is_down && self.consecutive_fails >= 3 {
                self.is_down = true;
                return Some(HealthEvent::WentDown);
            }
            None
        }
    }
}

impl Default for HealthTracker {
    fn default() -> Self { Self::new() }
}

#[derive(Default)]
struct Inner {
    tracker: HealthTracker,
    last_ok_at: Option<DateTime<Utc>>,
}

#[derive(Default)]
pub struct HealthRegistry {
    inner: Mutex<BTreeMap<String, Inner>>,
}

impl HealthRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn report(&self, name: &str, ok: bool) -> Option<HealthEvent> {
        let mut map = self.inner.lock().unwrap();
        let e = map.entry(name.to_string()).or_default();
        let ev = e.tracker.report(ok);
        if ok {
            e.last_ok_at = Some(Utc::now());
        }
        ev
    }

    pub fn snapshots(&self) -> Vec<SourceSnapshot> {
        let map = self.inner.lock().unwrap();
        map.iter().map(|(name, e)| SourceSnapshot {
            name: name.clone(),
            is_down: e.tracker.is_down,
            consecutive_fails: e.tracker.consecutive_fails,
            last_ok_at: e.last_ok_at,
        }).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_consecutive_failures_then_recovery() {
        let mut t = HealthTracker::new();
        assert_eq!(t.report(false), None);
        assert_eq!(t.report(false), None);
        assert_eq!(t.report(false), Some(HealthEvent::WentDown)); // 第 3 次
        assert_eq!(t.report(false), None);                         // 已 down 不重复
        assert_eq!(t.report(true), Some(HealthEvent::Recovered));
        assert_eq!(t.report(true), None);
    }

    #[test]
    fn intermittent_failure_never_goes_down() {
        let mut t = HealthTracker::new();
        for _ in 0..10 {
            assert_eq!(t.report(false), None);
            assert_eq!(t.report(true), None); // 成功即清零
        }
    }

    #[test]
    fn registry_stores_snapshots() {
        let r = HealthRegistry::new();
        r.report("boc", false);
        r.report("boc", false);
        r.report("boc", false);
        r.report("api", true);
        let s = r.snapshots();
        let boc = s.iter().find(|x| x.name == "boc").unwrap();
        assert!(boc.is_down && boc.consecutive_fails == 3);
        let api = s.iter().find(|x| x.name == "api").unwrap();
        assert!(!api.is_down && api.last_ok_at.is_some());
    }
}
