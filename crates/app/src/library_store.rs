use std::sync::Arc;

use futures::StreamExt;
use futures::channel::{mpsc, oneshot};
use gpui_kit::*;
use snowcase_core::StoreError;
use snowcase_core::library::{Library, Outcome, run};
use snowcase_core::snapshot::Snapshot;

type Job = Box<dyn FnOnce(&mut Library) + Send>;

const BATCH: usize = 32;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Progress {
    pub done: usize,
    pub total: usize,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub failures: Vec<(String, String)>,
    pub paused: bool,
    pub running: bool,
}

#[derive(Default)]
struct Background {
    progress: Progress,
    cancelled: bool,
    again: bool,
    wake: Option<oneshot::Sender<()>>,
    task: Option<Task<()>>,
}

enum Step {
    Go,
    Wait(oneshot::Receiver<()>),
    Stop,
}

pub struct LibraryStore {
    snapshot: Option<Arc<Snapshot>>,
    error: Option<StoreError>,
    jobs: Option<mpsc::UnboundedSender<Job>>,
    pending: usize,
    sent: u64,
    shown: u64,
    background: Background,
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
                    background: Background::default(),
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
                background: Background::default(),
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

    pub fn progress(&self) -> &Progress {
        &self.background.progress
    }

    pub fn start_background(&mut self, cx: &mut Context<Self>) {
        if self.background.progress.running {
            self.background.again = true;
            return;
        }
        let listed = self.change(cx, |library| Ok(library.pending_work()));
        self.background = Background {
            progress: Progress {
                running: true,
                ..Progress::default()
            },
            ..Background::default()
        };
        cx.notify();
        let task = cx.spawn(async move |this, cx| {
            let work = listed.await.unwrap_or_default();
            this.update(cx, |this, cx| {
                let progress = &mut this.background.progress;
                progress.total = work.len();
                progress.bytes_total = work.iter().map(|item| item.bytes()).sum();
                cx.notify();
            })
            .ok();
            let mut batch: Vec<Outcome> = Vec::new();
            for item in work {
                loop {
                    let step = this
                        .update(cx, |this, _| {
                            let background = &mut this.background;
                            if background.cancelled {
                                Step::Stop
                            } else if background.progress.paused {
                                let (wake, waiting) = oneshot::channel();
                                background.wake = Some(wake);
                                Step::Wait(waiting)
                            } else {
                                Step::Go
                            }
                        })
                        .unwrap_or(Step::Stop);
                    match step {
                        Step::Go => break,
                        Step::Wait(waiting) => {
                            flush(&this, cx, &mut batch).await;
                            waiting.await.ok();
                        }
                        Step::Stop => {
                            flush(&this, cx, &mut batch).await;
                            finish(&this, cx);
                            return;
                        }
                    }
                }
                let bytes = item.bytes();
                let outcome = cx.background_spawn(async move { run(item) }).await;
                this.update(cx, |this, cx| {
                    let progress = &mut this.background.progress;
                    progress.done += 1;
                    progress.bytes_done += bytes;
                    if let Err(error) = &outcome.result {
                        progress.failures.push((outcome.work.path.to_string_lossy().into_owned(), error.clone()));
                    }
                    cx.notify();
                })
                .ok();
                batch.push(outcome);
                if batch.len() >= BATCH {
                    flush(&this, cx, &mut batch).await;
                }
            }
            flush(&this, cx, &mut batch).await;
            finish(&this, cx);
        });
        self.background.task = Some(task);
    }

    pub fn pause_background(&mut self, cx: &mut Context<Self>) {
        if self.background.progress.running {
            self.background.progress.paused = true;
            cx.notify();
        }
    }

    pub fn resume_background(&mut self, cx: &mut Context<Self>) {
        self.background.progress.paused = false;
        if let Some(wake) = self.background.wake.take() {
            wake.send(()).ok();
        }
        cx.notify();
    }

    pub fn cancel_background(&mut self, cx: &mut Context<Self>) {
        self.background.cancelled = true;
        self.resume_background(cx);
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

async fn flush(this: &WeakEntity<LibraryStore>, cx: &mut AsyncApp, batch: &mut Vec<Outcome>) {
    if batch.is_empty() {
        return;
    }
    let outcomes = std::mem::take(batch);
    let write = this.update(cx, |this, cx| this.change(cx, move |library| library.apply_work(&outcomes)));
    if let Ok(write) = write {
        write.await;
    }
}

fn finish(this: &WeakEntity<LibraryStore>, cx: &mut AsyncApp) {
    this.update(cx, |this, cx| {
        let progress = &mut this.background.progress;
        progress.running = false;
        progress.paused = false;
        let again = this.background.again && !this.background.cancelled;
        cx.notify();
        if again {
            this.start_background(cx);
        }
    })
    .ok();
}

#[cfg(test)]
mod tests;
