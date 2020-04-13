#[macro_use]
extern crate clap;
#[macro_use]
extern crate common_failures;
extern crate caps;
#[macro_use]
extern crate failure;
extern crate libc;
extern crate redis;
extern crate serde;
extern crate serde_json;
extern crate thread_priority;
#[macro_use]
extern crate log;

mod assert;
mod gpio;
mod redis_streams;
mod rm8;

use common_failures::prelude::*;

use caps::CapSet;
use caps::Capability;
use clap::App;
use clap::Arg;
use redis::Commands;
use redis::Connection;
use redis_streams::process_stream;
use redis_streams::EntryId;
use rm8::RemoteControl;
use rm8::Switch;
use rm8::SwitchCode::Relay1;
use rm8::SwitchCode::Relay2;
use rm8::SwitchCode::Relay3;
use rm8::SwitchCode::Relay4;
use rm8::SwitchCode::Relay5;
use rm8::SwitchCode::Relay6;
use rm8::SwitchCode::Relay7;
use rm8::SwitchCode::Relay8;
use rm8::SwitchState;
use rm8::SwitchState::Off;
use rm8::SwitchState::On;
use serde::Deserialize;
use serde::Serialize;
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
const ARG_VERBOSITY: &str = "verbosity";
const ARG_QUIET: &str = "quiet";

#[derive(Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
struct SwitchStates {
    states: Vec<(Switch, SwitchState)>,
}

impl SwitchStates {
    pub fn new() -> SwitchStates {
        SwitchStates { states: Vec::new() }
    }

    pub fn set_state(&mut self, switch: &Switch, state: SwitchState) {
        for entry in &mut self.states {
            let existing_switch = &entry.0;
            if existing_switch == switch {
                entry.1 = state;
                return;
            }
        }
        self.states.push((switch.clone(), state));
    }

    pub fn get_state(&self, switch: &Switch) -> Option<SwitchState> {
        for entry in &self.states {
            let existing_switch = &entry.0;
            if existing_switch == switch {
                return Some(entry.1);
            }
        }
        None
    }

    pub fn iter(&self) -> std::slice::Iter<(rm8::Switch, rm8::SwitchState)> {
        self.states.iter()
    }
}

quick_main!(run);

fn run() -> Result<()> {
    let args = App::new(crate_name!())
        .version(crate_version!())
        .author(crate_authors!())
        .arg(
            Arg::with_name(ARG_VERBOSITY)
                .long(ARG_VERBOSITY)
                .short("v")
                .multiple(true)
                .takes_value(false)
                .required(false),
        )
        .arg(
            Arg::with_name(ARG_QUIET)
                .long(ARG_QUIET)
                .short("q")
                .multiple(false)
                .takes_value(false)
                .required(false),
        )
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
                .default_value("rm8"),
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

    let verbosity = args.occurrences_of(ARG_VERBOSITY) as usize + 1;
    let quiet = args.is_present(ARG_QUIET);

    stderrlog::new()
        .module(module_path!())
        .timestamp(stderrlog::Timestamp::Second)
        .verbosity(verbosity)
        .quiet(quiet)
        .init()?;

    let redis_host = value_t!(args, ARG_REDIS_HOST, String)?;
    let redis_port = value_t!(args, ARG_REDIS_PORT, usize)?;
    let name = value_t!(args, ARG_NAME, String)?;
    let gpio_pin = value_t!(args, ARG_GPIO_PIN, usize)?;

    // requires:
    // sudo setcap cap_sys_nice=ep <file>
    try_upgrade_thread_priority()?;

    let mut redis_connection =
        redis::Client::open(format!("redis://{}:{}", redis_host, redis_port))?.get_connection()?;

    let state_key = format!("{}_state", name);

    let result: Option<Vec<u8>> = redis_connection.get(&state_key)?;
    let mut switch_states: SwitchStates = match result {
        Some(result) => serde_json::from_slice(result.as_slice())?,
        None => SwitchStates::new(),
    };

    let mut rc = RemoteControl::open(gpio_pin)?;

    let commit = |connection: &mut Connection,
                  initialized: bool,
                  state: HashMap<Switch, SwitchState>|
     -> Result<()> {
        if initialized {
            for (switch, state) in state {
                info!("Set {:?} {:?}", switch, state);
                rc.send(&switch, state);
                switch_states.set_state(&switch, state);
            }
        } else {
            for (switch, state) in state {
                switch_states.set_state(&switch, state);
            }
            for (switch, state) in switch_states.iter() {
                info!("Set {:?} {:?}", switch, state);
                rc.send(switch, *state);
            }
        }

        let json = serde_json::to_vec(&switch_states)?;
        connection.set(&state_key, json)?;

        Ok(())
    };

    process_stream(name, redis_connection, map, reduce, commit)?;

    Ok(())
}

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
            "1" => Relay1,
            "2" => Relay2,
            "3" => Relay3,
            "4" => Relay4,
            "5" => Relay5,
            "6" => Relay6,
            "7" => Relay7,
            "8" => Relay8,
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

#[cfg(test)]
mod tests {
    // Note this useful idiom: importing names from outer (for mod tests) scope.
    use super::rm8::SwitchCode;
    use super::rm8::SwitchState;
    use super::*;

    #[test]
    fn test_switch_states_set_state() {
        let mut state = SwitchStates::new();
        assert_eq!(0, state.states.len());

        // test insert state of switch 1A
        let sys_code = [true, false, false, false, false];
        let switch_code = SwitchCode::Relay1;
        let switch = Switch::new(&sys_code, switch_code);

        state.set_state(&switch, SwitchState::On);
        assert_eq!(1, state.states.len());
        assert_eq!(Some(SwitchState::On), state.get_state(&switch));

        // test change state of switch 1A
        let sys_code = [true, false, false, false, false];
        let switch_code = SwitchCode::Relay1;
        let switch = Switch::new(&sys_code, switch_code);

        state.set_state(&switch, SwitchState::Off);
        assert_eq!(1, state.states.len());
        assert_eq!(Some(SwitchState::Off), state.get_state(&switch));

        // test insert state of another switch
        let sys_code_2 = [true, true, false, false, false];
        let switch_code_2 = SwitchCode::Relay1;
        let switch_2 = Switch::new(&sys_code_2, switch_code_2);

        state.set_state(&switch_2, SwitchState::On);
        assert_eq!(2, state.states.len());
        assert_eq!(Some(SwitchState::Off), state.get_state(&switch));
        assert_eq!(Some(SwitchState::On), state.get_state(&switch_2));
    }
}
