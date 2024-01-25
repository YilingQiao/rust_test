//! Sends a lot of f64 values over shared memory to the server every second.

use dbus::blocking::{Connection, Proxy};
use std::error::Error;
use std::thread::sleep;
use shmem_ipc::sharedring::Sender;
use std::time::Duration;
use std::fs::File;
use std::io::Write;
use std::io::BufWriter;

// use rand::Rng;

const PING_INTERVAL_NS: f64 = 1.0 * 1e9;
const MAX_MESSAGE: usize = 128;
const MIN_BUFLEN: usize = MAX_MESSAGE * 2;
const LOG_WRITER: bool = true;
const GENERATION_INTERVAL_MS: u64 = 10;

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

fn main() -> Result<(), Box<dyn Error>> {
    // Setup a D-Bus connection and call the Setup method of the server.
    let c = Connection::new_session()?;
    let proxy = Proxy::new("com.example.shmemtest", "/shmemtest", Duration::from_millis(3000), &c);


    // let (capacity, memfd, empty_signal, full_signal): (u64, File, File, File) =
    //     proxy.method_call("com.example.shmemtest", "Setup", ())?;

    let open_dbus = || -> Result<(u64, File, File, File), Box<dyn Error>> {
        let (capacity, memfd, empty_signal, full_signal): (u64, File, File, File) =
            proxy.method_call("com.example.shmemtest", "Setup", ())?;

        Ok((capacity, memfd, empty_signal, full_signal))
    };

    // fn open_dbus() -> Result<(), Box<dyn Error>> {
    //     let (capacity, memfd, empty_signal, full_signal): (u64, File, File, File) =
    //         proxy.method_call("com.example.shmemtest", "Setup", ())?;

    //     Ok(())
    // }

    let mut observer_alive = false;

    let mut capacity: u64;
    let mut memfd: File;
    let mut empty_signal: File;
    let mut full_signal: File;


    let mut local_buffer: [f64; MAX_MESSAGE] = [0.; MAX_MESSAGE];


    let write_file = File::create("test_writer.txt")?;
    let mut writer = BufWriter::new(&write_file);

    let mut r = Sender::new(MIN_BUFLEN * 2)?;
    let mut last_ping_nsec: f64 = 0.0;

    loop {

        // get timestamp, now_nsec ~ 1e15
        let (tv_sec, tv_nsec) = now_monotonic();
        let now_nsec = (tv_sec * 1000000000 + tv_nsec) as f64;

        // receive data from generator (or exchange's callback)
        let len_message = MAX_MESSAGE;
        fn load_value(i: usize, len_message: usize, now_nsec: f64) -> f64{
            match i {
                // starting entriy
                0 => - (len_message as f64),
                // time stamp
                1 => now_nsec,
                // other data
                _ => i as f64,
                // _ => ((now_nsec as i64)% (i as i64) )  as f64,
            }
        }
        for i in 0..len_message {
            let val = load_value(i, len_message, now_nsec);
            local_buffer[i as usize] = val;
        }

        // There is no observer working.
        // Try establishing the connection every `PING_INTERVAL_NS` nano second
        if !observer_alive && (now_nsec - last_ping_nsec > PING_INTERVAL_NS) {
            last_ping_nsec = now_nsec;
            match open_dbus() {
                Ok(v) => {
                    observer_alive = true;
                    (capacity, memfd, empty_signal, full_signal) = v;

                    println!("found server! capacity {:?}", capacity);
                    // // Setup the ringbuffer.
                    r = Sender::open(capacity as usize, memfd, empty_signal, full_signal)?;
                },
                Err(_e) => {
                    println!("no observer!");
                },
            };
        }

        // There is one observer
        // Write data to the shared memory `p`
        if observer_alive {
            r.send_raw(|p: *mut f64, mut count| unsafe {
                // We now have a slice of [f64; count], but due to the Rust aliasing rules
                // and the untrusted process restrictions, we cannot convert them into a
                // Rust slice, so we write the data through the raw pointer directly.
                // let mut total_time = 0;

                if len_message > count {
                    println!("ERROR: len_message {} >= count {}", len_message, count);
                }

                if count > MIN_BUFLEN { count = len_message; }

                for i in 0..len_message {
                    *p.offset(i as isize) = local_buffer[i as usize];
                }

                // println!("Sending {} items, in total {}", count, total_time);
                count
            }).unwrap();
        }

        // write to file
        if LOG_WRITER {
            for i in local_buffer {
                let _ = writer.write_fmt( format_args!("{} ", i) );
            }
            let _ = writer.write_fmt( format_args!("end!\n") );
            writer.flush().unwrap();

        }
        sleep(Duration::from_millis(GENERATION_INTERVAL_MS)); 
    }

    
    // Ok(())


}
