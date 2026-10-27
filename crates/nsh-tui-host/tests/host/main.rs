//! Standalone host acceptance through its bundle reader, event conversion and loop.

mod counter;

mod dev;

mod text_input;

mod performance;

#[cfg(unix)]
mod lifecycle;
