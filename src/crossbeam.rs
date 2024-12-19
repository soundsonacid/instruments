use std::cell::RefCell;
use std::collections::BinaryHeap;
use std::sync::{Arc, Mutex};

use crossbeam_channel::{Receiver, RecvError, Sender, TryRecvError, TrySendError};

/// Synchronous MPMC Broadcast channel.
pub fn broadcast<T: Clone>() -> (Broadcaster<T>, Listener<T>) {
    let (tx, rx) = crossbeam_channel::unbounded();
    let listeners = Arc::new(Mutex::new(vec![tx]));

    (Broadcaster::new(Arc::clone(&listeners)), Listener::new(rx, listeners))
}

#[derive(Clone)]
pub struct Broadcaster<T> {
    listeners: Arc<Mutex<Vec<Sender<T>>>>,
}

impl<T: Clone> Broadcaster<T> {
    fn new(listeners: Arc<Mutex<Vec<Sender<T>>>>) -> Self {
        Self { listeners }
    }

    pub fn broadcast(&self, msg: T) -> Result<(), TrySendError<T>> {
        let mut listeners = self.listeners.lock().unwrap();
        listeners.retain(|tx| tx.try_send(msg.clone()).is_ok());
        Ok(())
    }
}

pub struct Listener<T> {
    rx: Receiver<T>,
    listeners: Arc<Mutex<Vec<Sender<T>>>>,
}

impl<T> Listener<T> {
    fn new(rx: Receiver<T>, listeners: Arc<Mutex<Vec<Sender<T>>>>) -> Self {
        Self { rx, listeners }
    }

    pub fn listen(&self) -> Result<T, RecvError> {
        match self.rx.try_recv() {
            Ok(value) => Ok(value),
            Err(_) => self.rx.recv(),
        }
    }
}

impl<T> Clone for Listener<T> {
    fn clone(&self) -> Self {
        let (tx, rx) = crossbeam_channel::unbounded();
        self.listeners.lock().unwrap().push(tx);
        Self { rx, listeners: Arc::clone(&self.listeners) }
    }
}

pub enum NotifierStatus {
    Notified,
    DidNotNotify,
}

/// Synchronous SPMC Watcher channel that only broadcasts when values change.
pub fn watcher<T: PartialEq + Clone>() -> (Notifier<T>, Watcher<T>) {
    let (broadcaster, listener) = broadcast();
    (Notifier::new(broadcaster), Watcher::new(listener))
}

pub struct Notifier<T: PartialEq + Clone> {
    broadcaster: Broadcaster<T>,
    last_seen_value: RefCell<Option<T>>,
}

impl<T: PartialEq + Clone> Notifier<T> {
    fn new(broadcaster: Broadcaster<T>) -> Self {
        Self { broadcaster, last_seen_value: RefCell::new(None) }
    }

    pub fn maybe_notify(&self, msg: T) -> Result<NotifierStatus, TrySendError<T>> {
        let should_notify = match self.last_seen_value.borrow().as_ref() {
            None => true,
            Some(last_value) => last_value != &msg,
        };

        if should_notify {
            self.last_seen_value.replace(Some(msg.clone()));
            self.broadcaster.broadcast(msg)?;
            Ok(NotifierStatus::Notified)
        } else {
            Ok(NotifierStatus::DidNotNotify)
        }
    }
}

#[derive(Clone)]
pub struct Watcher<T> {
    listener: Listener<T>,
}

impl<T> Watcher<T> {
    fn new(listener: Listener<T>) -> Self {
        Self { listener }
    }

    pub fn watch(&self) -> Result<T, RecvError> {
        self.listener.listen()
    }
}

pub struct PriorityReceiver<T: Ord> {
    inner: crossbeam_channel::Receiver<T>,
    heap: RefCell<BinaryHeap<T>>,
}

unsafe impl<T: Ord + Send + Sync> Sync for PriorityReceiver<T> {}

pub fn unbounded<T: Ord>() -> (Sender<T>, PriorityReceiver<T>) {
    let (tx, rx) = crossbeam_channel::unbounded();
    (tx, PriorityReceiver::new(rx))
}

pub fn bounded<T: Ord>(cap: usize) -> (Sender<T>, PriorityReceiver<T>) {
    let (tx, rx) = crossbeam_channel::bounded(cap);
    (tx, PriorityReceiver::new(rx))
}

impl<T: Ord> PriorityReceiver<T> {
    pub fn new(inner: Receiver<T>) -> Self {
        Self { inner, heap: RefCell::new(BinaryHeap::new()) }
    }

    pub fn recv(&self) -> Result<T, RecvError> {
        let mut heap = self.heap.borrow_mut();
        heap.extend(self.inner.try_iter());

        // If there are elements present, return the first one (the max)
        if !heap.is_empty() {
            return Ok(heap.pop().unwrap());
        }

        // Otherwise, just receive the first message off the channel
        self.inner.recv()
    }

    pub fn try_recv(&self) -> Result<T, TryRecvError> {
        let mut heap = self.heap.borrow_mut();
        heap.extend(self.inner.try_iter());

        // If there are elements present, return the first one (the max)
        if !heap.is_empty() {
            return Ok(heap.pop().unwrap());
        }

        // Otherwise, just receive the first message off the channel
        self.inner.try_recv()
    }
}

pub trait ReceiverExt<T>: Send + Sync {
    fn recv(&self) -> Result<T, RecvError>;
    fn try_recv(&self) -> Result<T, TryRecvError>;
}

impl<T: Ord + Send + Sync> ReceiverExt<T> for PriorityReceiver<T> {
    fn recv(&self) -> Result<T, RecvError> {
        PriorityReceiver::recv(self)
    }

    fn try_recv(&self) -> Result<T, TryRecvError> {
        PriorityReceiver::try_recv(self)
    }
}

impl<T: Send + Sync> ReceiverExt<T> for Receiver<T> {
    fn recv(&self) -> Result<T, RecvError> {
        self.recv()
    }

    fn try_recv(&self) -> Result<T, TryRecvError> {
        self.try_recv()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread;
    use std::time::Duration;

    use super::*;

    #[test]
    fn test_broadcast_basic() {
        let (broadcaster, listener1) = broadcast();
        let listener2 = listener1.clone();
        let listener3 = listener1.clone();

        let counter = Arc::new(AtomicUsize::new(0));

        let threads: Vec<_> = vec![listener1, listener2, listener3]
            .into_iter()
            .map(|listener| {
                let counter = Arc::clone(&counter);
                thread::spawn(move || {
                    if let Ok(value) = listener.listen() {
                        counter.fetch_add(value, Ordering::SeqCst);
                    }
                })
            })
            .collect();

        broadcaster.broadcast(5).unwrap();

        for handle in threads {
            handle.join().unwrap();
        }

        assert_eq!(counter.load(Ordering::SeqCst), 15);
    }

    #[test]
    fn test_watcher_deduplication() {
        let (notifier, watcher1) = watcher();
        let watcher2 = watcher1.clone();

        let counter = Arc::new(AtomicUsize::new(0));

        let counter1 = Arc::clone(&counter);
        let handle1 = thread::spawn(move || {
            if let Ok(value) = watcher1.watch() {
                counter1.fetch_add(value, Ordering::SeqCst);
            }
        });

        let counter2 = Arc::clone(&counter);
        let handle2 = thread::spawn(move || {
            if let Ok(value) = watcher2.watch() {
                counter2.fetch_add(value, Ordering::SeqCst);
            }
        });

        notifier.maybe_notify(5).unwrap();
        thread::sleep(Duration::from_millis(10));
        let status = notifier.maybe_notify(5).unwrap();
        assert!(matches!(status, NotifierStatus::DidNotNotify));

        handle1.join().unwrap();
        handle2.join().unwrap();

        assert_eq!(counter.load(Ordering::SeqCst), 10);
    }

    #[test]
    fn test_concurrent_processing() {
        let (notifier, watcher1) = watcher();
        let watcher2 = watcher1.clone();

        let processed = Arc::new(Mutex::new(Vec::new()));

        let threads: Vec<_> = vec![watcher1, watcher2]
            .into_iter()
            .enumerate()
            .map(|(id, watcher)| {
                let processed = Arc::clone(&processed);
                thread::spawn(move || {
                    if let Ok(value) = watcher.watch() {
                        thread::sleep(Duration::from_millis(if id == 0 { 200 } else { 100 }));
                        processed.lock().unwrap().push((id, value));
                    }
                })
            })
            .collect();

        notifier.maybe_notify(42).unwrap();

        for handle in threads {
            handle.join().unwrap();
        }

        let final_processed = processed.lock().unwrap();
        assert_eq!(final_processed.len(), 2);
        assert!(final_processed.iter().all(|&(_, value)| value == 42));
    }

    #[test]
    fn test_late_subscribers() {
        let (notifier, watcher1) = watcher();

        notifier.maybe_notify(1).unwrap();

        let watcher2 = watcher1.clone();

        notifier.maybe_notify(2).unwrap();
        notifier.maybe_notify(3).unwrap();

        let mut sum1 = 0;
        let mut sum2 = 0;

        for _ in 0..3 {
            if let Ok(value) = watcher1.watch() {
                sum1 += value;
            }
        }

        for _ in 0..2 {
            if let Ok(value) = watcher2.watch() {
                sum2 += value;
            }
        }

        assert_eq!(sum1, 6);
        assert_eq!(sum2, 5);
    }

    #[test]
    fn test_priority_channel_basic() {
        let (tx, rx) = unbounded();

        tx.send(3).unwrap();
        tx.send(1).unwrap();
        tx.send(4).unwrap();
        tx.send(2).unwrap();

        assert_eq!(rx.recv().unwrap(), 4);
        assert_eq!(rx.recv().unwrap(), 3);
        assert_eq!(rx.recv().unwrap(), 2);
        assert_eq!(rx.recv().unwrap(), 1);
    }

    #[test]
    fn test_priority_channel_bounded() {
        let (tx, rx) = bounded(2);

        tx.send(2).unwrap();
        tx.send(1).unwrap();

        assert!(tx.try_send(3).is_err());

        assert_eq!(rx.recv().unwrap(), 2);
        tx.send(3).unwrap();

        assert_eq!(rx.recv().unwrap(), 3);
        assert_eq!(rx.recv().unwrap(), 1);
    }

    #[test]
    fn test_priority_channel_concurrent() {
        let (tx, rx) = unbounded();
        let tx_clone = tx.clone();

        let handle1 = thread::spawn(move || {
            tx.send(3).unwrap();
            tx.send(1).unwrap();
            tx.send(5).unwrap();
        });

        let handle2 = thread::spawn(move || {
            tx_clone.send(2).unwrap();
            tx_clone.send(4).unwrap();
            tx_clone.send(6).unwrap();
        });

        handle1.join().unwrap();
        handle2.join().unwrap();

        let mut received = Vec::new();
        for _ in 0..6 {
            received.push(rx.recv().unwrap());
        }

        assert_eq!(received, vec![6, 5, 4, 3, 2, 1]);
    }

    #[test]
    fn test_priority_channel_try_recv() {
        let (tx, rx) = unbounded();

        assert!(rx.try_recv().is_err());

        tx.send(2).unwrap();
        tx.send(1).unwrap();
        tx.send(3).unwrap();

        assert_eq!(rx.try_recv().unwrap(), 3);
        assert_eq!(rx.try_recv().unwrap(), 2);
        assert_eq!(rx.try_recv().unwrap(), 1);

        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn test_priority_channel_ordering_stability() {
        let (tx, rx) = unbounded();

        tx.send(2).unwrap();
        tx.send(2).unwrap();
        tx.send(1).unwrap();
        tx.send(2).unwrap();

        assert_eq!(rx.recv().unwrap(), 2);
        assert_eq!(rx.recv().unwrap(), 2);
        assert_eq!(rx.recv().unwrap(), 2);
        assert_eq!(rx.recv().unwrap(), 1);
    }
}
