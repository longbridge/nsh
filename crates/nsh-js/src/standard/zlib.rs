//! Use the pinned LLRT compression API unchanged, including its optional
//! caller-supplied `maxOutputLength`; NSH adds no implicit output ceiling.
pub(crate) use llrt_zlib::ZlibModule;
