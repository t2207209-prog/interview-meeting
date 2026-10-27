use parking_lot::Mutex;
use std::sync::Arc;

#[derive(Default)]
struct SessionInner {
    acknowledged: bool,
}

#[derive(Clone, Default)]
pub struct SessionState {
    inner: Arc<Mutex<SessionInner>>,
}

impl SessionState {
    pub fn mark_acknowledged(&self) -> bool {
        let mut g = self.inner.lock();
        if g.acknowledged {
            return false;
        }
        g.acknowledged = true;
        true
    }

    pub fn mark_interview_started(&self) {
        // Reserved for future session phases; acknowledge already gates entry.
        let _ = self;
    }
}
