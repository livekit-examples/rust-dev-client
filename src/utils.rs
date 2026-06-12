use parking_lot::deadlock;
use std::{thread, time::Duration};

pub fn watch_for_deadlocks() {
    thread::spawn(move || {
        loop {
            thread::sleep(Duration::from_secs(10));
            let deadlocks = deadlock::check_deadlock();
            if deadlocks.is_empty() {
                continue;
            }

            log::error!("{} deadlocks detected", deadlocks.len());
            for (i, threads) in deadlocks.iter().enumerate() {
                log::error!("Deadlock #{}", i);
                for t in threads {
                    log::error!("Thread Id {:#?}: \n{:#?}", t.thread_id(), t.backtrace());
                }
            }
        }
    });
}
