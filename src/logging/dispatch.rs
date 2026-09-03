use std::sync::{Arc, Mutex, mpsc};
use std::thread::{self, JoinHandle};

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
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl LogDispatcher {
    pub(crate) fn new(capacity: usize) -> Result<Arc<Self>, DispatchError> {
        if capacity == 0 {
            return Err(DispatchError::InvalidCapacity);
        }

        let (sender, receiver) = mpsc::sync_channel(capacity);
        let subscribers = Arc::new(Mutex::new(Vec::<Arc<dyn LogSink>>::new()));
        let worker_subscribers = Arc::clone(&subscribers);
        let worker = thread::spawn(move || {
            while let Ok(message) = receiver.recv() {
                match message {
                    DispatchMessage::Event(event) => {
                        if let Ok(subscribers) = worker_subscribers.lock() {
                            for subscriber in subscribers.iter() {
                                subscriber.publish(&event);
                            }
                        }
                    }
                    DispatchMessage::Stop => break,
                }
            }
        });

        Ok(Arc::new(Self {
            sender,
            subscribers,
            worker: Mutex::new(Some(worker)),
        }))
    }

    pub(crate) fn subscribe(&self, sink: Arc<dyn LogSink>) {
        if let Ok(mut subscribers) = self.subscribers.lock() {
            subscribers.push(sink);
        }
    }

    pub(crate) fn submit(&self, event: LogEvent) -> Result<DispatchOutcome, DispatchError> {
        match self
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
        }
    }

    pub(crate) fn shutdown(&self) -> Result<(), DispatchError> {
        let _ = self.sender.send(DispatchMessage::Stop);
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
