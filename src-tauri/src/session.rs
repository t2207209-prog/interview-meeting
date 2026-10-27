use parking_lot::Mutex;
use std::sync::Arc;

#[derive(Default)]
struct SessionInner {
    acknowledged: bool,
    interview_started: bool,
}

#[derive(Clone, Default)]
pub struct SessionState {
    inner: Arc<Mutex<SessionInner>>,
}

impl SessionState {
    pub fn is_acknowledged(&self) -> bool {
        self.inner.lock().acknowledged
    }

    /// First call returns true; later calls return false.
    pub fn mark_acknowledged(&self) -> bool {
        let mut g = self.inner.lock();
        if g.acknowledged {
            return false;
        }
        g.acknowledged = true;
        true
    }

    /// First call returns true; later calls return false.
    pub fn try_start_interview(&self) -> bool {
        let mut g = self.inner.lock();
        if g.interview_started {
            return false;
        }
        g.interview_started = true;
        true
    }
}
