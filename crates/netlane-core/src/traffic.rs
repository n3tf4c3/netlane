use crate::Adapter;
use std::{
    collections::{BTreeMap, VecDeque},
    time::Duration,
};

#[derive(Clone, Debug, Default)]
pub struct Usage {
    pub download_mbps: Option<f64>,
    pub upload_mbps: Option<f64>,
    pub received: u64,
    pub sent: u64,
    pub history: VecDeque<(Option<f64>, Option<f64>)>,
    previous: Option<(Duration, u64, u64)>,
}
#[derive(Default)]
pub struct TrafficTracker {
    pub usage: BTreeMap<String, Usage>,
}
impl TrafficTracker {
    pub fn sample(&mut self, at: Duration, adapters: &[Adapter]) {
        self.usage
            .retain(|id, _| adapters.iter().any(|a| a.id == *id));
        for a in adapters {
            let u = self.usage.entry(a.id.clone()).or_default();
            u.download_mbps = None;
            u.upload_mbps = None;
            if let (Some(rx), Some(tx)) = (a.received, a.sent) {
                if let Some((previous_at, prx, ptx)) = u.previous
                    && let Some(elapsed) = at.checked_sub(previous_at)
                    && !elapsed.is_zero()
                    && elapsed <= Duration::from_secs(10)
                    && rx >= prx
                    && tx >= ptx
                {
                    u.download_mbps =
                        Some((rx - prx) as f64 * 8. / 1_000_000. / elapsed.as_secs_f64());
                    u.upload_mbps =
                        Some((tx - ptx) as f64 * 8. / 1_000_000. / elapsed.as_secs_f64());
                    u.received = u.received.saturating_add(rx - prx);
                    u.sent = u.sent.saturating_add(tx - ptx);
                }
                u.previous = Some((at, rx, tx));
            } else {
                u.previous = None;
            }
            u.history.push_back((u.download_mbps, u.upload_mbps));
            if u.history.len() > 30 {
                u.history.pop_front();
            }
        }
    }
    pub fn reset(&mut self) {
        self.usage.clear();
    }
}
