use crate::rm8_types::RelayState;

use super::rm8_types::RelayId;
use super::rm8_types::RelayId::*;
use super::rm8_types::RelayState::*;

use anyhow::Result;
use redis::streams::StreamReadOptions;
use redis::streams::StreamReadReply;
use redis::Commands;
use redis::Connection;
use redis::FromRedisValue;
use redis::RedisError;
use redis::Value;
use std::collections::HashMap;
use thiserror::Error;

#[derive(Debug, Error)]
#[error("Failed to convert entry '{entry_id}' of stream '{stream_key}': {message}")]
pub struct ConvertionError {
    stream_key: String,
    entry_id: String,
    message: String,
    #[source]
    source: Option<RedisError>,
}

impl ConvertionError {
    pub fn new<S1: Into<String>, S2: Into<String>, S3: Into<String>>(
        stream_key: S1,
        entry_id: S2,
        message: S3,
    ) -> Self {
        Self {
            stream_key: stream_key.into(),
            entry_id: entry_id.into(),
            message: message.into(),
            source: None,
        }
    }

    pub fn from_redis_error<S1: Into<String>, S2: Into<String>>(
        stream_key: S1,
        entry_id: S2,
        source: RedisError,
    ) -> Self {
        Self {
            stream_key: stream_key.into(),
            entry_id: entry_id.into(),
            message: format!("{}", source),
            source: Some(source),
        }
    }
}

#[derive(Debug, Error)]
enum XReadRelayStatesError {
    #[error(transparent)]
    ConvertionError(#[from] ConvertionError),

    #[error(transparent)]
    RedisError(#[from] RedisError),
}

#[derive(Debug, Error)]
pub enum DispatchRelayStatesError {
    #[error(transparent)]
    ConvertionError(#[from] ConvertionError),

    #[error(transparent)]
    RedisError(#[from] RedisError),

    #[error(transparent)]
    RelayStatesHandlerError(#[from] anyhow::Error),
}

pub fn dispatch_relay_states<E, H>(
    mut redis: &mut Connection,
    stream_key: &str,
    last_entry_id_key: &str,
    block_ms: usize,
    convertion_error_handler: E,
    relay_states_handler: &mut H,
) -> Result<(), DispatchRelayStatesError>
where
    E: Fn(ConvertionError) -> Option<ConvertionError>,
    H: FnMut(Option<HashMap<RelayId, RelayState>>) -> Result<()>,
{
    loop {
        let last_entry_id = redis.get::<&str, Option<String>>(&last_entry_id_key)?;

        let last_entry_id = match last_entry_id {
            Some(last_entry_id) => last_entry_id,
            None => String::from("$"),
        };

        let xread_result = xread_relay_states(
            &mut redis,
            &stream_key,
            &last_entry_id,
            block_ms,
            &convertion_error_handler,
        );

        let xread_result = match xread_result {
            Ok(xread_result) => xread_result,
            Err(e) => match e {
                XReadRelayStatesError::ConvertionError(e) => {
                    return Err(DispatchRelayStatesError::ConvertionError(e))
                }
                XReadRelayStatesError::RedisError(e) => {
                    return Err(DispatchRelayStatesError::RedisError(e))
                }
            },
        };

        let xread_result = match xread_result {
            Some(xread_result) => (Some(xread_result.0), xread_result.1),
            None => (None, None),
        };

        relay_states_handler(xread_result.1)?;

        if let Some(last_entry_id) = xread_result.0 {
            redis.set(&last_entry_id_key, last_entry_id)?;
        }
    }
}

fn xread_relay_states<E>(
    redis: &mut Connection,
    stream_key: &str,
    last_entry_id: &str,
    block_ms: usize,
    convertion_error_handler: E,
) -> Result<Option<(String, Option<HashMap<RelayId, RelayState>>)>, XReadRelayStatesError>
where
    E: Fn(ConvertionError) -> Option<ConvertionError>,
{
    let opts = StreamReadOptions::default().block(block_ms);
    let reply: Option<StreamReadReply> =
        redis.xread_options(&[stream_key], &[last_entry_id], &opts)?;

    let reply = match reply {
        Some(reply) => reply,
        None => return Ok(None),
    };

    for stream in reply.keys {
        let mut relay_states = HashMap::<RelayId, RelayState>::new();
        let mut entry_id = String::from(last_entry_id);
        for entry in stream.ids {
            entry_id = entry.id;
            match from_redis(&stream.key, &entry_id, entry.map) {
                Ok(relay) => relay_states.insert(relay.0, relay.1),
                Err(e) => {
                    if let Some(e) = convertion_error_handler(e) {
                        return Err(XReadRelayStatesError::ConvertionError(e));
                    }
                    continue;
                }
            };
        }

        let relay_states = match relay_states.is_empty() {
            true => None,
            false => Some(relay_states),
        };

        return Ok(Some((entry_id, relay_states)));
    }
    Ok(None)
}

fn from_redis(
    stream_key: &str,
    entry_id: &str,
    fields: HashMap<String, Value>,
) -> Result<(RelayId, RelayState), ConvertionError> {
    let relay = get_as_string(stream_key, entry_id, &fields, "relay")?;

    let relay = match relay.as_str() {
        "1" => Relay1,
        "2" => Relay2,
        "3" => Relay3,
        "4" => Relay4,
        "5" => Relay5,
        "6" => Relay6,
        "7" => Relay7,
        "8" => Relay8,
        unknown => {
            return Err(ConvertionError::new(
                stream_key,
                entry_id,
                format!("Invalid relay number '{}'", unknown),
            ))
        }
    };

    let state = get_as_string(stream_key, entry_id, &fields, "state")?;

    let state = match state.as_str() {
        "On" => On,
        "Off" => Off,
        unknown => {
            return Err(ConvertionError::new(
                stream_key,
                entry_id,
                format!("Invalid relay state '{}'", unknown),
            ))
        }
    };

    Ok((relay, state))
}

fn get_as_string(
    stream_key: &str,
    entry_id: &str,
    fields: &HashMap<String, Value>,
    field: &str,
) -> Result<String, ConvertionError> {
    let value = match fields.get(field) {
        Some(value) => value,
        None => {
            return Err(ConvertionError::new(
                stream_key,
                entry_id,
                format!("Field '{}' is missing", field),
            ))
        }
    };
    let value = match String::from_redis_value(value) {
        Err(e) => return Err(ConvertionError::from_redis_error(stream_key, entry_id, e)),
        Ok(value) => value,
    };
    Ok(value)
}
