#[macro_use]
extern crate clap;
#[macro_use]
extern crate common_failures;
extern crate caps;
#[macro_use]
extern crate failure;
extern crate libc;
extern crate redis;
extern crate thread_priority;

mod assert;
mod gpio;
mod rcs;
mod redis_streams;

use common_failures::prelude::*;

use assert::assert;
use caps::CapSet;
use caps::Capability;
use clap::App;
use clap::Arg;
use rcs::RemoteControl;
use rcs::Switch;
use rcs::SwitchCode::SwitchA;
use rcs::SwitchCode::SwitchB;
use rcs::SwitchCode::SwitchC;
use rcs::SwitchCode::SwitchD;
use rcs::SwitchCode::SwitchE;
use rcs::SwitchState;
use rcs::SwitchState::Off;
use rcs::SwitchState::On;
use redis::Commands;
use redis::Connection;
use redis::Value;
use redis_streams::as_bulk;
use redis_streams::as_stream_entry;
use redis_streams::is_nil;
use redis_streams::EntryId;
use std::collections::HashMap;
use thread_priority::set_thread_priority;
use thread_priority::thread_native_id;
use thread_priority::RealtimeThreadSchedulePolicy;
use thread_priority::ThreadPriority;
use thread_priority::ThreadSchedulePolicy;

const ARG_REDIS_HOST: &str = "redis-host";
const ARG_REDIS_PORT: &str = "redis-port";
const ARG_NAME: &str = "name";
const ARG_GPIO_PIN: &str = "gpio-pin";

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

fn map(_: EntryId, values: HashMap<String, String>) -> Result<Option<(Switch, SwitchState)>> {
    let system_code = match values.get("system_code") {
        Some(system_code) => {
            match system_code.len() {
                5 => (),
                _ => return Ok(None), //TODO: log warning
            };

            let mut result = [false; 5];
            for (i, c) in system_code.chars().enumerate() {
                result[i] = match c {
                    '0' => false,
                    '1' => true,
                    _ => return Ok(None), //TODO: log warning
                };
            }
            result
        }
        None => return Ok(None), //TODO: log warning
    };

    let switch = match values.get("switch") {
        Some(switch) => match switch.as_str() {
            "A" => SwitchA,
            "B" => SwitchB,
            "C" => SwitchC,
            "D" => SwitchD,
            "E" => SwitchE,
            _ => return Ok(None), //TODO: log warning
        },
        None => return Ok(None), //TODO: log warning
    };

    let state = match values.get("state") {
        Some(state) => {
            match state.as_str() {
                "On" => On,
                "Off" => Off,
                _ => return Ok(None), //TODO: log warning
            }
        }
        None => return Ok(None), //TODO: log warning
    };

    Ok(Some((Switch::new(&system_code, switch), state)))
}

fn reduce(items: Vec<(Switch, SwitchState)>) -> Result<HashMap<Switch, SwitchState>> {
    let mut result: HashMap<Switch, SwitchState> = HashMap::new();
    for item in items {
        result.insert(item.0, item.1);
    }
    Ok(result)
}

fn process_stream<M, R, C, MappedItem, ReducedItem>(
    stream_name: String,
    mut connection: Connection,
    map: M,
    reduce: R,
    mut commit: C,
) -> Result<()>
where
    M: (Fn(EntryId, HashMap<String, String>) -> Result<Option<MappedItem>>),
    R: (Fn(Vec<MappedItem>) -> Result<ReducedItem>),
    C: (FnMut(ReducedItem) -> Result<()>),
{
    let start_key = format!("{}_start", stream_name);

    let mut start = match connection.get::<&String, Option<String>>(&start_key)? {
        Some(start) => EntryId::from_str(start)?,
        None => EntryId::new(0, 0),
    };
    loop {
        println!("{}", start);

        let streams: Value = redis::cmd("XREAD")
            .arg("BLOCK")
            .arg("5000")
            .arg("STREAMS")
            .arg(&stream_name)
            .arg(start.to_string())
            .query(&mut connection)?;

        // nil == timeout
        if !is_nil(&streams) {
            let mut streams = as_bulk(streams)?;
            assert(
                || streams.len() == 1,
                format!("We query only for one stream, but got: {:?}", streams),
            )?;

            // extract rcs stream
            let mut stream_name_to_entries = as_bulk(streams.remove(0))?;
            // extract rcs stream entries. 0 == name, 1 == entries
            let stream_entries = as_bulk(stream_name_to_entries.remove(1))?;

            let mut items = Vec::new();

            for stream_entry in stream_entries {
                let stream_entry = as_stream_entry(stream_entry)?;
                let entry_id = stream_entry.0;
                let entry_values = stream_entry.1;
                println!("{}: {:?}", entry_id, entry_values);

                if let Some(item) = map(entry_id.clone(), entry_values)? {
                    items.push(item);
                }

                start = entry_id.next();
            }

            commit(reduce(items)?)?;

            connection.set(&start_key, start.to_string())?;
        }
    }
}

fn run() -> Result<()> {
    let args = App::new(crate_name!())
        .version(crate_version!())
        .author(crate_authors!())
        .arg(
            Arg::with_name(ARG_REDIS_HOST)
                .long(ARG_REDIS_HOST)
                .multiple(false)
                .takes_value(true)
                .required(false)
                .default_value("localhost"),
        )
        .arg(
            Arg::with_name(ARG_REDIS_PORT)
                .long(ARG_REDIS_PORT)
                .multiple(false)
                .takes_value(true)
                .required(false)
                .default_value("6379"),
        )
        .arg(
            Arg::with_name(ARG_NAME)
                .long(ARG_NAME)
                .multiple(false)
                .takes_value(true)
                .required(false)
                .default_value("rcs"),
        )
        .arg(
            Arg::with_name(ARG_GPIO_PIN)
                .long(ARG_GPIO_PIN)
                .multiple(false)
                .takes_value(true)
                .required(false)
                .default_value("17"),
        )
        .get_matches();

    let redis_host = value_t!(args, ARG_REDIS_HOST, String)?;
    let redis_port = value_t!(args, ARG_REDIS_PORT, usize)?;
    let name = value_t!(args, ARG_NAME, String)?;
    let gpio_pin = value_t!(args, ARG_GPIO_PIN, usize)?;

    // requires:
    // sudo setcap cap_sys_nice=ep <file>
    try_upgrade_thread_priority()?;

    let redis_connection =
        redis::Client::open(format!("redis://{}:{}", redis_host, redis_port))?.get_connection()?;

    let mut rc = RemoteControl::open(gpio_pin)?;

    let commit = |state: HashMap<Switch, SwitchState>| -> Result<()> {
        println!("{:?}", state);
        for (switch, state) in state {
            rc.send(&switch, state);
        }
        Ok(())
    };

    process_stream(name, redis_connection, map, reduce, commit)?;

    Ok(())
}

quick_main!(run);
