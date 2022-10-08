use std::cell::Ref;
use std::cell::RefCell;
use rusqlite::ToSql;
use std::path::PathBuf;
use std::sync::Arc;
use rusqlite::Connection;
use std::marker::PhantomData;
use std::path::Path;
use std::time::Duration;
use log::error;
use log::debug;
use serde::de::DeserializeOwned;
use serde::Serialize;
use thread_local::ThreadLocal;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("event db error")]
    Db(#[from] #[source] rusqlite::Error),
    #[error("event db connection error")]
    Connection,
    #[error("serialize error")]
    Ser(#[from] #[source] rmp_serde::encode::Error),
    #[error("deserialize error")]
    De(#[from] #[source] rmp_serde::decode::Error),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, Clone)]
pub struct EventLog<T> {
    db: Arc<ThreadLocal<RefCell<Connection>>>,
    path: PathBuf,
    _event_t: PhantomData<fn(T)>,
}

#[derive(Debug)]
pub struct Subscription<T> {
    name: String,
    log: EventLog<T>,
    _event_t: PhantomData<fn(T)>,
}

// clone is a bad derive bound
impl<T: DeserializeOwned + Serialize + Clone> EventLog<T> {
    /// Store events at this location
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        Ok(Self {
            path: path.as_ref().to_path_buf(),
            db: Arc::new(ThreadLocal::new()),
            _event_t: PhantomData,
        })
    }

    /// Create or continue event observation
    pub fn subscribe(&self, name: impl Into<String>) -> Result<Subscription<T>> {
        let name = name.into();
        let db = self.db()?;
        let mut q = db.prepare_cached("INSERT OR IGNORE INTO subscribers(name, last_event_id) VALUES(?1, ?2)")?;
        let args: &[&dyn ToSql] = &[&name, &0i32];
        q.execute(args)?;
        Ok(Subscription {
            name,
            log: (*self).clone(),
            _event_t: PhantomData,
        })
    }

    fn db(&self) -> Result<Ref<Connection>> {
        self.db.get_or_try(|| {
            let db = Connection::open(&self.path)?;
            db.busy_timeout(Duration::from_secs(60))?;
            db.execute_batch("
                CREATE TABLE IF NOT EXISTS events(id INTEGER PRIMARY KEY AUTOINCREMENT, data BLOB NOT NULL);
                CREATE TABLE IF NOT EXISTS subscribers(name TEXT PRIMARY KEY, last_event_id INTEGER NOT NULL);
            ")?;
            Ok(RefCell::new(db))
        }).map(|r| r.borrow())
    }

    /// Fire an event
    pub fn post(&self, event: &T) -> Result<()> {
        let event_bytes = rmp_serde::to_vec_named(event)?;
        let db = self.db()?;
        let mut q = db.prepare_cached("INSERT INTO events(data) VALUES(?1)")?;
        q.execute(&[&event_bytes])?;
        Ok(())
    }
}

#[derive(Debug)]
pub struct EventBatch<'sub, T: DeserializeOwned + Serialize + Clone> {
    ack: Option<u64>,
    sub: &'sub Subscription<T>,
    events: Vec<(u64, Vec<u8>)>,
    _event_t: PhantomData<fn(T)>,
}

impl<T: DeserializeOwned + Serialize + Clone> EventBatch<'_, T> {
    pub fn is_empty(&mut self) -> bool {
        self.events.is_empty()
    }
}

impl<T: DeserializeOwned + Serialize + Clone> Iterator for EventBatch<'_, T> {
    type Item = Result<T>;
    fn next(&mut self) -> Option<Self::Item> {
        let (k, v) = self.events.pop()?;
        self.ack = Some(k);
        Some(rmp_serde::from_slice(&v).map_err(From::from))
    }
}

impl<T: DeserializeOwned + Serialize + Clone> Drop for EventBatch<'_, T> {
    fn drop(&mut self) {
        if std::thread::panicking() {
            return;
        }

        if let Some(ack) = self.ack {
            let _ = self.sub.mark_ack(ack)
                .map_err(|e| error!("drop-ack: {}", e));
        }
    }
}

impl<T: DeserializeOwned + Serialize + Clone> Subscription<T> {
    fn mark_ack(&self, id: u64) -> Result<()> {
        let db = self.log.db()?;
        let mut q = db.prepare_cached("INSERT OR REPLACE INTO subscribers(name, last_event_id) VALUES(?1, ?2)")?;
        let args: &[&dyn ToSql] = &[&self.name, &id];
        q.execute(args)?;
        debug!("ACKd events of {} up to {}", self.name, id);
        Ok(())
    }

    fn fetch_batch(&self) -> Result<EventBatch<'_, T>> {
        // TODO: some kind of lock against concurrent access, so that last_event_id isn't messed up
        let db = self.log.db()?;
        let mut q = db.prepare_cached("SELECT e.id, e.data FROM events e WHERE e.id > (SELECT last_event_id FROM subscribers WHERE name = ?1) ORDER BY e.id LIMIT 10")?;
        let mut events = q.query_map(&[&self.name], |row| Ok((row.get(0)?, row.get(1)?)))?.collect::<Result<Vec<_>, _>>()?;
        events.reverse(); // batch iterator pops them!
        Ok(EventBatch {
            events,
            sub: self,
            ack: None,
            _event_t: PhantomData,
        })
    }

    pub async fn next_batch(&mut self) -> Result<EventBatch<'_, T>> {
        let mut wait = 2;
        loop {
            let mut batch = self.fetch_batch()?;
            if !batch.is_empty() {
                debug!("found event batch for {} with events {}-{}", self.name, batch.events[0].0, batch.events.last().unwrap().0);
                return Ok(batch);
            }
            tokio::time::sleep(Duration::from_secs(wait)).await;
            if wait < 10 { wait += 1; }
        }
    }
}
