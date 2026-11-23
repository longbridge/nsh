//! The device's location (`nsh`'s `Location`): where it is, and whether the
//! app may read it.
//!
//! A platform answers [`LocationCommand`]s on the root with [`LocationEvent`]s
//! where [`Environment::location`](crate::platform::Environment::location)
//! says it reports one; an embedding app answers with a [`LocationBackend`]
//! instead ([`Core::set_location`](crate::Core::set_location),
//! `EngineConfig::location`), as it can for speech and the photo picker.
//!
//! What the app may read is the app's declaration (`nsh.json`
//! `"capabilities": { "location": true }` or
//! `{ "location": { "background": true } }`, [`crate::access`]), and the
//! system's own authorization on top of it: [`Authorization`] is both.

use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    task::Poll,
    time::Duration,
};

use crate::element::codes;

/// What the app may read, as the system says (TS: `Location.authorization()`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, crate::element::PropEnum)]
#[non_exhaustive]
pub enum Authorization {
    /// The user has not been asked yet: a call asks.
    #[default]
    NotDetermined,
    /// While the app is in front.
    WhenInUse,
    /// Also while it is not in front.
    Always,
    /// The user said no, or took back a yes in the system's settings.
    Denied,
    /// The system forbids it (a parental control, a managed device): the user
    /// cannot change it.
    Restricted,
}

impl Authorization {
    /// Every value, in its documented order.
    pub const ALL: [Self; 5] = [
        Self::NotDetermined,
        Self::WhenInUse,
        Self::Always,
        Self::Denied,
        Self::Restricted,
    ];

    /// Its `authorization()` name: `"notDetermined"`, `"whenInUse"`,
    /// `"always"`, `"denied"`, `"restricted"`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotDetermined => "notDetermined",
            Self::WhenInUse => "whenInUse",
            Self::Always => "always",
            Self::Denied => "denied",
            Self::Restricted => "restricted",
        }
    }

    /// The value `name` names.
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|value| value.as_str() == name)
    }
}

/// How precise a position to ask for (TS: `Location.watch`'s `accuracy`), in
/// Android's `LocationRequest` words, which each platform maps onto its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum LocationAccuracy {
    /// The most precise the device gives: the satellite providers, the fix
    /// costing the most battery.
    Best,
    /// Precise, at more battery than [`Self::Balanced`].
    High,
    /// The balance the platforms choose by default.
    #[default]
    Balanced,
    /// City-block: the network provider, or a fix coarsened.
    Low,
    /// Only a fix another app asked for (Android's passive provider).
    Passive,
}

impl LocationAccuracy {
    /// Every value, from the most precise.
    pub const ALL: [Self; 5] = [
        Self::Best,
        Self::High,
        Self::Balanced,
        Self::Low,
        Self::Passive,
    ];

    /// Its `accuracy` name: `"best"`, `"high"`, `"balanced"`, `"low"`,
    /// `"passive"`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Best => "best",
            Self::High => "high",
            Self::Balanced => "balanced",
            Self::Low => "low",
            Self::Passive => "passive",
        }
    }

    /// The value `name` names.
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|value| value.as_str() == name)
    }
}

/// What the app asked for, checked ([`Self::checked`]).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[non_exhaustive]
pub struct LocationRequest {
    /// How precise.
    pub accuracy: LocationAccuracy,
    /// Report a new position only after the device moved this many metres.
    /// `None`: as often as the platform reports one.
    pub distance_filter: Option<f64>,
    /// Report one at most this often. `None`: as often as the platform
    /// reports one.
    pub interval: Option<Duration>,
    /// Keep reporting while the app is not in front, which needs the app's
    /// declaration (`nsh.json` `"location": { "background": true }`) and a
    /// platform that does it ([`Environment::location_background`](crate::platform::Environment::location_background)).
    pub background: bool,
    /// `current` only: fail with [`ETIMEDOUT`](LocationError::ETIMEDOUT) when
    /// no position comes in this long. `None`: wait as long as the platform
    /// takes.
    pub timeout: Option<Duration>,
}

impl LocationRequest {
    /// The request, or why it is not one: what the app passes is checked
    /// before anything travels (a `RangeError` in TS).
    pub fn checked(self) -> Result<Self, String> {
        if let Some(distance) = self.distance_filter
            && !(distance.is_finite() && distance >= 0.0)
        {
            return Err(format!(
                "`distanceFilter` must be a number of metres, not {distance}"
            ));
        }
        if self.interval.is_some_and(|interval| interval.is_zero()) {
            return Err("`interval` must be longer than zero".into());
        }
        if self.timeout.is_some_and(|timeout| timeout.is_zero()) {
            return Err("`timeout` must be longer than zero".into());
        }
        Ok(self)
    }
}

/// A position the device reported.
///
/// Every field a platform does not report is `None` (TS `undefined`), never
/// zero: a device without a barometer knows no altitude, and one at rest
/// knows no heading.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct Position {
    /// Degrees north of the equator, -90 to 90.
    pub latitude: f64,
    /// Degrees east of Greenwich, -180 to 180.
    pub longitude: f64,
    /// How far the position may be from the device, in metres: the radius a
    /// map would draw around it. An approximate fix (Android's
    /// `ACCESS_COARSE_LOCATION`, the network provider) reports what it is.
    pub accuracy: f64,
    /// Metres above sea level.
    pub altitude: Option<f64>,
    /// How far [`Self::altitude`] may be off, in metres.
    pub altitude_accuracy: Option<f64>,
    /// Degrees clockwise from true north the device faces, 0 to 360; `None`
    /// when it is not moving.
    pub heading: Option<f64>,
    /// Metres per second.
    pub speed: Option<f64>,
    /// When the device took the fix, in milliseconds since the Unix epoch
    /// (what the DOM's `GeolocationPosition.timestamp` is).
    pub timestamp: f64,
}

impl Position {
    /// A fix of `latitude`, `longitude` and its `accuracy` in metres, taken
    /// at `timestamp` (milliseconds since the Unix epoch), with what else the
    /// device did not report left out.
    pub fn new(latitude: f64, longitude: f64, accuracy: f64, timestamp: f64) -> Self {
        Self {
            latitude,
            longitude,
            accuracy,
            altitude: None,
            altitude_accuracy: None,
            heading: None,
            speed: None,
            timestamp,
        }
    }

    /// The fix with its altitude, in metres.
    pub fn with_altitude(mut self, altitude: f64, accuracy: f64) -> Self {
        self.altitude = Some(altitude);
        self.altitude_accuracy = Some(accuracy);
        self
    }

    /// The fix with the heading the device faces, in degrees from north.
    pub fn with_heading(mut self, heading: f64) -> Self {
        self.heading = Some(heading);
        self
    }

    /// The fix with the device's speed, in metres per second.
    pub fn with_speed(mut self, speed: f64) -> Self {
        self.speed = Some(speed);
        self
    }
}

/// Why a position did not come.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct LocationError {
    /// A stable code the app branches on ([`Self::DENIED`],
    /// [`Self::UNAVAILABLE`], [`Self::ETIMEDOUT`]).
    pub code: &'static str,
    /// What went wrong, in words.
    pub message: String,
}

impl LocationError {
    /// The code of a location the user does not let the app read (Node's
    /// `EACCES`: the system refused).
    pub const DENIED: &'static str = "EACCES";

    /// The code of a location where the platform reports none.
    pub const UNAVAILABLE: &'static str = codes::UNAVAILABLE;

    /// The code of a `current` that found nothing within its timeout.
    pub const ETIMEDOUT: &'static str = "ETIMEDOUT";

    /// A failure that carries a code.
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    /// The user does not let the app read the location.
    pub fn denied(message: impl Into<String>) -> Self {
        Self::new(Self::DENIED, message)
    }

    /// The platform reports no location here.
    pub fn unavailable(message: impl Into<String>) -> Self {
        Self::new(Self::UNAVAILABLE, message)
    }
}

/// What the core asks of the platform (and of a [`LocationBackend`]).
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum LocationCommand {
    /// Report the current [`Authorization`] (the priming query the core sends
    /// once; later changes come through
    /// [`Core::set_location_authorization`](crate::Core::set_location_authorization)).
    Authorization,
    /// Ask the user, for when-in-use or (with `background`) for the location
    /// while the app is not in front.
    RequestAuthorization {
        /// The request the answer belongs to.
        id: u32,
        /// Ask for the location while the app is not in front.
        background: bool,
    },
    /// Report one position, then end.
    Current {
        /// The request it answers.
        id: u32,
        /// What was asked for.
        request: LocationRequest,
    },
    /// Report a position each time it changes, until [`Self::Stop`].
    Watch {
        /// The watch.
        id: u32,
        /// What was asked for.
        request: LocationRequest,
    },
    /// End watch `id`.
    Stop {
        /// The watch.
        id: u32,
    },
}

/// What the platform (or a [`LocationBackend`]) reports.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum LocationEvent {
    /// The position the request asked for; a watch reports one per change.
    Position {
        /// The request or watch it answers.
        id: u32,
        /// Where the device is.
        position: Position,
    },
    /// What the user answered (the answer to [`LocationCommand::RequestAuthorization`]).
    Authorized {
        /// The request it answers.
        id: u32,
        /// What the app may read now.
        authorization: Authorization,
    },
    /// The request failed: nothing more comes.
    Failed {
        /// The request or watch that failed.
        id: u32,
        /// Why.
        error: LocationError,
    },
    /// The watch ended (stopped, or no longer running): nothing more comes.
    Ended {
        /// The watch.
        id: u32,
    },
}

/// Reports the position in place of the platform: an embedding app's own
/// location provider, or a test's fake.
pub trait LocationBackend: Send + Sync {
    /// Whether it reports a location now (TS: `Location.available()`).
    fn available(&self) -> bool {
        true
    }

    /// What the app may read now.
    fn authorization(&self) -> Authorization;

    /// Asks the user, reporting the answer with
    /// [`LocationEvent::Authorized`] for `id` (or [`LocationEvent::Failed`]).
    fn request_authorization(&self, id: u32, background: bool, events: LocationSink);

    /// Reports one position with [`LocationEvent::Position`] for `id`.
    fn current(&self, id: u32, request: LocationRequest, events: LocationSink);

    /// Reports a position each time it changes, until the returned control
    /// stops it.
    fn watch(
        &self,
        id: u32,
        request: LocationRequest,
        events: LocationSink,
    ) -> Box<dyn LocationControl>;
}

/// A running watch of a [`LocationBackend`].
pub trait LocationControl {
    /// Stops reporting: the core already ended the watch.
    fn stop(&mut self);
}

/// Where a request's events go: the app (through the core), from any thread.
/// After [`LocationEvent::Failed`] or [`LocationEvent::Ended`] it takes
/// nothing more.
#[derive(Clone)]
pub struct LocationSink {
    channel: Arc<Mutex<Channel>>,
}

#[derive(Default)]
struct Channel {
    events: VecDeque<LocationEvent>,
    waker: Option<std::task::Waker>,
    ended: bool,
}

impl std::fmt::Debug for LocationSink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LocationSink")
            .field("ended", &self.is_ended())
            .finish()
    }
}

impl LocationSink {
    /// Reports `event`; `false` (and nothing reported) once the request ended.
    pub fn send(&self, event: LocationEvent) -> bool {
        let waker = {
            let mut channel = self.channel.lock().expect("location channel");
            if channel.ended {
                return false;
            }
            channel.ended = matches!(
                event,
                LocationEvent::Ended { .. } | LocationEvent::Failed { .. }
            );
            channel.events.push_back(event);
            channel.waker.take()
        };
        if let Some(waker) = waker {
            waker.wake();
        }
        true
    }

    /// Reports the position a request asked for.
    pub fn position(&self, id: u32, position: Position) -> bool {
        self.send(LocationEvent::Position { id, position })
    }

    /// Reports what the user answered.
    pub fn authorized(&self, id: u32, authorization: Authorization) -> bool {
        self.send(LocationEvent::Authorized { id, authorization })
    }

    /// Ends the request, with `error` when it failed.
    pub fn end(&self, id: u32, error: Option<LocationError>) -> bool {
        match error {
            Some(error) => self.send(LocationEvent::Failed { id, error }),
            None => self.send(LocationEvent::Ended { id }),
        }
    }

    /// Whether the request ended: nothing more is reported.
    pub fn is_ended(&self) -> bool {
        self.channel.lock().expect("location channel").ended
    }
}

/// The receiving end of a request's [`LocationSink`], read on the engine
/// thread.
pub(crate) struct LocationEvents {
    channel: Arc<Mutex<Channel>>,
}

impl LocationEvents {
    /// The next event, without waiting (`None` after the end was taken).
    #[cfg(test)]
    pub(crate) fn try_next(&mut self) -> Option<LocationEvent> {
        self.channel
            .lock()
            .expect("location channel")
            .events
            .pop_front()
    }

    /// The next event: `Pending` until one comes or the request ends.
    pub(crate) fn poll_next(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<LocationEvent>> {
        let mut channel = self.channel.lock().expect("location channel");
        if let Some(event) = channel.events.pop_front() {
            return Poll::Ready(Some(event));
        }
        if channel.ended {
            return Poll::Ready(None);
        }
        channel.waker = Some(cx.waker().clone());
        Poll::Pending
    }
}

/// A request's sink and the events it reports.
pub(crate) fn channel() -> (LocationSink, LocationEvents) {
    let channel = Arc::new(Mutex::new(Channel::default()));
    (
        LocationSink {
            channel: channel.clone(),
        },
        LocationEvents { channel },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip() {
        for authorization in Authorization::ALL {
            assert_eq!(
                Authorization::from_name(authorization.as_str()),
                Some(authorization),
                "{authorization:?}"
            );
        }
        for accuracy in LocationAccuracy::ALL {
            assert_eq!(
                LocationAccuracy::from_name(accuracy.as_str()),
                Some(accuracy),
                "{accuracy:?}"
            );
        }
        assert_eq!(Authorization::from_name("nope"), None);
        assert_eq!(LocationAccuracy::from_name("nope"), None);
        assert_eq!(LocationAccuracy::default(), LocationAccuracy::Balanced);
    }

    #[test]
    fn the_names_are_the_dom_and_platform_words() {
        assert_eq!(Authorization::NotDetermined.as_str(), "notDetermined");
        assert_eq!(Authorization::WhenInUse.as_str(), "whenInUse");
        assert_eq!(Authorization::Always.as_str(), "always");
        assert_eq!(Authorization::Denied.as_str(), "denied");
        assert_eq!(Authorization::Restricted.as_str(), "restricted");
        assert_eq!(LocationAccuracy::Best.as_str(), "best");
        assert_eq!(LocationAccuracy::Passive.as_str(), "passive");
    }

    #[test]
    fn a_request_is_checked_before_it_travels() {
        assert!(LocationRequest::default().checked().is_ok());

        for bad in [
            LocationRequest {
                distance_filter: Some(-1.0),
                ..LocationRequest::default()
            },
            LocationRequest {
                distance_filter: Some(f64::NAN),
                ..LocationRequest::default()
            },
            LocationRequest {
                interval: Some(Duration::ZERO),
                ..LocationRequest::default()
            },
            LocationRequest {
                timeout: Some(Duration::ZERO),
                ..LocationRequest::default()
            },
        ] {
            assert!(bad.checked().is_err(), "{bad:?}");
        }

        // Zero metres and a real interval are fine.
        assert!(
            LocationRequest {
                distance_filter: Some(0.0),
                interval: Some(Duration::from_millis(500)),
                timeout: Some(Duration::from_secs(5)),
                ..LocationRequest::default()
            }
            .checked()
            .is_ok()
        );
    }

    #[test]
    fn a_sink_takes_nothing_after_it_ends() {
        let (sink, mut events) = super::channel();
        let position = Position::new(1.0, 2.0, 3.0, 0.0);
        assert!(sink.send(LocationEvent::Ended { id: 1 }));
        assert!(!sink.send(LocationEvent::Position { id: 1, position }));
        assert!(matches!(
            events.try_next(),
            Some(LocationEvent::Ended { .. })
        ));
        assert_eq!(events.try_next(), None);
    }

    #[test]
    fn a_position_left_out_is_not_zero() {
        let position = Position::new(22.3, 114.2, 12.0, 1_700_000_000_000.0);
        assert_eq!(position.altitude, None);
        assert_eq!(position.heading, None);
        assert_eq!(position.speed, None);
        assert_eq!(
            position.with_altitude(30.0, 5.0).altitude_accuracy,
            Some(5.0)
        );
    }
}
