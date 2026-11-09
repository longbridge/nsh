//! `nsh`'s `HostStream`: a live stream the embedding app registered by
//! name, which `<Video srcObject>` plays ([`crate::element::HostStream`]).
//! It holds only the name: the embedding app owns the stream, its frames
//! never reach JS. A name `capabilities.streams` does not grant throws
//! `ERR_ACCESS_DENIED` (checked here, in the call; the core also refuses
//! the streams of Rust components, which then play nothing).

use std::{rc::Rc, sync::Arc};

use crate::element::{ClassInstance, Instance, JsError, System, class};

/// A live stream (a call's video, a camera) the embedding app registered,
/// for a video's `srcObject`.
#[class]
pub(crate) struct HostStream {
    stream: crate::element::HostStream,
}

#[class]
impl HostStream {
    /// The stream the embedding app registered as `name` (on the web, the
    /// page with `player.registerStream`). A name nothing registered shows
    /// nothing (logged once). A name that is empty or longer than 256 bytes
    /// is a `TypeError`; one the `streams` capability does not grant throws
    /// `ERR_ACCESS_DENIED` (`permission` `"streams"`, `resource` the name).
    pub(crate) fn from_host(name: String, sys: System) -> Result<HostStream, JsError> {
        crate::element::HostStream::checked(&name).map_err(JsError::Type)?;
        if !sys.access.allows_stream(&name) {
            return Err(JsError::access_denied(
                crate::core::stream_not_granted(&name),
                "streams",
                name,
            ));
        }
        Ok(HostStream {
            stream: crate::element::HostStream::from_host(name),
        })
    }

    /// The name the embedding app registered the stream with.
    #[getter]
    pub(crate) fn name(&self) -> String {
        self.stream.name().to_owned()
    }
}

/// The stream's name, when `instance` is a `HostStream`.
pub(crate) fn name_of(instance: Rc<dyn ClassInstance>) -> Option<Arc<str>> {
    Instance::<HostStream>::of(instance).map(|stream| stream.with(|s| s.stream.name_arc()))
}
