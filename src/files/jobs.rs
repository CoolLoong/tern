use std::{
    io,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

#[derive(Clone, Default)]
pub(crate) struct Cancellation(Arc<AtomicBool>);

impl Cancellation {
    pub(crate) fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    pub(crate) fn check(&self) -> io::Result<()> {
        if self.0.load(Ordering::Relaxed) {
            Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "listing cancelled",
            ))
        } else {
            Ok(())
        }
    }
}

/// The owner cancels on replacement and on drop (including closing a tab).
/// Identity checks also reject a completed result that was superseded before
/// its foreground callback ran.
#[derive(Default)]
pub(crate) struct JobSlot(Option<Cancellation>);

impl JobSlot {
    pub(crate) fn start(&mut self) -> Cancellation {
        self.cancel();
        let token = Cancellation::default();
        self.0 = Some(token.clone());
        token
    }

    pub(crate) fn cancel(&mut self) {
        if let Some(token) = self.0.take() {
            token.cancel();
        }
    }

    pub(crate) fn finish(&mut self, token: &Cancellation) -> bool {
        if self
            .0
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(&current.0, &token.0))
        {
            self.0 = None;
            token.check().is_ok()
        } else {
            false
        }
    }
}

impl Drop for JobSlot {
    fn drop(&mut self) {
        self.cancel();
    }
}
