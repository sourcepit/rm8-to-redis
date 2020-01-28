use common_failures::prelude::*;

use assert::assert;
use redis::Value;
use std::collections::HashMap;
use std::fmt::Display;
use std::fmt::Formatter;
use std::fmt::Result as FmtResult;

#[derive(Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct EntryId {
    time: u64,
    sequence_number: u32,
}

impl EntryId {
    pub fn from_str<S>(string: S) -> Result<EntryId>
    where
        S: Into<String>,
    {
        let string = string.into();
        let parts: Vec<&str> = string.split('-').collect();
        assert(
            || parts.len() == 2,
            format!("Illegal redis stream entry id '{}'", string),
        )?;

        let time = parts[0].parse::<u64>()?;
        let sequence_number = parts[1].parse::<u32>()?;

        Ok(EntryId {
            time,
            sequence_number,
        })
    }

    pub fn new(time: u64, sequence_number: u32) -> EntryId {
        EntryId {
            time,
            sequence_number,
        }
    }

    pub fn next(&self) -> EntryId {
        EntryId::new(self.time, self.sequence_number + 1)
    }
}

impl Display for EntryId {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{}-{}", self.time, self.sequence_number)
    }
}

pub fn is_nil(value: &Value) -> bool {
    match value {
        Value::Nil => true,
        _ => false,
    }
}

pub fn as_bulk(value: Value) -> Result<Vec<Value>> {
    match value {
        Value::Bulk(value) => Ok(value),
        _ => Err(format_err!(
            "Expected redis bulk value but got {:?}.",
            value
        )),
    }
}

pub fn as_status(value: Value) -> Result<String> {
    match value {
        Value::Status(value) => Ok(value),
        _ => Err(format_err!(
            "Expected redis status value but got {:?}.",
            value
        )),
    }
}

pub fn as_string(value: Value) -> Result<String> {
    match value {
        Value::Data(value) => Ok(String::from_utf8(value)?),
        _ => Err(format_err!(
            "Expected utf8 encoded redis data value but got {:?}.",
            value
        )),
    }
}

pub fn as_stream_entry(value: Value) -> Result<(EntryId, HashMap<String, String>)> {
    let mut stream_entry = as_bulk(value)?;
    assert(
        || stream_entry.len() == 2,
        format!("Invalid redis stream entry: {:?}", stream_entry),
    )?;

    let entry_id = EntryId::from_str(as_status(stream_entry.remove(0))?)?;

    let mut entry_items = as_bulk(stream_entry.remove(0))?;
    assert(
        || entry_items.len() % 2 == 0,
        format!("Invalid redis stream entry with id: {}", entry_id),
    )?;

    let mut entry_values = HashMap::new();
    for _ in (0..entry_items.len()).step_by(2) {
        let key = as_string(entry_items.remove(0))?;
        let value = as_string(entry_items.remove(0))?;
        entry_values.insert(key, value);
    }

    Ok((entry_id, entry_values))
}
