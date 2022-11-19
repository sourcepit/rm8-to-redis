mod gpio;
mod redis_shim;
mod rm8_ctl;
mod rm8_types;

use std::collections::HashMap;
use std::time::Duration;
use std::time::Instant;

use anyhow::Result;
use clap::Parser;
use log::info;
use log::warn;
use redis_shim::dispatch_relay_states;
use rm8_ctl::Rm8Control;
use rm8_types::RelayId;
use rm8_types::RelayId::*;
use rm8_types::RelayState;
use rm8_types::RelayState::Off;
use std::cmp::Ordering::Greater;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[arg(long, short = 'q', default_value_t = false, conflicts_with = "verbose")]
    quiet: bool,
    #[arg(long, short = 'v', action = clap::ArgAction::Count)]
    verbose: u8,
    #[arg(long, default_value_t = String::from("localhost"))]
    redis_host: String,
    #[arg(long, default_value_t = 6379)]
    redis_port: usize,
    #[arg(long, default_value_t = String::from("rm8"))]
    redis_stream_name: String,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    stderrlog::new()
        .module(module_path!())
        .timestamp(stderrlog::Timestamp::Second)
        .verbosity(usize::from(cli.verbose))
        .quiet(cli.quiet)
        .init()?;

    let mut redis = redis::Client::open(format!("redis://{}:{}", cli.redis_host, cli.redis_port))?
        .get_connection()?;

    let stream_key = cli.redis_stream_name;
    let last_entry_id_key = format!("{}_start", stream_key);

    let block_rate_ms = Duration::from_secs(1).as_millis().try_into()?;
    let burts_rate = Duration::from_secs(5);

    let gpio_pins = vec![6, 13, 19, 26, 12, 16, 20, 21];

    let mut rm8_ctl = Rm8Control::open(gpio_pins, true)?;

    let mut event_times = new_event_times_map();

    rm8_ctl.set_all(Off);

    let handle_relay_states = &mut |relay_states| {
        if let Some(relay_states) = relay_states {
            set_states(&mut rm8_ctl, &relay_states);
            update_event_times(&mut event_times, &relay_states.into_keys().collect());
        }
        check_burst_rate_violations(&mut rm8_ctl, &mut event_times, &burts_rate);
        Ok(())
    };

    if let Err(e) = dispatch_relay_states(
        &mut redis,
        &stream_key,
        &last_entry_id_key,
        block_rate_ms,
        |convertion_error| {
            warn!("{}", convertion_error);
            None
        },
        handle_relay_states,
    ) {
        rm8_ctl.set_all(Off);
        return Err(e.into());
    }

    Ok(())
}

fn set_states(rm8_ctl: &mut Rm8Control, relay_states: &HashMap<RelayId, RelayState>) {
    for (relay, state) in relay_states {
        rm8_ctl.set(relay, state.to_owned());
    }
}

fn update_event_times(event_times: &mut HashMap<RelayId, Instant>, event_triggers: &Vec<RelayId>) {
    let now = Instant::now();
    for relay in event_triggers {
        event_times.insert(relay.to_owned(), now);
    }
}

fn check_burst_rate_violations(
    rm8_ctl: &mut Rm8Control,
    event_times: &mut HashMap<RelayId, Instant>,
    burts_rate: &Duration,
) {
    let now = Instant::now();

    let mut bursted = Vec::<RelayId>::new();

    for (relay_id, event_time) in event_times.iter() {
        let relay_id = relay_id.to_owned();
        let event_time = event_time.to_owned();

        let duration_since = now.duration_since(event_time);
        if let Greater = duration_since.cmp(burts_rate) {
            bursted.push(relay_id);
        }
    }

    for relay_id in bursted.iter() {
        warn!(
            "Set relay '{:?}' to 'Off' due to burst rate violation",
            relay_id
        );
        rm8_ctl.set(relay_id, Off);
    }

    for relay_id in bursted {
        event_times.insert(relay_id, now);
    }
}

fn new_event_times_map() -> HashMap<RelayId, Instant> {
    let mut event_times = HashMap::<RelayId, Instant>::new();
    let now = Instant::now();
    event_times.insert(Relay1, now);
    event_times.insert(Relay2, now);
    event_times.insert(Relay3, now);
    event_times.insert(Relay4, now);
    event_times.insert(Relay5, now);
    event_times.insert(Relay6, now);
    event_times.insert(Relay7, now);
    event_times.insert(Relay8, now);
    event_times
}
