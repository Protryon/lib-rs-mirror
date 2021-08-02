use event_log::*;

#[tokio::main]
async fn main() -> Result<()> {
    let e = EventLog::<rmpv::Value>::new("../data/event_log.db")?;

    let mut s = e.subscribe("log viewer")?;

    loop {
        let batch = s.next_batch().await?;
        for e in batch {
            println!("{:?}", e);
        }
    }
}
