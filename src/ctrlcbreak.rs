use ctrlc;
use KitchenSinkErr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Once, ONCE_INIT};
use std::process;

static STOPPED: AtomicUsize = AtomicUsize::new(0);
static INIT: Once = ONCE_INIT;

pub fn stopped() -> bool {
    INIT.call_once(|| {
        ctrlc::set_handler(move || {
            let stops = STOPPED.fetch_add(1, Ordering::SeqCst);
            if stops > 1 {
                eprintln!("STOPPING");
                if stops > 3 {
                    process::exit(1);
                }
            }
        }).expect("Error setting Ctrl-C handler");
    });
    STOPPED.load(Ordering::Relaxed) > 0
}

#[inline]
pub fn running() -> Result<(), KitchenSinkErr> {
    if !stopped() {Ok(())} else {Err(KitchenSinkErr::Stopped)}
}
