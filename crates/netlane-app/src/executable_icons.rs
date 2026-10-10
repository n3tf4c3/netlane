//! UI cache: at most two background extractions, no disk I/O during rendering.
use gpui::RenderImage;
use netlane_windows::icons::{ICON_SIZE, IconPixels, IconProvider, local_executable_path};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::{Duration, Instant},
};
const CAPACITY: usize = 128;
const WORKERS: usize = 2;
const RECHECK: Duration = Duration::from_secs(5);
struct Entry {
    path: String,
    image: Option<Arc<RenderImage>>,
    checked: Option<Instant>,
    ticket: Option<u64>,
    used: u64,
}
pub struct IconRequest {
    pub path: String,
    pub key: String,
    pub ticket: u64,
    pub provider: Arc<IconProvider>,
}
pub struct ExecutableIcons {
    provider: Option<Arc<IconProvider>>,
    entries: HashMap<String, Entry>,
    clock: u64,
    busy: usize,
    active: HashSet<u64>,
}
impl ExecutableIcons {
    pub fn new(enabled: bool) -> Self {
        Self {
            provider: enabled.then(|| Arc::new(IconProvider::default())),
            entries: HashMap::new(),
            clock: 0,
            busy: 0,
            active: HashSet::new(),
        }
    }
    pub fn image(&self, path: Option<&str>) -> Option<Arc<RenderImage>> {
        let key = local_executable_path(path?)?.to_lowercase();
        self.entries.get(&key)?.image.clone()
    }
    pub fn prepare(
        &mut self,
        paths: Vec<String>,
        now: Instant,
    ) -> (Vec<IconRequest>, Vec<Arc<RenderImage>>) {
        let Some(provider) = &self.provider else {
            return (vec![], vec![]);
        };
        let mut wanted = HashSet::new();
        let mut paths = paths
            .into_iter()
            .filter_map(|p| local_executable_path(&p))
            .filter_map(|path| {
                let key = path.to_lowercase();
                wanted.insert(key.clone()).then_some((key, path))
            })
            .take(CAPACITY)
            .collect::<Vec<_>>();
        // Keep only bounded metadata for paths decorating the current view.
        wanted = paths.iter().map(|(key, _)| key.clone()).collect();
        let mut dropped = Vec::new();
        while self.entries.len()
            + paths
                .iter()
                .filter(|(k, _)| !self.entries.contains_key(k))
                .count()
            > CAPACITY
        {
            let victim = self
                .entries
                .iter()
                .filter(|(key, _)| !wanted.contains(*key))
                .min_by_key(|(_, v)| v.used)
                .map(|(k, _)| k.clone());
            let Some(victim) = victim else {
                break;
            };
            if let Some(image) = self.entries.remove(&victim).and_then(|entry| entry.image) {
                dropped.push(image);
            }
        }
        let mut requests = Vec::new();
        for (key, path) in paths.drain(..) {
            self.clock += 1;
            let entry = self.entries.entry(key.clone()).or_insert(Entry {
                path,
                image: None,
                checked: None,
                ticket: None,
                used: self.clock,
            });
            entry.used = self.clock;
            if self.busy < WORKERS
                && entry.ticket.is_none()
                && entry
                    .checked
                    .is_none_or(|last| now.duration_since(last) >= RECHECK)
            {
                entry.ticket = Some(self.clock);
                self.busy += 1;
                self.active.insert(self.clock);
                requests.push(IconRequest {
                    path: entry.path.clone(),
                    key,
                    ticket: self.clock,
                    provider: provider.clone(),
                });
            }
        }
        (requests, dropped)
    }
    pub fn complete(
        &mut self,
        key: &str,
        ticket: u64,
        pixels: Option<IconPixels>,
        now: Instant,
    ) -> Option<Arc<RenderImage>> {
        if !self.active.remove(&ticket) {
            return None;
        }
        self.busy = self.busy.saturating_sub(1);
        let entry = self.entries.get_mut(key)?;
        if entry.ticket != Some(ticket) {
            return None;
        } // Ignore an evicted/recreated path's old result.
        entry.ticket = None;
        entry.checked = Some(now);
        if let (Some(old), Some(new)) = (&entry.image, &pixels)
            && old.as_bytes(0) == Some(new.bgra.as_slice())
        {
            return None;
        }
        let image = pixels
            .and_then(|pixels| image::RgbaImage::from_raw(ICON_SIZE, ICON_SIZE, pixels.bgra))
            .map(|buffer| Arc::new(RenderImage::new(vec![image::Frame::new(buffer)])));
        let old = entry.image.take();
        entry.image = image;
        old
    }
    #[cfg(feature = "gui-test")]
    pub fn stats(&self) -> (usize, usize) {
        (self.entries.len(), self.busy)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn pixels(value: u8) -> IconPixels {
        IconPixels {
            bgra: vec![value; (ICON_SIZE * ICON_SIZE * 4) as usize],
        }
    }
    #[test]
    fn work_and_cache_are_bounded_and_late_results_cannot_replace_a_recreated_path() {
        let mut cache = ExecutableIcons::new(true);
        let now = Instant::now();
        let first = cache
            .prepare(vec![r"C:\First.exe".into()], now)
            .0
            .pop()
            .unwrap();
        assert_eq!(cache.prepare(vec![r"c:\first.exe".into()], now).0.len(), 0);
        let next = (0..128).map(|n| format!("C:\\Page{n}.exe")).collect();
        let (pending, _) = cache.prepare(next, now);
        assert_eq!(cache.entries.len(), 128);
        assert_eq!(pending.len(), 1);
        assert_eq!(cache.busy, 2);
        cache.complete(&first.key, first.ticket, Some(pixels(1)), now);
        let fresh = cache
            .prepare(vec![r"C:\First.exe".into()], now)
            .0
            .pop()
            .unwrap();
        assert_ne!(fresh.ticket, first.ticket);
        cache.complete(&first.key, first.ticket, Some(pixels(2)), now);
        assert!(cache.image(Some(r"C:\First.exe")).is_none());
        cache.complete(&fresh.key, fresh.ticket, Some(pixels(3)), now);
        assert_eq!(
            cache
                .image(Some(r"c:\first.exe"))
                .unwrap()
                .as_bytes(0)
                .unwrap()[0],
            3
        );
    }
    #[test]
    fn demo_never_schedules_io_and_unchanged_images_reuse_the_gpu_id() {
        let now = Instant::now();
        let mut demo = ExecutableIcons::new(false);
        assert!(demo.prepare(vec![r"C:\app.exe".into()], now).0.is_empty());
        let mut cache = ExecutableIcons::new(true);
        let request = cache
            .prepare(vec![r"C:\app.exe".into()], now)
            .0
            .pop()
            .unwrap();
        cache.complete(&request.key, request.ticket, Some(pixels(1)), now);
        let before = cache.image(Some(r"C:\app.exe")).unwrap().id;
        assert!(cache.prepare(vec![r"C:\app.exe".into()], now).0.is_empty());
        let later = now + RECHECK;
        let request = cache
            .prepare(vec![r"C:\app.exe".into()], later)
            .0
            .pop()
            .unwrap();
        assert!(
            cache
                .complete(&request.key, request.ticket, Some(pixels(1)), later)
                .is_none()
        );
        assert_eq!(cache.image(Some(r"C:\app.exe")).unwrap().id, before);
        let request = cache
            .prepare(vec![r"C:\app.exe".into()], later + RECHECK)
            .0
            .pop()
            .unwrap();
        assert!(
            cache
                .complete(&request.key, request.ticket, None, later + RECHECK)
                .is_some()
        );
        assert!(cache.image(Some(r"C:\app.exe")).is_none());
    }
}
