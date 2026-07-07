//! Generic scheduler that drives the event loop's wake-up cadence.
//!
//! Originally lived in the desktop fork as
//! `frontends/neoism/src/app/scheduler.rs`. Extracted here so web /
//! daemon callers can reuse the timer ordering + repeat semantics
//! without re-implementing the priority queue.
//!
//! The `Proxy` trait abstracts the dispatch step — native wraps
//! `neoism_window::event_loop::EventLoopProxy<EventPayload>`; web wraps
//! a `postMessage` / `setTimeout` shim.
//!
//! Originally retired from
//! <https://github.com/alacritty/alacritty/blob/e35e5ad14fce8456afdd89f2b392b9924bb27471/alacritty/src/scheduler.rs>
//! (Apache 2.0). Re-licensed-compatible with the rest of `neoism-ui`.

use std::collections::VecDeque;
use web_time::{Duration, Instant};

/// Available timer topics. Identical to the desktop fork's enum so
/// callers can `pub use neoism_ui::scheduler::Topic` and keep their
/// existing match arms.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Topic {
    Render,
    RenderRoute,
    UpdateConfig,
    CursorBlinking,
    UpdateTitles,
    SelectionScrolling,
    FileTree,
    FileTreeGitStatus,
}

/// ID uniquely identifying a timer.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct TimerId {
    pub topic: Topic,
    pub id: usize,
}

impl TimerId {
    pub fn new(topic: Topic, id: usize) -> Self {
        Self { topic, id }
    }
}

/// Event scheduled to be emitted at a specific time.
#[derive(Debug)]
pub struct Timer<E> {
    pub deadline: Instant,
    pub event: E,
    pub id: TimerId,
    interval: Option<Duration>,
}

/// Abstracts the event-dispatch side of the scheduler so the policy
/// stays free of winit types.
pub trait Proxy<E> {
    fn dispatch(&self, event: E);
}

// Note: native callers (desktop fork) implement `Proxy<E>` for their own
// `neoism_window::event_loop::EventLoopProxy<E>` wrapper. We can't impl
// it here because `neoism-ui` doesn't depend on `neoism-window` (the
// orphan rule also blocks it from outside).

/// Scheduler tracking all pending timers.
pub struct Scheduler<E, P: Proxy<E>> {
    timers: VecDeque<Timer<E>>,
    event_proxy: P,
}

impl<E: Clone, P: Proxy<E>> Scheduler<E, P> {
    pub fn new(event_proxy: P) -> Self {
        Self {
            timers: VecDeque::new(),
            event_proxy,
        }
    }

    /// Process all pending timers.
    ///
    /// If there are still timers pending after all ready events have
    /// been processed, the closest pending deadline will be returned.
    pub fn update(&mut self) -> Option<Instant> {
        let now = Instant::now();

        while !self.timers.is_empty() && self.timers[0].deadline <= now {
            if let Some(timer) = self.timers.pop_front() {
                // Automatically repeat the event.
                if let Some(interval) = timer.interval {
                    self.schedule(timer.event.clone(), interval, true, timer.id);
                }
                self.event_proxy.dispatch(timer.event);
            }
        }

        self.timers.front().map(|timer| timer.deadline)
    }

    /// Schedule a new event.
    pub fn schedule(&mut self, event: E, interval: Duration, repeat: bool, timer_id: TimerId) {
        let deadline = Instant::now() + interval;

        // Get insert position in the schedule.
        let index = self
            .timers
            .iter()
            .position(|timer| timer.deadline > deadline)
            .unwrap_or(self.timers.len());

        // Set the automatic event repeat rate.
        let interval = if repeat { Some(interval) } else { None };

        self.timers.insert(
            index,
            Timer {
                interval,
                deadline,
                event,
                id: timer_id,
            },
        );
    }

    /// Cancel a scheduled event.
    pub fn unschedule(&mut self, id: TimerId) -> Option<Timer<E>> {
        let index = self.timers.iter().position(|timer| timer.id == id)?;
        self.timers.remove(index)
    }

    /// Check if a timer is already scheduled.
    pub fn scheduled(&mut self, id: TimerId) -> bool {
        self.timers.iter().any(|timer| timer.id == id)
    }

    /// Remove all timers scheduled for a tab.
    ///
    /// This must be called when a tab is removed to ensure that timers
    /// on intervals do not stick around forever and cause a memory
    /// leak.
    pub fn unschedule_window(&mut self, id: usize) {
        self.timers.retain(|timer| timer.id.id != id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    struct CollectingProxy(Rc<RefCell<Vec<u32>>>);

    impl Proxy<u32> for CollectingProxy {
        fn dispatch(&self, event: u32) {
            self.0.borrow_mut().push(event);
        }
    }

    #[test]
    fn timer_fires_after_deadline() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let mut s = Scheduler::new(CollectingProxy(log.clone()));
        s.schedule(7u32, Duration::from_millis(0), false, TimerId::new(Topic::Render, 0));
        std::thread::sleep(Duration::from_millis(1));
        s.update();
        assert_eq!(log.borrow().as_slice(), &[7]);
    }

    #[test]
    fn unschedule_removes_timer() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let mut s = Scheduler::new(CollectingProxy(log.clone()));
        let id = TimerId::new(Topic::Render, 1);
        s.schedule(3u32, Duration::from_secs(60), false, id);
        assert!(s.scheduled(id));
        assert!(s.unschedule(id).is_some());
        assert!(!s.scheduled(id));
    }

    #[test]
    fn unschedule_window_drops_matching_ids() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let mut s = Scheduler::new(CollectingProxy(log));
        s.schedule(0u32, Duration::from_secs(60), false, TimerId::new(Topic::Render, 5));
        s.schedule(1u32, Duration::from_secs(60), false, TimerId::new(Topic::Render, 6));
        s.unschedule_window(5);
        assert!(s.scheduled(TimerId::new(Topic::Render, 6)));
        assert!(!s.scheduled(TimerId::new(Topic::Render, 5)));
    }
}
