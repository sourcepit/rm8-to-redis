use common_failures::prelude::*;

use assert::assert;
use redis::Commands;
use redis::Connection;
use redis::Value;
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;
use std::fmt::Display;
use std::fmt::Formatter;
use std::fmt::Result as FmtResult;

#[derive(Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
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

pub fn read_stream<S>(
    connection: &mut Connection,
    stream_name: S,
    start_entry: &EntryId,
    block: Option<usize>,
) -> Result<Vec<(EntryId, HashMap<String, String>)>>
where
    S: Into<String>,
{
    let mut cmd = redis::cmd("XREAD");

    if let Some(block) = block {
        cmd.arg("BLOCK").arg(block.to_string());
    }

    cmd.arg("STREAMS")
        .arg(stream_name.into())
        .arg(start_entry.to_string());

    let streams = cmd.query(connection)?;
    let mut result = Vec::new();

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

        for stream_entry in stream_entries {
            let stream_entry = as_stream_entry(stream_entry)?;
            result.push(stream_entry);
        }
    }

    Ok(result)
}

pub fn process_stream<M, R, C, MappedItem, ReducedItem>(
    stream_name: String,
    mut connection: &mut Connection,
    map: M,
    reduce: R,
    commit: &mut C,
) -> Result<()>
where
    M: (Fn(EntryId, HashMap<String, String>) -> Result<Option<MappedItem>>),
    R: (Fn(Vec<MappedItem>) -> Result<ReducedItem>),
    C: (FnMut(&mut Connection, bool, ReducedItem) -> Result<()>),
{
    let start_key = format!("{}_start", stream_name);

    let mut start = match connection.get::<&String, Option<String>>(&start_key)? {
        Some(start) => EntryId::from_str(start)?,
        None => EntryId::new(0, 0),
    };

    let mut initialized = false;

    loop {
        debug!(
            "Reading stream {} starting with entry {}",
            stream_name, start
        );
        let stream_entries = if initialized {
            read_stream(&mut connection, &stream_name, &start, Some(5000))?
        } else {
            read_stream(&mut connection, &stream_name, &start, None)?
        };

        let mut items = Vec::new();
        for stream_entry in stream_entries {
            let entry_id = stream_entry.0;
            let entry_values = stream_entry.1;

            if let Some(item) = map(entry_id.clone(), entry_values)? {
                items.push(item);
            }

            start = entry_id.next();
        }

        commit(&mut connection, initialized, reduce(items)?)?;

        connection.set(&start_key, start.to_string())?;

        initialized = true;
    }
}

fn is_nil(value: &Value) -> bool {
    match value {
        Value::Nil => true,
        _ => false,
    }
}

fn as_bulk(value: Value) -> Result<Vec<Value>> {
    match value {
        Value::Bulk(value) => Ok(value),
        _ => Err(format_err!(
            "Expected redis bulk value but got {:?}.",
            value
        )),
    }
}

fn as_entry_id(value: Value) -> Result<String> {
    match value {
        Value::Data(value) => Ok(String::from_utf8(value)?),
        Value::Status(value) => Ok(value),
        _ => Err(format_err!(
            "Expected redis status value but got {:?}.",
            value
        )),
    }
}

fn as_string(value: Value) -> Result<String> {
    match value {
        Value::Data(value) => Ok(String::from_utf8(value)?),
        _ => Err(format_err!(
            "Expected utf8 encoded redis data value but got {:?}.",
            value
        )),
    }
}

fn as_stream_entry(value: Value) -> Result<(EntryId, HashMap<String, String>)> {
    let mut stream_entry = as_bulk(value)?;
    assert(
        || stream_entry.len() == 2,
        format!("Invalid redis stream entry: {:?}", stream_entry),
    )?;

    let entry_id = EntryId::from_str(as_entry_id(stream_entry.remove(0))?)?;

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
