use futures::sink::SinkExt;
use futures::channel::mpsc;
use futures::Stream;
use std::io;
use actix_web::web::Bytes;

pub struct Writer<T: 'static, E: 'static> {
    sender: mpsc::Sender<Result<T, E>>,
    rt: tokio::runtime::Handle,
}

impl<T, E: std::fmt::Display> Writer<T, E> {
    pub fn fail(&mut self, error: E) {
        eprintln!("async write aborted: {}", error);
        let _ = self.sender.send(Err(error));
    }
}

impl<E: 'static> io::Write for Writer<Bytes, E>
where
    E: Send + Sync + 'static,
{
    fn write(&mut self, d: &[u8]) -> io::Result<usize> {
        let len = d.len();
        let sent = self.sender.send(Ok(Bytes::copy_from_slice(d)));
        self.rt.enter(|| futures::executor::block_on(sent))
            .map_err(|e| {
                eprintln!("write failed: {}", e);
                io::Error::new(io::ErrorKind::BrokenPipe, e)
            })?;
        Ok(len)
    }

    fn write_all(&mut self, d: &[u8]) -> io::Result<()> {
        self.write(d).map(|_| ())
    }

    fn flush(&mut self) -> io::Result<()> {
        let flushed = self.sender.flush();
        self.rt.enter(|| futures::executor::block_on(flushed))
            .map_err(|e| {
                eprintln!("flush failed: {}", e);
                io::Error::new(io::ErrorKind::BrokenPipe, e)
            })?;
        Ok(())
    }
}

pub async fn writer<T: 'static, E: 'static + std::fmt::Debug>() -> (Writer<T, E>, impl Stream<Item = Result<T, E>>) {
    let rt = tokio::runtime::Handle::current();
    let (tx, rx) = mpsc::channel(3);
    let w = Writer {
        sender: tx,
        rt,
    };
    (w, rx)
}
