use std::future::Future;
use std::pin::Pin;
use std::task::Poll;

pub struct NonBlock<F> {
    future: F,
    label: &'static str,
}

impl<F: Future> NonBlock<F> {
    #[inline]
    pub fn new(label: &'static str, future: F) -> Self {
        Self { future, label }
    }
}

impl<F: Future> Future for NonBlock<F> {
    type Output = F::Output;
    fn poll(self: Pin<&mut Self>, ctx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
        let start = std::time::Instant::now();
        let label = self.label;
        let projected = unsafe { self.map_unchecked_mut(|s| &mut s.future) };
        let res = projected.poll(ctx);
        let elapsed = start.elapsed().as_secs();
        if elapsed >= 1 {
            eprintln!("blocking poll: {} took {}s", label, elapsed);
        }
        res
    }
}
