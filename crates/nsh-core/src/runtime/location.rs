//! `nsh`'s `Location`: the device's location, and whether the app may read it
//! ([`crate::location`]).
//!
//! `available()` is whether the platform reports one, or the embedding app
//! gave the core its own provider; `authorization()` is what the app may read
//! now, as the system says. `current()` reads one position, `watch()` reports
//! each change, and both ask the user for when-in-use location themselves when
//! nobody has been asked yet.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    task::Poll,
    time::Duration,
};

use crate::{
    element::{CallCx, Callback, JsError, JsObject, Later, Stop, System, codes, module},
    location::{Authorization, LocationAccuracy, LocationError, LocationEvent, LocationRequest},
};

/// The device's location.
#[module]
pub(crate) struct Location;

#[module]
impl Location {
    /// Resolves with whether the device's location can be read here: the
    /// platform reports one, or the embedding app gave its own provider.
    pub(crate) fn available() -> Later<bool> {
        Later::new(async { Ok::<_, JsError>(crate::prelude::location_available().await) })
    }

    /// Resolves with what the app may read now, as the system says:
    /// `"notDetermined"`, `"whenInUse"`, `"always"`, `"denied"` or
    /// `"restricted"`.
    pub(crate) fn authorization() -> Later<Authorization> {
        Later::new(async { Ok::<_, JsError>(crate::prelude::location_authorization().await) })
    }

    /// Asks the user for the app's location, resolving with what they
    /// answered. `background: true` asks for the location while the app is not
    /// in front, which needs `"location": { "background": true }` and a
    /// platform that reports one there. Rejects with `ERR_ACCESS_DENIED` when
    /// the capability is not granted, and with `EACCES` when the user refused.
    pub(crate) fn request_authorization(
        options: Option<RequestAuthorizationOptions>,
        sys: System,
    ) -> Result<Later<Authorization>, JsError> {
        let background = options.unwrap_or_default().background.unwrap_or(false);
        if !sys.access.allows_location() {
            return Err(not_granted());
        }
        if background && !sys.access.allows_location_background() {
            return Err(background_not_granted());
        }
        Ok(Later::new(async move {
            let mut call = crate::prelude::request_location_authorization(background);
            match std::future::poll_fn(|cx| call.poll_next(cx)).await {
                Some(LocationEvent::Authorized { authorization, .. }) => Ok(authorization),
                Some(LocationEvent::Failed { error, .. }) => Err(js_error(error)),
                _ => Err(unavailable()),
            }
        }))
    }

    /// Reads the device's location once, resolving with it. Rejects with
    /// `EACCES` when the user does not let the app read it, with
    /// `ETIMEDOUT` when nothing came within `timeout`, and with
    /// `ERR_FEATURE_UNAVAILABLE_ON_PLATFORM` where no platform reports one.
    pub(crate) fn current(
        options: Option<CurrentOptions>,
        sys: System,
    ) -> Result<Later<Position>, JsError> {
        let request = options.unwrap_or_default().request()?;
        if !sys.access.allows_location() {
            return Err(not_granted());
        }
        Ok(Later::new(async move {
            current_request(request).await.map(Position::from)
        }))
    }

    /// Reports the device's location each time it changes, until the returned
    /// function stops it. `background: true` keeps reporting while the app is
    /// not in front, which needs `"location": { "background": true }` and a
    /// platform that reports one there (`ERR_ACCESS_DENIED` and
    /// `ERR_FEATURE_UNAVAILABLE_ON_PLATFORM` otherwise). One watch runs at a
    /// time: a second stops the first, which ends with `Ended`.
    pub(crate) fn watch(
        options: Option<WatchOptions>,
        #[args(listener)] listener: Callback<Position>,
        sys: System,
        cx: CallCx<'_>,
    ) -> Result<Stop, JsError> {
        let request = options.unwrap_or_default().request()?;
        if !sys.access.allows_location() {
            return Err(not_granted());
        }
        if request.background && !sys.access.allows_location_background() {
            return Err(background_not_granted());
        }
        // The watch starts in its task: a JS call that resumes after an
        // `await` has no reactive graph entered, and starting a request needs
        // the engine's executor.
        let call: Rc<RefCell<Option<crate::prelude::LocationCall>>> = Rc::new(RefCell::new(None));
        let stopped = Rc::new(Cell::new(false));
        cx.start(deliver(
            Rc::downgrade(&call),
            Rc::clone(&stopped),
            request,
            listener,
        ));
        Ok(Stop::new(move || {
            stopped.set(true);
            if let Ok(mut call) = call.try_borrow_mut()
                && let Some(call) = call.as_mut()
            {
                call.stop();
            }
        }))
    }
}

/// The device's position, as `current` resolves and `watch` reports (the DOM's
/// `GeolocationPosition`).
#[derive(Debug, Clone, PartialEq, JsObject)]
pub(crate) struct Position {
    coords: Coordinates,
    timestamp: f64,
}

/// A position's coordinates (the DOM's `GeolocationCoordinates`).
#[derive(Debug, Clone, PartialEq, JsObject)]
pub(crate) struct Coordinates {
    latitude: f64,
    longitude: f64,
    accuracy: f64,
    altitude: Option<f64>,
    altitude_accuracy: Option<f64>,
    heading: Option<f64>,
    speed: Option<f64>,
}

impl From<crate::location::Position> for Position {
    fn from(position: crate::location::Position) -> Self {
        Self {
            coords: Coordinates {
                latitude: position.latitude,
                longitude: position.longitude,
                accuracy: position.accuracy,
                altitude: position.altitude,
                altitude_accuracy: position.altitude_accuracy,
                heading: position.heading,
                speed: position.speed,
            },
            timestamp: position.timestamp,
        }
    }
}

/// `Location.current`'s options.
#[derive(Debug, Clone, Default, PartialEq, JsObject)]
#[prop(exact)]
pub(crate) struct CurrentOptions {
    /// `"best"`, `"high"`, `"balanced"` (the default), `"low"` or `"passive"`.
    accuracy: Option<String>,
    /// How long to wait for a position, in milliseconds; as long as the
    /// platform takes when not given.
    timeout: Option<f64>,
}

impl CurrentOptions {
    fn request(self) -> Result<LocationRequest, JsError> {
        LocationRequest {
            accuracy: accuracy(self.accuracy)?,
            timeout: millis("timeout", self.timeout)?,
            ..LocationRequest::default()
        }
        .checked()
        .map_err(JsError::Range)
    }
}

/// `Location.watch`'s options.
#[derive(Debug, Clone, Default, PartialEq, JsObject)]
#[prop(exact)]
pub(crate) struct WatchOptions {
    /// `"best"`, `"high"`, `"balanced"` (the default), `"low"` or `"passive"`.
    accuracy: Option<String>,
    /// Report a new position only after the device moved this many metres.
    distance_filter: Option<f64>,
    /// Report one at most this often, in milliseconds.
    interval: Option<f64>,
    /// Keep reporting while the app is not in front.
    background: Option<bool>,
}

impl WatchOptions {
    fn request(self) -> Result<LocationRequest, JsError> {
        if let Some(distance) = self.distance_filter
            && !(distance.is_finite() && distance >= 0.0)
        {
            return Err(JsError::Range(format!(
                "`distanceFilter` must be a number of metres, not {distance}"
            )));
        }
        LocationRequest {
            accuracy: accuracy(self.accuracy)?,
            distance_filter: self.distance_filter,
            interval: millis("interval", self.interval)?,
            background: self.background.unwrap_or(false),
            ..LocationRequest::default()
        }
        .checked()
        .map_err(JsError::Range)
    }
}

/// `Location.requestAuthorization`'s options.
#[derive(Debug, Clone, Default, PartialEq, JsObject)]
#[prop(exact)]
pub(crate) struct RequestAuthorizationOptions {
    /// Ask for the location while the app is not in front.
    background: Option<bool>,
}

/// A position's `accuracy` name to its value.
fn accuracy(name: Option<String>) -> Result<LocationAccuracy, JsError> {
    match name {
        None => Ok(LocationAccuracy::default()),
        Some(name) => LocationAccuracy::from_name(&name).ok_or_else(|| {
            JsError::Range(format!(
                "`accuracy` must be \"best\", \"high\", \"balanced\", \"low\" or \"passive\", not {name:?}"
            ))
        }),
    }
}

/// Milliseconds to a duration.
fn millis(name: &str, value: Option<f64>) -> Result<Option<Duration>, JsError> {
    match value {
        None => Ok(None),
        Some(value) if value.is_finite() && value >= 0.0 => {
            Ok(Some(Duration::from_millis(value as u64)))
        }
        Some(_) => Err(JsError::Range(format!(
            "`{name}` must be milliseconds, not a negative or infinite number"
        ))),
    }
}

/// One position, or why none came.
async fn current_request(request: LocationRequest) -> Result<crate::location::Position, JsError> {
    let mut call = crate::prelude::current_location(request);
    let event = match request.timeout {
        None => std::future::poll_fn(|cx| call.poll_next(cx)).await,
        Some(timeout) => {
            let mut timer = std::pin::pin!(crate::prelude::timer(timeout));
            let mut expired = false;
            let event = std::future::poll_fn(|cx| {
                if let Poll::Ready(event) = call.poll_next(cx) {
                    return Poll::Ready(event);
                }
                if std::future::Future::poll(timer.as_mut(), cx).is_ready() {
                    expired = true;
                    return Poll::Ready(None);
                }
                Poll::Pending
            })
            .await;
            if expired {
                return Err(JsError::Error("no position came in time".into())
                    .with_code(LocationError::ETIMEDOUT));
            }
            event
        }
    };
    match event {
        Some(LocationEvent::Position { position, .. }) => Ok(position),
        Some(LocationEvent::Failed { error, .. }) => Err(js_error(error)),
        _ => Err(unavailable()),
    }
}

/// Starts a watch in its task and reports its positions to `listener` until
/// it ends or `stopped` is set.
async fn deliver(
    slot: std::rc::Weak<RefCell<Option<crate::prelude::LocationCall>>>,
    stopped: Rc<Cell<bool>>,
    request: LocationRequest,
    listener: Callback<Position>,
) {
    let Some(shared) = slot.upgrade() else {
        return;
    };
    if stopped.get() {
        return;
    }
    *shared.borrow_mut() = Some(crate::prelude::watch_location(request));
    loop {
        if stopped.get() {
            break;
        }
        let event = std::future::poll_fn(|cx| match shared.try_borrow_mut() {
            Ok(mut call) => match call.as_mut() {
                Some(call) => call.poll_next(cx),
                None => Poll::Ready(None),
            },
            Err(_) => Poll::Pending,
        })
        .await;
        let Some(event) = event else {
            break;
        };
        if let LocationEvent::Position { position, .. } = event {
            listener.post(Position::from(position));
        }
    }
    if let Some(mut call) = shared.borrow_mut().take() {
        call.stop();
    }
}

/// How a location failure reaches the app: an `Error` with its code.
fn js_error(error: LocationError) -> JsError {
    JsError::Error(error.message).with_code(error.code)
}

/// The capability that grants the device's location is not declared.
fn not_granted() -> JsError {
    JsError::access_denied(
        "the device's location is not granted: add `\"location\": true` to capabilities in nsh.json",
        "location",
        "",
    )
}

/// The capability that grants it in the background is not declared.
fn background_not_granted() -> JsError {
    JsError::access_denied(
        "background location is not granted: add `\"location\": { \"background\": true }` to \
         capabilities in nsh.json",
        "location",
        "background",
    )
}

/// No platform reports a position here.
fn unavailable() -> JsError {
    JsError::Error("this platform reports no location".into()).with_code(codes::UNAVAILABLE)
}
