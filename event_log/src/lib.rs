use sled::IVec;
use std::marker::PhantomData;
use std::{convert::TryInto, iter::Peekable, path::Path};
use log::error;
use log::debug;
use serde::de::DeserializeOwned;
use serde::Serialize;

trait IVecConv {
    fn as_u64be(&self) -> Option<u64>;
}

impl IVecConv for sled::IVec {
    fn as_u64be(&self) -> Option<u64> {
        Some(u64::from_be_bytes(self.get(..8)?.try_into().unwrap()))
    }
}

impl IVecConv for Option<sled::IVec> {
    fn as_u64be(&self) -> Option<u64> {
        self.as_ref()?.as_u64be()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("event db error")]
    Db(#[from] #[source] sled::Error),
    #[error("serialize error")]
    Ser(#[from] #[source] rmp_serde::encode::Error),
    #[error("deserialize error")]
    De(#[from] #[source] rmp_serde::decode::Error),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

pub struct EventLog<T> {
    db: sled::Db,
    subscribers: sled::Tree,
    events: sled::Tree,
    _event_t: PhantomData<fn(T)>,
}

pub struct Subscription<T> {
    name: String,
    events: sled::Tree,
    subscribers: sled::Tree,
    _event_t: PhantomData<fn(T)>,
}

impl<T: DeserializeOwned + Serialize> EventLog<T> {
    /// Store events at this location
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let db = sled::Config::default().path(path).open()?;
        let subscribers = db.open_tree("subscribers")?;
        let events = db.open_tree("events")?;
        Ok(Self {
            db,
            subscribers,
            events,
            _event_t: PhantomData,
        })
    }

    /// Create or continue event observation
    pub fn subscribe(&self, name: impl Into<String>) -> Result<Subscription<T>> {
        Ok(Subscription {
            name: name.into(),
            subscribers: self.subscribers.clone(),
            events: self.events.clone(),
            _event_t: PhantomData,
        })
    }

    /// Fire an event
    pub fn post(&self, event: &T) -> Result<()> {
        let id = self.db.generate_id()?;

        let event_bytes = rmp_serde::to_vec_named(event)?;
        self.events.insert(&id.to_be_bytes(), event_bytes)?;
        Ok(())

    }
}

pub struct EventBatch<'sub, T> {
    ack: Option<IVec>,
    in_progress: Option<IVec>,
    iter: Peekable<sled::Iter>,
    sub: &'sub Subscription<T>,
    _event_t: PhantomData<fn(T)>,
}

impl<T> EventBatch<'_, T> {
    pub fn done(mut self) {
        if let Some(in_progress) = self.in_progress.take() {
            self.ack = Some(in_progress);
        }
    }

    pub fn is_empty(&mut self) -> bool {
        self.iter.peek().is_none()
    }
}

impl<T: serde::de::DeserializeOwned> Iterator for EventBatch<'_, T> {
    type Item = Result<T>;
    fn next(&mut self) -> Option<Self::Item> {
        let (k, v) = self.iter.next()?.map_err(|e| error!("EventBatch: {}", e)).ok()?;
        self.ack = self.in_progress.take();
        self.in_progress = Some(k);
        Some(rmp_serde::from_slice(&v).map_err(From::from))
    }
}

impl<T> Drop for EventBatch<'_, T> {
    fn drop(&mut self) {
        if std::thread::panicking() {
            return;
        }

        if let Some(ack) = &self.ack {
            let _ = self.sub.subscribers.insert(&self.sub.name, ack)
                .map_err(|e| error!("drop-ack: {}", e));
        }
    }
}

impl<T> Subscription<T> {
    fn fetch_batch(&self) -> Result<EventBatch<'_, T>> {
        // TODO: some kind of lock against concurrent access, so that last_event_id isn't messed up
        let last_event_id = self.subscribers.get(&self.name)?;
        let last_event_id = last_event_id.as_deref().unwrap_or(b"");
        let iter = self.events.range(last_event_id..).peekable();
        Ok(EventBatch {
            iter,
            sub: self,
            ack: None,
            in_progress: None,
            _event_t: PhantomData,
        })
    }

    pub async fn next_batch<'a>(&'a mut self) -> Result<EventBatch<'a, T>> {
        let mut watch = self.events.watch_prefix(b"");
        loop {
            {
                let mut pending = self.fetch_batch()?;
                if !pending.is_empty() {
                    debug!("found event batch for {}", self.name);
                    return Ok(unsafe {
                        // borrow-checker limitation
                        std::mem::transmute::<EventBatch<T>, EventBatch<'a, T>>(pending)
                    });
                }
            }
            debug!("waiting for any events for {}", self.name);
            let _ = (&mut watch).await;
        }
    }
}
