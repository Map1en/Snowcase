use std::sync::Arc;

use futures::StreamExt;
use futures::channel::{mpsc, oneshot};
use gpui_kit::*;
use snowcase_core::StoreError;
use snowcase_core::library::{Library, Snapshot};

type Job = Box<dyn FnOnce(&mut Library) + Send>;

pub struct LibraryStore {
    snapshot: Option<Arc<Snapshot>>,
    error: Option<StoreError>,
    jobs: Option<mpsc::UnboundedSender<Job>>,
    pending: usize,
    sent: u64,
    shown: u64,
    _writer: Option<Task<()>>,
}

impl LibraryStore {
    pub fn new(library: Result<Library, StoreError>, cx: &mut Context<Self>) -> Self {
        match library {
            Ok(mut library) => {
                let snapshot = library.snapshot();
                let (jobs, mut queue) = mpsc::unbounded::<Job>();
                let writer = cx.background_spawn(async move {
                    while let Some(job) = queue.next().await {
                        job(&mut library);
                    }
                });
                Self {
                    snapshot: Some(snapshot),
                    error: None,
                    jobs: Some(jobs),
                    pending: 0,
                    sent: 0,
                    shown: 0,
                    _writer: Some(writer),
                }
            }
            Err(error) => Self {
                snapshot: None,
                error: Some(error),
                jobs: None,
                pending: 0,
                sent: 0,
                shown: 0,
                _writer: None,
            },
        }
    }

    pub fn snapshot(&self) -> Option<&Snapshot> {
        self.snapshot.as_deref()
    }

    pub fn error(&self) -> Option<&StoreError> {
        self.error.as_ref()
    }

    pub fn is_busy(&self) -> bool {
        self.pending > 0
    }

    pub fn change<T, F>(&mut self, cx: &mut Context<Self>, change: F) -> Task<Option<T>>
    where
        T: Send + 'static,
        F: FnOnce(&mut Library) -> Result<T, StoreError> + Send + 'static,
    {
        let Some(jobs) = &self.jobs else {
            return Task::ready(None);
        };
        let (done, outcome) = oneshot::channel();
        let job: Job = Box::new(move |library| {
            let result = change(library);
            done.send((result, library.snapshot())).ok();
        });
        if jobs.unbounded_send(job).is_err() {
            return Task::ready(None);
        }
        self.pending += 1;
        self.sent += 1;
        let sequence = self.sent;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let (result, snapshot) = outcome.await.ok()?;
            this.update(cx, |this, cx| {
                this.pending -= 1;
                let latest = sequence > this.shown;
                if latest {
                    this.shown = sequence;
                    this.snapshot = Some(snapshot);
                    this.error = None;
                }
                let value = result
                    .map_err(|error| {
                        if latest {
                            this.error = Some(error);
                        }
                    })
                    .ok();
                cx.notify();
                value
            })
            .ok()
            .flatten()
        })
    }
}

#[cfg(test)]
mod tests;
