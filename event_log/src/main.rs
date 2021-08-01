use event_log::*;

#[tokio::main]
async fn main() -> Result<()> {
    let e = EventLog::<serde_json::Value>::new("data/event_log.sled")?;

    let mut s = e.subscribe("log viewer")?;

    loop {
        let batch = s.next_batch().await?;
        for e in batch {
            println!("{:?}", e);
        }
    }
}
