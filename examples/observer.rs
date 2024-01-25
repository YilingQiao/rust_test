//! This is a example service that sums all f64 coming in through shared memory.
//!
//! For the initial setup, the service advertises a setup function over D-Bus.

use dbus::channel::MatchingReceiver;
use dbus::channel::Sender;
use dbus::Path;
use dbus::Message;
use std::sync::Mutex;
use std::sync::Arc;
use std::thread;
use dbus::MethodErr;
use dbus::blocking::Connection;
use std::fs::File;
use dbus_crossroads::{Crossroads};
use std::error::Error;
use shmem_ipc::sharedring::Receiver;

use std::io::Write;
use std::io::BufWriter;

use libc;

// ringbuffer size
const CAPACITY: usize = 500000;
const LOG_OBSERVER: bool = true;
const PRINT_INTERVAL_MS: u64 = 1000;


pub fn now_monotonic() -> (i64, i64) {
    let mut time = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    let ret = unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut time) };
    assert!(ret == 0);
    (time.tv_sec, time.tv_nsec)
}



#[derive(Default)]
struct State {
    sum: Arc<Mutex<f64>>, // total latency 
    ct: Arc<Mutex<f64>>, // total package
}

impl State {
    fn add_receiver(&mut self) -> Result<(u64, File, File, File), Box<dyn Error>> {
        // Create a receiver in shared memory.
        let mut r = Receiver::new(CAPACITY as usize)?;
        let m = r.memfd().as_file().try_clone()?;
        let e = r.empty_signal().try_clone()?;
        let f = r.full_signal().try_clone()?;
        // In this example, we spawn a thread for every ringbuffer.
        // More complex real-world scenarios might multiplex using non-block frameworks,
        // as well as having a mechanism to detect when a client is gone.
        let sum = self.sum.clone();
        let ct = self.ct.clone();

        thread::spawn(move || {

            let write_file = File::create("test_observer.txt").unwrap();
            let mut writer = BufWriter::new(&write_file);
            loop {
                r.block_until_readable().unwrap();
                let mut s: f64 = 0.0;
                let mut now_nsec: f64 = 0.0;
                r.receive_raw(|ptr: *const f64, count| unsafe {
                    // We now have a slice of [f64; count], but due to the Rust aliasing rules
                    // and the untrusted process restrictions, we cannot convert them into a
                    // Rust slice, so we read the data from the raw pointer directly.

                    // let mut total_time = 0;
                    let mut is_time = false;
                    let mut sent_time = 0.0;
                    let mut to_process = 1000.0;
                    // println!("count: {}", count);
                    for i in 0..count {

                        let val = *ptr.offset(i as isize);
                        
                        // this entry is a timestamp
                        if is_time {
                            sent_time = val;
                            is_time = false;
                        }

                        // this entry is a starting of the package
                        if val < 0.0 {
                            is_time = true;
                            to_process = - val
                        }

                        to_process -= 1.0;
                        if to_process == 0.0 {
                            let (tv_sec, tv_nsec) = now_monotonic();
                            // let now_nsec = (tv_sec % 1000) * 1000000000 + tv_nsec;
                            now_nsec = (tv_sec * 1000000000 + tv_nsec) as f64;
                            s += ( now_nsec - sent_time ) / 1e3;

                            *sum.lock().unwrap() += s;
                            *ct.lock().unwrap() += 1.0;

                            break;
                        }
                    }


                    // write to file
                    if LOG_OBSERVER {
                        to_process = 1000.0;
                        let _ = writer.write_fmt( format_args!("{} ", now_nsec) );
                        for i in 0..count {
                            let val = *ptr.offset(i as isize);
                            
                            // this entry is a starting of the package
                            if val < 0.0 {
                                to_process = - val
                            }
                            let _ = writer.write_fmt( format_args!("{} ", val) );
                            to_process -= 1.0;
                            if to_process == 0.0 {
                                break;
                            }
                        }
                        let _ = writer.write_fmt( format_args!("end!\n") );
                        writer.flush().unwrap();
                    }

                    count
                }).unwrap();
            }
        });
        Ok((CAPACITY as u64, m, e, f))
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let c = Connection::new_session()?;
    c.request_name("com.example.shmemtest", false, true, false)?;
    let mut cr = Crossroads::new();
    let iface_token = cr.register("com.example.shmemtest", |b| {
        b.method("Setup", (), ("capacity", "memfd", "empty_signal", "full_signal"), |_, state: &mut State, _: ()| {
            state.add_receiver().map_err(|e| {
                println!("{}, {:?}", e, e.source());
                MethodErr::failed("failed to setup shared memory")
            })
        });
        b.signal::<(f64,), _>("Sum", ("sum",));
        b.signal::<(f64,), _>("Ct", ("ct",));
    });
    cr.insert("/shmemtest", &[iface_token], State::default());
    let acr = Arc::new(Mutex::new(cr));
    let acr_clone = acr.clone();
    c.start_receive(dbus::message::MatchRule::new_method_call(), Box::new(move |msg, conn| {
        acr_clone.lock().unwrap().handle_message(msg, conn).unwrap();
        true
    }));

    loop {
        c.process(std::time::Duration::from_millis(PRINT_INTERVAL_MS))?;
        let mut cr = acr.lock().unwrap();
        let state: &mut State = cr.data_mut(&Path::from("/shmemtest")).unwrap();
        let mut sum = state.sum.lock().unwrap();
        let mut ct = state.ct.lock().unwrap();
        if *sum != 0.0 {
            println!("average latency: {} us, package count: {}", *sum / *ct, ct);
            c.send(Message::new_signal("/shmemtest", "com.example.shmemtest", "Sum").unwrap().append1(*sum)).unwrap();
            c.send(Message::new_signal("/shmemtest", "com.example.shmemtest", "Ct").unwrap().append1(*ct)).unwrap();
            *sum = 0.0;
            *ct = 0.0;
        }
    }
}
