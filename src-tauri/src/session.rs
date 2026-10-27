use parking_lot::Mutex;
use std::sync::Arc;

#[derive(Default)]
struct SessionInner {
    acknowledged: bool,
    face_verified: bool,
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

    pub fn is_face_verified(&self) -> bool {
        self.inner.lock().face_verified
    }

    /// Ready to show chrome + biz interview shell.
    pub fn is_interview_ready(&self) -> bool {
        let g = self.inner.lock();
        g.acknowledged && g.face_verified
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
    pub fn mark_face_verified(&self) -> bool {
        let mut g = self.inner.lock();
        if !g.acknowledged || g.face_verified {
            return false;
        }
        g.face_verified = true;
        true
    }

    /// First call returns true; later calls return false.
    pub fn try_start_interview(&self) -> bool {
        let mut g = self.inner.lock();
        if g.interview_started {
            return false;
        }
        if !(g.acknowledged && g.face_verified) {
            return false;
        }
        g.interview_started = true;
        true
    }
}
