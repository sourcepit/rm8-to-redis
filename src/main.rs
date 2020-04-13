#[macro_use]
extern crate clap;
#[macro_use]
extern crate common_failures;
#[macro_use]
extern crate failure;
extern crate libc;
extern crate redis;
extern crate serde;
extern crate serde_json;
#[macro_use]
extern crate log;

mod assert;
mod gpio;
mod redis_streams;
mod rm8;

use common_failures::prelude::*;

use clap::App;
use clap::Arg;
use redis::Commands;
use redis::Connection;
use redis_streams::process_stream;
use redis_streams::EntryId;
use rm8::Relay;
use rm8::Relay::Relay1;
use rm8::Relay::Relay2;
use rm8::Relay::Relay3;
use rm8::Relay::Relay4;
use rm8::Relay::Relay5;
use rm8::Relay::Relay6;
use rm8::Relay::Relay7;
use rm8::Relay::Relay8;
use rm8::RelayState;
use rm8::RelayState::Off;
use rm8::RelayState::On;
use rm8::RemoteControl;
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;

const ARG_REDIS_HOST: &str = "redis-host";
const ARG_REDIS_PORT: &str = "redis-port";
const ARG_NAME: &str = "name";
const ARG_GPIO_PIN: &str = "gpio-pin";
const ARG_VERBOSITY: &str = "verbosity";
const ARG_QUIET: &str = "quiet";

#[derive(Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
struct SwitchStates {
    states: Vec<(Relay, RelayState)>,
}

impl SwitchStates {
    pub fn new() -> SwitchStates {
        SwitchStates { states: Vec::new() }
    }

    pub fn set_state(&mut self, relay: &Relay, state: RelayState) {
        for entry in &mut self.states {
            let existing_relay = &entry.0;
            if existing_relay == relay {
                entry.1 = state;
                return;
            }
        }
        self.states.push((relay.clone(), state));
    }

    pub fn get_state(&self, relay: &Relay) -> Option<RelayState> {
        for entry in &self.states {
            let existing_relay = &entry.0;
            if existing_relay == relay {
                return Some(entry.1);
            }
        }
        None
    }

    pub fn iter(&self) -> std::slice::Iter<(rm8::Relay, rm8::RelayState)> {
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

    let mut redis_connection =
        redis::Client::open(format!("redis://{}:{}", redis_host, redis_port))?.get_connection()?;

    let state_key = format!("{}_state", name);

    let result: Option<Vec<u8>> = redis_connection.get(&state_key)?;
    let mut relay_states: SwitchStates = match result {
        Some(result) => serde_json::from_slice(result.as_slice())?,
        None => SwitchStates::new(),
    };

    let mut rc = RemoteControl::open(gpio_pin)?;

    let commit = |connection: &mut Connection,
                  initialized: bool,
                  state: HashMap<Relay, RelayState>|
     -> Result<()> {
        if initialized {
            for (relay, state) in state {
                info!("Set {:?} {:?}", relay, state);
                rc.send(&relay, state);
                relay_states.set_state(&relay, state);
            }
        } else {
            for (relay, state) in state {
                relay_states.set_state(&relay, state);
            }
            for (relay, state) in relay_states.iter() {
                info!("Set {:?} {:?}", relay, state);
                rc.send(relay, *state);
            }
        }

        let json = serde_json::to_vec(&relay_states)?;
        connection.set(&state_key, json)?;

        Ok(())
    };

    process_stream(name, redis_connection, map, reduce, commit)?;

    Ok(())
}

fn map(_: EntryId, values: HashMap<String, String>) -> Result<Option<(Relay, RelayState)>> {
    let relay = match values.get("relay") {
        Some(relay) => match relay.as_str() {
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

    Ok(Some((relay, state)))
}

fn reduce(items: Vec<(Relay, RelayState)>) -> Result<HashMap<Relay, RelayState>> {
    let mut result: HashMap<Relay, RelayState> = HashMap::new();
    for item in items {
        result.insert(item.0, item.1);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    // Note this useful idiom: importing names from outer (for mod tests) scope.
    use super::rm8::Relay;
    use super::rm8::RelayState;
    use super::*;

    #[test]
    fn test_relay_states_set_state() {
        let mut state = SwitchStates::new();
        assert_eq!(0, state.states.len());

        // test insert state of relay 1A
        let relay = Relay::Relay1;

        state.set_state(&relay, RelayState::On);
        assert_eq!(1, state.states.len());
        assert_eq!(Some(RelayState::On), state.get_state(&relay));

        // test change state of relay 1A
        let relay = Relay::Relay1;

        state.set_state(&relay, RelayState::Off);
        assert_eq!(1, state.states.len());
        assert_eq!(Some(RelayState::Off), state.get_state(&relay));

        // test insert state of another relay
        let relay_2 = Relay::Relay2;

        state.set_state(&relay_2, RelayState::On);
        assert_eq!(2, state.states.len());
        assert_eq!(Some(RelayState::Off), state.get_state(&relay));
        assert_eq!(Some(RelayState::On), state.get_state(&relay_2));
    }
}
