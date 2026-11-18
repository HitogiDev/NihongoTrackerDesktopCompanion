//! The OCR's own thread and its queue.
//!
//! Inference takes seconds per page and blocks whatever runs it, so it never
//! runs on the watcher's tick or a tokio worker: one OS thread takes jobs from
//! a queue, best first:
//!
//! 1. a box the user drew and wants read, or owocr's connection test
//!    (someone is waiting for them);
//! 2. the pages on screen in the reader;
//! 3. the pages after them, so turning the page finds text;
//! 4. "process volume", page by page in order.
//!
//! A page asked for twice is one job, at the better priority. The engine is
//! loaded on the first job and dropped after a while without work (it holds
//! a few hundred MB).

use std::collections::{HashMap, HashSet};
use std::sync::mpsc;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use serde::Serialize;

/// How long the engine stays loaded without work.
const UNLOAD_AFTER: Duration = Duration::from_secs(300);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum Priority {
    Region = 0,
    Visible = 1,
    Ahead = 2,
    Volume = 3,
}

pub enum Work {
    Page,
    /// Read the text inside one box of the page; the answer goes back.
    Region { bbox: [f64; 4], reply: mpsc::Sender<Result<String, String>> },
    /// "Test connection" to owocr on a port. It goes through the queue so
    /// that owocr never has two images of ours at once (it has no request
    /// ids). Not a page: `volume_id` is empty.
    Probe { port: u16, reply: mpsc::Sender<super::owocr::OwocrProbe> },
}

impl Work {
    /// Someone waits for the answer: never dropped with a volume's pages.
    fn awaited(&self) -> bool {
        matches!(self, Work::Region { .. } | Work::Probe { .. })
    }
}

pub struct Job {
    pub volume_id: String,
    pub page: i64,
    pub priority: Priority,
    seq: u64,
    pub work: Work,
}

/// A "process volume" in progress, as the UI shows it.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct OcrVolumeRun {
    pub volume_id: String,
    pub title: String,
    pub done: i64,
    pub total: i64,
    pub failed: i64,
}

#[derive(Default)]
pub struct Queue {
    jobs: Vec<Job>,
    seq: u64,
    /// Volumes being processed whole.
    pub runs: HashMap<String, OcrVolumeRun>,
    /// The page being read now.
    pub current: Option<(String, i64)>,
    pub stopping: bool,
    /// The engine is being loaded.
    pub loading: bool,
    /// Where it runs once loaded (`directml`, `coreml`, `cpu`).
    pub provider: Option<&'static str>,
    /// Why it could not load, or why the last page failed.
    pub error: Option<String>,
    /// Drop the engine before the next job: its files or variant changed.
    pub reload: bool,
}

impl Queue {
    /// Adds a page, or raises the priority of the same page already waiting.
    /// `false` when it was already there at the same or a better priority.
    pub fn push(&mut self, volume_id: &str, page: i64, priority: Priority, work: Work) -> bool {
        if let Work::Page = work {
            if let Some(job) = self.jobs.iter_mut().find(|job| matches!(job.work, Work::Page) && job.volume_id == volume_id && job.page == page) {
                if priority < job.priority {
                    job.priority = priority;
                    return true;
                }
                return false;
            }
            if self.current.as_ref().is_some_and(|(volume, current)| volume == volume_id && *current == page) {
                return false;
            }
        }
        self.seq += 1;
        self.jobs.push(Job { volume_id: volume_id.to_string(), page, priority, seq: self.seq, work });
        true
    }

    /// The best job: highest priority, then the oldest.
    pub fn pop(&mut self) -> Option<Job> {
        let best = self.jobs.iter().enumerate().min_by_key(|(_, job)| (job.priority, job.seq)).map(|(index, _)| index)?;
        Some(self.jobs.swap_remove(best))
    }

    /// Drops a volume's queued pages of `priority` or worse (a region is
    /// never dropped: someone waits for it).
    pub fn drop_volume(&mut self, volume_id: &str, priority: Priority) -> usize {
        let before = self.jobs.len();
        self.jobs.retain(|job| job.volume_id != volume_id || job.priority < priority || job.work.awaited());
        before - self.jobs.len()
    }

    /// Drops the read-ahead of every volume but `keep`: the reader moved on.
    pub fn drop_ahead_except(&mut self, keep: &str) {
        self.jobs.retain(|job| job.volume_id == keep || job.priority != Priority::Ahead);
    }

    pub fn queued_pages(&self, volume_id: &str) -> HashSet<i64> {
        self.jobs.iter().filter(|job| job.volume_id == volume_id && !matches!(job.work, Work::Probe { .. })).map(|job| job.page).collect()
    }

    /// Takes every page and region waiting (the engine cannot read them);
    /// connection tests stay, they do not need it.
    pub fn take_reads(&mut self) -> Vec<Job> {
        let (probes, reads) = std::mem::take(&mut self.jobs).into_iter().partition(|job| matches!(job.work, Work::Probe { .. }));
        self.jobs = probes;
        reads
    }

    pub fn is_empty(&self) -> bool {
        self.jobs.is_empty()
    }
}

pub type Shared = Arc<(Mutex<Queue>, Condvar)>;

/// Waits for the next job. `None` after `UNLOAD_AFTER` without one (the
/// caller drops the engine and waits again) or when stopping.
pub fn next(shared: &Shared, idle: bool) -> Result<Job, bool> {
    let (lock, ready) = &**shared;
    let mut queue = lock.lock().unwrap();
    loop {
        if queue.stopping {
            return Err(true);
        }
        if let Some(job) = queue.pop() {
            queue.current = Some((job.volume_id.clone(), job.page));
            return Ok(job);
        }
        if idle {
            queue = ready.wait(queue).unwrap();
        } else {
            let (guard, timeout) = ready.wait_timeout(queue, UNLOAD_AFTER).unwrap();
            queue = guard;
            if timeout.timed_out() && queue.is_empty() {
                return Err(false);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn order(queue: &mut Queue) -> Vec<(String, i64)> {
        std::iter::from_fn(|| queue.pop()).map(|job| (job.volume_id, job.page)).collect()
    }

    #[test]
    fn the_visible_page_goes_first_then_the_next_ones_then_the_volume() {
        let mut queue = Queue::default();
        for page in 0..4 {
            queue.push("v", page, Priority::Volume, Work::Page);
        }
        queue.push("v", 10, Priority::Ahead, Work::Page);
        queue.push("v", 11, Priority::Ahead, Work::Page);
        queue.push("v", 9, Priority::Visible, Work::Page);
        let (reply, _answer) = mpsc::channel();
        queue.push("v", 9, Priority::Region, Work::Region { bbox: [0.0; 4], reply });
        let order = order(&mut queue);
        let pages: Vec<i64> = order.iter().map(|(_, page)| *page).collect();
        assert_eq!(pages, [9, 9, 10, 11, 0, 1, 2, 3]);
    }

    #[test]
    fn a_page_asked_again_is_one_job_at_the_better_priority() {
        let mut queue = Queue::default();
        assert!(queue.push("v", 3, Priority::Volume, Work::Page));
        assert!(queue.push("v", 1, Priority::Volume, Work::Page));
        assert!(!queue.push("v", 3, Priority::Volume, Work::Page));
        assert!(queue.push("v", 3, Priority::Visible, Work::Page), "raised");
        assert!(!queue.push("v", 3, Priority::Ahead, Work::Page), "not lowered");
        assert_eq!(order(&mut queue), [("v".to_string(), 3), ("v".to_string(), 1)]);
    }

    #[test]
    fn cancelling_a_volume_keeps_what_is_on_screen() {
        let mut queue = Queue::default();
        queue.push("v", 0, Priority::Visible, Work::Page);
        queue.push("v", 1, Priority::Volume, Work::Page);
        queue.push("v", 2, Priority::Ahead, Work::Page);
        queue.push("w", 1, Priority::Ahead, Work::Page);
        assert_eq!(queue.drop_volume("v", Priority::Volume), 1);
        queue.drop_ahead_except("v");
        assert_eq!(order(&mut queue), [("v".to_string(), 0), ("v".to_string(), 2)]);
    }

    #[test]
    fn a_connection_test_survives_what_drops_pages() {
        let mut queue = Queue::default();
        let (reply, _answer) = mpsc::channel();
        queue.push("", -1, Priority::Region, Work::Probe { port: 7331, reply });
        queue.push("v", 1, Priority::Volume, Work::Page);
        queue.push("v", 2, Priority::Visible, Work::Page);
        assert_eq!(queue.drop_volume("", Priority::Volume), 0);
        assert!(queue.queued_pages("").is_empty());
        let taken: Vec<i64> = queue.take_reads().into_iter().map(|job| job.page).collect();
        assert_eq!(taken.len(), 2);
        assert!(matches!(queue.pop().map(|job| job.work), Some(Work::Probe { port: 7331, .. })));
        assert!(queue.pop().is_none());
    }

    #[test]
    fn the_page_being_read_is_not_queued_again() {
        let mut queue = Queue::default();
        queue.current = Some(("v".into(), 4));
        assert!(!queue.push("v", 4, Priority::Visible, Work::Page));
        assert!(queue.push("v", 5, Priority::Visible, Work::Page));
    }
}
