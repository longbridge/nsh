//! Synchronous standard operations use the same Rust algorithms as the
//! native standard modules, without an interpreter dependency.
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    io::Read,
};

use hmac::Mac;
use nsh_core::{element::PropError, js::host::HostInput};
use sha2::Digest;

use super::values::Value;
#[derive(Default)]
pub(super) struct Standard {
    next: Cell<u32>,
    hashes: RefCell<HashMap<u32, HashState>>,
}
enum HashState {
    Md5(md5::Md5),
    HMd5(hmac::Hmac<md5::Md5>),
    Sha1(sha1::Sha1),
    HSha1(hmac::Hmac<sha1::Sha1>),
    Sha256(sha2::Sha256),
    HSha256(hmac::Hmac<sha2::Sha256>),
    Sha384(sha2::Sha384),
    HSha384(hmac::Hmac<sha2::Sha384>),
    Sha512(sha2::Sha512),
    HSha512(hmac::Hmac<sha2::Sha512>),
    Crc32(crc32fast::Hasher),
    Crc32c(u32),
}
impl HashState {
    fn update(&mut self, bytes: &[u8]) {
        match self {
            Self::Md5(hash) => Digest::update(hash, bytes),
            Self::HMd5(hash) => Mac::update(hash, bytes),
            Self::Sha1(hash) => Digest::update(hash, bytes),
            Self::HSha1(hash) => Mac::update(hash, bytes),
            Self::Sha256(hash) => Digest::update(hash, bytes),
            Self::HSha256(hash) => Mac::update(hash, bytes),
            Self::Sha384(hash) => Digest::update(hash, bytes),
            Self::HSha384(hash) => Mac::update(hash, bytes),
            Self::Sha512(hash) => Digest::update(hash, bytes),
            Self::HSha512(hash) => Mac::update(hash, bytes),
            Self::Crc32(hash) => hash.update(bytes),
            Self::Crc32c(crc) => *crc = crc32c::crc32c_append(*crc, bytes),
        }
    }
    fn finish(self) -> Vec<u8> {
        match self {
            Self::Md5(hash) => hash.finalize().to_vec(),
            Self::HMd5(hash) => hash.finalize().into_bytes().to_vec(),
            Self::Sha1(hash) => hash.finalize().to_vec(),
            Self::HSha1(hash) => hash.finalize().into_bytes().to_vec(),
            Self::Sha256(hash) => hash.finalize().to_vec(),
            Self::HSha256(hash) => hash.finalize().into_bytes().to_vec(),
            Self::Sha384(hash) => hash.finalize().to_vec(),
            Self::HSha384(hash) => hash.finalize().into_bytes().to_vec(),
            Self::Sha512(hash) => hash.finalize().to_vec(),
            Self::HSha512(hash) => hash.finalize().into_bytes().to_vec(),
            Self::Crc32(hash) => hash.finalize().to_be_bytes().to_vec(),
            Self::Crc32c(crc) => crc.to_be_bytes().to_vec(),
        }
    }
}
impl Standard {
    pub(super) fn call(&self, op: u32, args: &Value) -> Result<Value, PropError> {
        if matches!(op, 903..=905) {
            let id = args.index(0)?.as_number().unwrap_or(0.0) as u32;
            if op == 905 {
                self.hashes.borrow_mut().remove(&id);
                return Ok(Value(0));
            }
            if op == 903 {
                let bytes = args
                    .index(1)?
                    .read_bytes()
                    .ok_or_else(|| PropError("expected bytes".into()))?;
                if let Some(hash) = self.hashes.borrow_mut().get_mut(&id) {
                    hash.update(&bytes);
                }
                return Ok(Value(0));
            }
            let hash = self
                .hashes
                .borrow_mut()
                .remove(&id)
                .ok_or_else(|| PropError("Digest already called".into()))?;
            return Value::bytes(&hash.finish());
        }
        let name = args.index(0)?.as_string().unwrap_or_default();
        if op == 902 {
            let input = args.index(1)?.as_string().unwrap_or_default();
            return Value::string(&match name.as_str() {
                "domainToASCII" => url::quirks::domain_to_ascii(&input),
                "domainToUnicode" => url::quirks::domain_to_unicode(&input),
                _ => return Err(PropError("unknown URL operation".into())),
            });
        }
        if matches!(op, 900 | 906) {
            let key = args.index(1)?.read_bytes();
            let algorithm = name.to_ascii_uppercase().replace('-', "");
            let state = match (op, algorithm.as_str()) {
                (900, "MD5") => {
                    if let Some(key) = key {
                        HashState::HMd5(
                            hmac::Hmac::<md5::Md5>::new_from_slice(&key)
                                .map_err(|e| PropError(e.to_string()))?,
                        )
                    } else {
                        HashState::Md5(<md5::Md5>::new())
                    }
                }
                (900, "SHA1") => {
                    if let Some(key) = key {
                        HashState::HSha1(
                            hmac::Hmac::<sha1::Sha1>::new_from_slice(&key)
                                .map_err(|e| PropError(e.to_string()))?,
                        )
                    } else {
                        HashState::Sha1(<sha1::Sha1>::new())
                    }
                }
                (900, "SHA256") => {
                    if let Some(key) = key {
                        HashState::HSha256(
                            hmac::Hmac::<sha2::Sha256>::new_from_slice(&key)
                                .map_err(|e| PropError(e.to_string()))?,
                        )
                    } else {
                        HashState::Sha256(<sha2::Sha256>::new())
                    }
                }
                (900, "SHA384") => {
                    if let Some(key) = key {
                        HashState::HSha384(
                            hmac::Hmac::<sha2::Sha384>::new_from_slice(&key)
                                .map_err(|e| PropError(e.to_string()))?,
                        )
                    } else {
                        HashState::Sha384(<sha2::Sha384>::new())
                    }
                }
                (900, "SHA512") => {
                    if let Some(key) = key {
                        HashState::HSha512(
                            hmac::Hmac::<sha2::Sha512>::new_from_slice(&key)
                                .map_err(|e| PropError(e.to_string()))?,
                        )
                    } else {
                        HashState::Sha512(<sha2::Sha512>::new())
                    }
                }
                (906, "CRC32") => HashState::Crc32(crc32fast::Hasher::new()),
                (906, "CRC32C") => HashState::Crc32c(0),
                _ => return Err(PropError(format!("'{name}' not available"))),
            };
            let id = self
                .next
                .get()
                .checked_add(1)
                .ok_or_else(|| PropError("hash IDs exhausted".into()))?;
            self.next.set(id);
            self.hashes.borrow_mut().insert(id, state);
            return Ok(Value::number(id.into()));
        }
        let bytes = args
            .index(1)?
            .read_bytes()
            .ok_or_else(|| PropError("expected bytes".into()))?;
        if op != 901 {
            return Err(PropError("unknown standard operation".into()));
        }
        let level = args.index(2)?.as_number().unwrap_or(6.0) as i32;
        let limit = args
            .index(3)?
            .as_number()
            .unwrap_or((64 * 1024 * 1024) as f64) as usize;
        let limit = limit.min(64 * 1024 * 1024);
        let compression = flate2::Compression::new(level.max(0) as u32);
        let input = bytes.as_slice();
        let io = |e: std::io::Error| PropError(e.to_string());
        let reader: Box<dyn Read + '_> = match name.as_str() {
            "gzip" => Box::new(flate2::read::GzEncoder::new(input, compression)),
            "gunzip" => Box::new(flate2::read::GzDecoder::new(input)),
            "deflate" => Box::new(flate2::read::ZlibEncoder::new(input, compression)),
            "inflate" => Box::new(flate2::read::ZlibDecoder::new(input)),
            "deflateRaw" => Box::new(flate2::read::DeflateEncoder::new(input, compression)),
            "inflateRaw" => Box::new(flate2::read::DeflateDecoder::new(input)),
            "brotliCompress" => Box::new(brotli::CompressorReader::new(input, 8096, 11, 22)),
            "brotliDecompress" => Box::new(brotli::Decompressor::new(input, 8096)),
            "zstdCompress" => Box::new(zstd::stream::read::Encoder::new(input, level).map_err(io)?),
            "zstdDecompress" => Box::new(zstd::stream::read::Decoder::new(input).map_err(io)?),
            _ => return Err(PropError("unknown compression operation".into())),
        };
        let mut output = Vec::new();
        reader
            .take(limit as u64 + 1)
            .read_to_end(&mut output)
            .map_err(io)?;
        if output.len() > limit {
            return Err(PropError(format!("output exceeds {limit} bytes")));
        }
        Value::bytes(&output)
    }
}
