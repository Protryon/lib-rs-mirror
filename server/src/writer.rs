use futures::Stream;
use futures::sink::{Sink, Wait};
use futures::sync::mpsc;
use std::io;

pub struct Writer<T, E>(Wait<mpsc::Sender<Result<T, E>>>);

impl<T, E> Writer<T, E> {
    pub fn fail(&mut self, error: E) {
        let _ = self.0.send(Err(error));
    }
}

impl<T, E> io::Write for Writer<T, E>
where
    T: for<'a> From<&'a [u8]> + Send + Sync + 'static,
    E: Send + Sync + 'static,
{
    fn write(&mut self, d: &[u8]) -> io::Result<usize> {
        let len = d.len();
        self.0
            .send(Ok(d.into()))
            .map(|()| len)
            .map_err(|e| io::Error::new(io::ErrorKind::BrokenPipe, e))
    }

    fn write_all(&mut self, d: &[u8]) -> io::Result<()> {
        self.write(d).map(|_| ())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0
            .flush()
            .map_err(|e| io::Error::new(io::ErrorKind::BrokenPipe, e))
    }
}

pub fn writer<T, E>() -> (Writer<T, E>, impl Stream<Item = T, Error = E>) {
    let (tx, rx) = mpsc::channel(3);
    let w = Writer(tx.wait());
    let r = rx.then(|r| r.unwrap());
    (w, r)
}
