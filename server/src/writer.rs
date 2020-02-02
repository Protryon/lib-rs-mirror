use futures::sink::SinkExt;
use futures::channel::mpsc;
use futures::Stream;
use std::io;
use actix_web::web::Bytes;

pub struct Writer<T: 'static, E: 'static> {
    sender: mpsc::Sender<Result<T, E>>,
    rt: tokio::runtime::Runtime,
}

impl<T, E> Writer<T, E> {
    pub fn fail(&mut self, error: E) {
        let _ = self.sender.send(Err(error));
    }
}

impl<E: 'static> io::Write for Writer<Bytes, E>
where
    E: Send + Sync + 'static,
{
    fn write(&mut self, d: &[u8]) -> io::Result<usize> {
        let len = d.len();
        let data = Bytes::copy_from_slice(d);
        self.rt.block_on(self.sender.send(Ok(data)))
            .map_err(|e| io::Error::new(io::ErrorKind::BrokenPipe, e))?;
        Ok(len)
    }

    fn write_all(&mut self, d: &[u8]) -> io::Result<()> {
        self.write(d).map(|_| ())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.rt.block_on(self.sender
            .flush())
            .map_err(|e| io::Error::new(io::ErrorKind::BrokenPipe, e))?;
        Ok(())
    }
}

pub fn writer<T, E: std::fmt::Debug>() -> (Writer<T, E>, impl Stream<Item = Result<T, E>>) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let (tx, rx) = mpsc::channel(3);
    let w = Writer {
        sender: tx,
        rt,
    };
    (w, rx)
}
