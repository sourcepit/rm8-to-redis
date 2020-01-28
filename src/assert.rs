use common_failures::prelude::*;

pub fn assert<E, S>(expr: E, err: S) -> Result<()>
where
    E: FnOnce() -> bool,
    S: Into<String>,
{
    match expr() {
        true => Ok(()),
        false => Err(format_err!("{}", err.into())),
    }
}