#[macro_use]
extern crate common_failures;
extern crate caps;
#[macro_use]
extern crate failure;
extern crate libc;
extern crate redis;
extern crate thread_priority;

mod gpio;
mod rcs;

use common_failures::prelude::*;

use caps::CapSet;
use caps::Capability;
use rcs::RemoteControl;
use rcs::Switch;
use rcs::SwitchCode::SwitchB;
use rcs::SwitchState::Off;
use rcs::SwitchState::On;
use redis::Commands;
use std::thread;
use std::time::Duration;
use thread_priority::set_thread_priority;
use thread_priority::thread_native_id;
use thread_priority::RealtimeThreadSchedulePolicy;
use thread_priority::ThreadPriority;
use thread_priority::ThreadSchedulePolicy;

fn try_upgrade_thread_priority() -> Result<()> {
    let has_cap_sys_nice = match caps::has_cap(None, CapSet::Permitted, Capability::CAP_SYS_NICE) {
        Ok(v) => v,
        Err(e) => return Err(format_err!("{}", e)),
    };
    if has_cap_sys_nice {
        let thread_id = thread_native_id();
        let res = set_thread_priority(
            thread_id,
            ThreadPriority::Max,
            ThreadSchedulePolicy::Realtime(RealtimeThreadSchedulePolicy::Fifo),
        );
        match res {
            Ok(_) => {}
            Err(e) => return Err(format_err!("{:?}", e)),
        }
    };
    Ok(())
}

fn run() -> Result<()> {
    // requires:
    // sudo setcap cap_sys_nice=ep <file>
    try_upgrade_thread_priority()?;

    let client = redis::Client::open("redis://127.0.0.1/")?;
    let mut con = client.get_connection()?;

    let stream_name = "rcs";
    
    let result = redis::cmd("XRANGE")
        .arg("rcs")
        .arg("-")
        .arg("+")
        .query(&mut con)?;

    let pin = 17;
    let system_code = [true, true, true, false, false];

    let switch_b = Switch::new(&system_code, SwitchB);

    let mut rc = RemoteControl::open(pin)?;

    rc.send(&switch_b, On);
    thread::sleep(Duration::from_secs(2));

    rc.send(&switch_b, Off);
    thread::sleep(Duration::from_secs(2));

    Ok(())
}

quick_main!(run);
