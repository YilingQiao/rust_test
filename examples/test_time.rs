
use std::time::{Duration, SystemTime, UNIX_EPOCH, Instant};
use libc;

pub fn now_monotonic() -> (i64, i64) {
    let mut time = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    let ret = unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut time) };
    assert!(ret == 0);
    (time.tv_sec, time.tv_nsec)
}



fn main() {
    // let now = SystemTime::now();

    let n_rep = 1_000_000;
    const MAX_MESSAGE: usize = 256;

    let start = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();

    for _ in 0..n_rep {
       let mut local_buffer: [f64; MAX_MESSAGE] = [0.; MAX_MESSAGE];
        for i in 0..MAX_MESSAGE {
            local_buffer[i as usize] = i as f64;
        }
    }
    let end = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();


    println!("write {}-array time: {} ns", MAX_MESSAGE, (end - start).as_nanos() / n_rep);

}