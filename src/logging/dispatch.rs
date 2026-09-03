use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use super::{LogEvent, LogLevel, LogSink};

/// Result of placing an event into the bounded dispatch queue.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DispatchOutcome {
    Queued,
    Dropped,
}

/// Failure while submitting or stopping asynchronous dispatch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DispatchError {
    QueueFull,
    Closed,
    InvalidCapacity,
    WorkerPanicked,
}

enum DispatchMessage {
    Event(Box<LogEvent>),
    Stop,
}

pub(crate) struct LogDispatcher {
    sender: mpsc::SyncSender<DispatchMessage>,
    subscribers: Arc<Mutex<Vec<Arc<dyn LogSink>>>>,
    lifecycle: Arc<Mutex<Lifecycle>>,
    lifecycle_changed: Arc<Condvar>,
    worker_id: thread::ThreadId,
    worker: Mutex<Option<JoinHandle<()>>>,
}

struct Lifecycle {
    shutting_down: bool,
    active_publishers: usize,
    stopped: bool,
}

impl LogDispatcher {
    pub(crate) fn new(capacity: usize) -> Result<Arc<Self>, DispatchError> {
        if capacity == 0 {
            return Err(DispatchError::InvalidCapacity);
        }

        let (sender, receiver) = mpsc::sync_channel(capacity);
        let subscribers = Arc::new(Mutex::new(Vec::<Arc<dyn LogSink>>::new()));
        let worker_subscribers = Arc::clone(&subscribers);
        let worker_lifecycle = Arc::new(Mutex::new(Lifecycle {
            shutting_down: false,
            active_publishers: 0,
            stopped: false,
        }));
        let lifecycle_changed = Arc::new(Condvar::new());
        let worker_lifecycle_changed = Arc::clone(&lifecycle_changed);
        let worker_lifecycle_state = Arc::clone(&worker_lifecycle);
        let worker = thread::spawn(move || {
            let dispatch_event = |event: Box<LogEvent>| {
                let subscribers = worker_subscribers
                    .lock()
                    .map(|subscribers| subscribers.clone())
                    .unwrap_or_default();
                for subscriber in subscribers {
                    let result = catch_unwind(AssertUnwindSafe(|| {
                        subscriber.publish(&event);
                    }));
                    if result.is_err()
                        && let Ok(mut registered) = worker_subscribers.lock()
                    {
                        registered.retain(|candidate| !Arc::ptr_eq(candidate, &subscriber));
                    }
                }
            };

            while let Ok(message) = receiver.recv() {
                match message {
                    DispatchMessage::Event(event) => dispatch_event(event),
                    DispatchMessage::Stop => break,
                }
                let shutting_down = worker_lifecycle_state
                    .lock()
                    .map(|lifecycle| lifecycle.shutting_down)
                    .unwrap_or(true);
                if shutting_down {
                    loop {
                        let active_publishers = worker_lifecycle_state
                            .lock()
                            .map(|lifecycle| lifecycle.active_publishers)
                            .unwrap_or(0);
                        if active_publishers == 0 {
                            while let Ok(DispatchMessage::Event(event)) = receiver.try_recv() {
                                dispatch_event(event);
                            }
                            break;
                        }

                        match receiver.recv_timeout(Duration::from_millis(10)) {
                            Ok(DispatchMessage::Event(event)) => dispatch_event(event),
                            Ok(DispatchMessage::Stop) => break,
                            Err(mpsc::RecvTimeoutError::Timeout) => continue,
                            Err(mpsc::RecvTimeoutError::Disconnected) => break,
                        }
                    }
                    break;
                }
            }

            if let Ok(mut lifecycle) = worker_lifecycle_state.lock() {
                lifecycle.stopped = true;
                worker_lifecycle_changed.notify_all();
            }
        });

        Ok(Arc::new(Self {
            sender,
            subscribers,
            lifecycle: worker_lifecycle,
            lifecycle_changed,
            worker_id: worker.thread().id(),
            worker: Mutex::new(Some(worker)),
        }))
    }

    pub(crate) fn subscribe(&self, sink: Arc<dyn LogSink>) {
        if let Ok(mut subscribers) = self.subscribers.lock() {
            subscribers.push(sink);
        }
    }

    pub(crate) fn submit(&self, event: LogEvent) -> Result<DispatchOutcome, DispatchError> {
        let mut lifecycle = self
            .lifecycle
            .lock()
            .map_err(|_| DispatchError::WorkerPanicked)?;
        if lifecycle.shutting_down {
            return Err(DispatchError::Closed);
        }
        lifecycle.active_publishers += 1;
        drop(lifecycle);

        let result = match self
            .sender
            .try_send(DispatchMessage::Event(Box::new(event)))
        {
            Ok(()) => Ok(DispatchOutcome::Queued),
            Err(mpsc::TrySendError::Full(DispatchMessage::Event(event))) => {
                if event.level <= LogLevel::Info {
                    Ok(DispatchOutcome::Dropped)
                } else {
                    self.sender
                        .send(DispatchMessage::Event(event))
                        .map(|()| DispatchOutcome::Queued)
                        .map_err(|_| DispatchError::Closed)
                }
            }
            Err(mpsc::TrySendError::Disconnected(_)) => Err(DispatchError::Closed),
            Err(mpsc::TrySendError::Full(DispatchMessage::Stop)) => Err(DispatchError::QueueFull),
        };

        let mut lifecycle = self
            .lifecycle
            .lock()
            .map_err(|_| DispatchError::WorkerPanicked)?;
        lifecycle.active_publishers -= 1;
        self.lifecycle_changed.notify_all();
        result
    }

    pub(crate) fn shutdown(&self) -> Result<(), DispatchError> {
        let mut lifecycle = self
            .lifecycle
            .lock()
            .map_err(|_| DispatchError::WorkerPanicked)?;
        if lifecycle.shutting_down {
            if thread::current().id() != self.worker_id {
                while !lifecycle.stopped {
                    lifecycle = self
                        .lifecycle_changed
                        .wait(lifecycle)
                        .map_err(|_| DispatchError::WorkerPanicked)?;
                }
            }
            return Ok(());
        }
        lifecycle.shutting_down = true;
        let called_by_worker = thread::current().id() == self.worker_id;
        if called_by_worker {
            self.lifecycle_changed.notify_all();
            return Ok(());
        }
        while lifecycle.active_publishers != 0 {
            lifecycle = self
                .lifecycle_changed
                .wait(lifecycle)
                .map_err(|_| DispatchError::WorkerPanicked)?;
        }
        let _ = self.sender.send(DispatchMessage::Stop);
        drop(lifecycle);
        let worker = self
            .worker
            .lock()
            .map_err(|_| DispatchError::WorkerPanicked)?
            .take();
        if let Some(worker) = worker {
            worker.join().map_err(|_| DispatchError::WorkerPanicked)?;
        }
        Ok(())
    }
}
