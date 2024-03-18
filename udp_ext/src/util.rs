use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

pub struct DropTracker {
    alive: Arc<AtomicBool>,
}

impl DropTracker {
    pub fn new() -> Self {
        DropTracker {
            alive: Arc::new(AtomicBool::new(true)),
        }
    }

    pub fn handle(&self) -> DropTrackerHandle {
        DropTrackerHandle {
            drop_tracker_alive: self.alive.clone(),
        }
    }
}

impl Drop for DropTracker {
    fn drop(&mut self) {
        self.alive.store(false, Ordering::Relaxed);
    }
}

pub struct DropTrackerHandle {
    drop_tracker_alive: Arc<AtomicBool>,
}

impl DropTrackerHandle {
    pub fn alive(&self) -> bool {
        self.drop_tracker_alive.load(Ordering::Relaxed)
    }
}
