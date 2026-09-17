use std::ffi::OsString;

pub const FLAG: &str = "--version";

pub const TOGGLE_FLAG: &str = "--toggle-order";

#[derive(Debug, Clone, PartialEq)]
pub enum Request {
    Watch,
    Report,
    Toggle,
    Refuse(String),
}

pub fn requested(arguments: &[OsString]) -> Request {
    let shown: Vec<String> = arguments
        .iter()
        .map(|argument| argument.to_string_lossy().into_owned())
        .collect();
    match shown.as_slice() {
        [] => Request::Watch,
        [flag] if flag == FLAG => Request::Report,
        [flag] if flag == TOGGLE_FLAG => Request::Toggle,
        [flag, extra, ..] if flag == FLAG || flag == TOGGLE_FLAG => Request::Refuse(extra.clone()),
        [first, ..] => Request::Refuse(first.clone()),
    }
}
