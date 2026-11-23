//! Location on Apple platforms (`Command::Location`, `nsh`'s `Location`):
//! `CLLocationManager` from the CoreLocation framework. Shared by macOS and
//! iOS (the iOS crate includes this file).
//!
//! The manager hears its fixes through a delegate; NSH reads
//! `CLLocationManager.location` on the main queue instead — the fix the
//! manager took last — so no delegate class is built at run time and nothing
//! has to be kept alive but the manager. `current` waits for the first fix
//! within its timeout and `watch` reports each new one. Authorization is the
//! system's own prompt (`requestWhenInUseAuthorization` /
//! `requestAlwaysAuthorization`), read back from
//! `CLLocationManager.authorizationStatus` until the user answers; the
//! packaged app explains it in `NSLocationWhenInUseUsageDescription`, which
//! `nsh package` writes for an app granted `location`.

use std::{
    cell::RefCell,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use dispatch2::DispatchQueue;
use nsh_core::{
    EventKind, EventPayload, ROOT_ID,
    location::{
        Authorization, LocationAccuracy, LocationCommand, LocationError, LocationEvent,
        LocationRequest, Position,
    },
};
use nsh_engine::EngineControl;
use objc2::rc::Retained;
use objc2_core_location::{CLAuthorizationStatus, CLLocation, CLLocationManager};

thread_local! {
    /// The manager, made once on the main thread: the system keeps the app's
    /// authorization with it.
    static MANAGER: RefCell<Option<Retained<CLLocationManager>>> = const { RefCell::new(None) };
    /// The request running, on the main thread.
    static RUNNING: RefCell<Option<Running>> = const { RefCell::new(None) };
    /// The authorization asked for, waiting for the user's answer.
    static ASKING: RefCell<Option<Asking>> = const { RefCell::new(None) };
}

/// A `current` or a `watch` running.
struct Running {
    id: u32,
    control: EngineControl,
    /// A watch reports every fix; a `current` the first, then ends.
    watch: bool,
    /// When a `current` gives up.
    deadline: Option<Instant>,
    /// The fix last reported: a watch skips one it already sent.
    reported: Option<f64>,
    /// Whether the system's refusal was reported.
    refused: bool,
}

/// An authorization the user is deciding.
struct Asking {
    id: u32,
    control: EngineControl,
}

/// Applies `command`, reporting through `control`. On the main thread.
pub fn command(control: &EngineControl, command: &LocationCommand) {
    match command {
        LocationCommand::Authorization => {
            let authorization = authorization(unsafe { manager().authorizationStatus() });
            control.set_location_authorization(authorization);
        }
        LocationCommand::RequestAuthorization { id, background } => ask(*id, control, *background),
        LocationCommand::Current { id, request } => start(*id, control, request, false),
        LocationCommand::Watch { id, request } => start(*id, control, request, true),
        LocationCommand::Stop { id } => stop(*id),
        _ => {}
    }
}

/// The manager, made once.
fn manager() -> Retained<CLLocationManager> {
    MANAGER.with(|slot| {
        slot.borrow_mut()
            // SAFETY: on the main thread (`command` and the polls run there),
            // which is where a `CLLocationManager` is made and messaged.
            .get_or_insert_with(|| unsafe { CLLocationManager::new() })
            .clone()
    })
}

/// What the system lets the app read.
fn authorization(status: CLAuthorizationStatus) -> Authorization {
    match status {
        CLAuthorizationStatus::Restricted => Authorization::Restricted,
        CLAuthorizationStatus::Denied => Authorization::Denied,
        CLAuthorizationStatus::AuthorizedAlways => Authorization::Always,
        CLAuthorizationStatus::AuthorizedWhenInUse => Authorization::WhenInUse,
        _ => Authorization::NotDetermined,
    }
}

/// The manager's precision for a request's.
fn desired_accuracy(accuracy: LocationAccuracy) -> f64 {
    match accuracy {
        // `kCLLocationAccuracyBest`.
        LocationAccuracy::Best => -1.0,
        // `kCLLocationAccuracyNearestTenMeters`.
        LocationAccuracy::High => 10.0,
        // `kCLLocationAccuracyHundredMeters`.
        LocationAccuracy::Balanced => 100.0,
        // `kCLLocationAccuracyKilometer`.
        LocationAccuracy::Low => 1000.0,
        // `kCLLocationAccuracyThreeKilometers`: Apple has no passive provider.
        LocationAccuracy::Passive => 3000.0,
        _ => 100.0,
    }
}

/// Asks the user, reporting the answer as `id`.
fn ask(id: u32, control: &EngineControl, background: bool) {
    let manager = manager();
    // SAFETY: on the main thread, the manager made there.
    unsafe {
        if background {
            manager.requestAlwaysAuthorization();
        } else {
            manager.requestWhenInUseAuthorization();
        }
    }
    let status = unsafe { manager.authorizationStatus() };
    if authorization(status) != Authorization::NotDetermined {
        control.set_location_authorization(authorization(status));
        report(
            control,
            id,
            LocationEvent::Authorized {
                id,
                authorization: authorization(status),
            },
        );
        return;
    }
    ASKING.with(|slot| {
        *slot.borrow_mut() = Some(Asking {
            id,
            control: control.clone(),
        });
    });
    after_authorization(Duration::from_millis(200));
}

/// Waits for the user's answer to an authorization.
fn poll_authorization() {
    let status = unsafe { manager().authorizationStatus() };
    let answered = authorization(status) != Authorization::NotDetermined;
    if !answered {
        after_authorization(Duration::from_millis(200));
        return;
    }
    let Some(asking) = ASKING.with(|slot| slot.borrow_mut().take()) else {
        return;
    };
    asking
        .control
        .set_location_authorization(authorization(status));
    report(
        &asking.control,
        asking.id,
        LocationEvent::Authorized {
            id: asking.id,
            authorization: authorization(status),
        },
    );
}

/// Starts a `current` or a watch.
fn start(id: u32, control: &EngineControl, request: &LocationRequest, watch: bool) {
    let manager = manager();
    // SAFETY: on the main thread, the manager made there.
    unsafe {
        manager.setDesiredAccuracy(desired_accuracy(request.accuracy));
        manager.setDistanceFilter(request.distance_filter.unwrap_or(-1.0));
        manager.startUpdatingLocation();
    }
    let deadline =
        (!watch).then(|| Instant::now() + request.timeout.unwrap_or(Duration::from_secs(10)));
    RUNNING.with(|slot| {
        *slot.borrow_mut() = Some(Running {
            id,
            control: control.clone(),
            watch,
            deadline,
            reported: None,
            refused: false,
        });
    });
    after(poll_interval(watch));
}

/// Stops request `id` if it is the one running.
fn stop(id: u32) {
    let stopped = RUNNING.with(|slot| {
        let running = slot.borrow();
        running.as_ref().is_some_and(|running| running.id == id)
    });
    if stopped {
        RUNNING.with(|slot| *slot.borrow_mut() = None);
        // SAFETY: on the main thread, the manager made there.
        unsafe { manager().stopUpdatingLocation() };
    }
}

fn poll_interval(watch: bool) -> Duration {
    Duration::from_millis(if watch { 500 } else { 250 })
}

/// When to run the next poll, on the main queue.
fn after(delay: Duration) {
    let nanos = delay.as_nanos().min(i64::MAX as u128) as i64;
    let when = dispatch2::DispatchTime::NOW.time(nanos);
    let _ = DispatchQueue::main().after(when, poll);
}

/// When to ask for the authorization's answer again.
fn after_authorization(delay: Duration) {
    let nanos = delay.as_nanos().min(i64::MAX as u128) as i64;
    let when = dispatch2::DispatchTime::NOW.time(nanos);
    let _ = DispatchQueue::main().after(when, poll_authorization);
}

/// Reads the fix the manager last took, and reports it.
fn poll() {
    let status = authorization(unsafe { manager().authorizationStatus() });
    let next = RUNNING.with(|slot| {
        let mut slot = slot.borrow_mut();
        let running = slot.as_mut()?;
        if matches!(status, Authorization::Denied | Authorization::Restricted) {
            if !running.refused {
                running.refused = true;
                report(
                    &running.control,
                    running.id,
                    LocationEvent::Failed {
                        id: running.id,
                        error: LocationError::denied(
                            "the user does not let the app read the device's location",
                        ),
                    },
                );
            }
            finish(&mut slot);
            return None;
        }
        if let Some(deadline) = running.deadline
            && Instant::now() >= deadline
        {
            report(
                &running.control,
                running.id,
                LocationEvent::Failed {
                    id: running.id,
                    error: LocationError::new(LocationError::ETIMEDOUT, "no position came in time"),
                },
            );
            finish(&mut slot);
            return None;
        }
        // SAFETY: on the main thread, the manager made there.
        if let Some(fix) = unsafe { manager().location() } {
            let position = unsafe { position(&fix) };
            if running.reported != Some(position.timestamp) {
                running.reported = Some(position.timestamp);
                report(
                    &running.control,
                    running.id,
                    LocationEvent::Position {
                        id: running.id,
                        position,
                    },
                );
                if !running.watch {
                    report(
                        &running.control,
                        running.id,
                        LocationEvent::Ended { id: running.id },
                    );
                    finish(&mut slot);
                    return None;
                }
            }
        }
        Some(poll_interval(running.watch))
    });
    if let Some(next) = next {
        after(next);
    }
}

/// Forgets the request running and stops the manager.
fn finish(slot: &mut std::cell::RefMut<'_, Option<Running>>) {
    if slot.take().is_some() {
        // SAFETY: on the main thread, the manager made there.
        unsafe { manager().stopUpdatingLocation() };
    }
}

/// What a fix says, as `nsh` gives it.
///
/// # Safety
///
/// `fix` must be a `CLLocation` from the manager, read on the main thread.
unsafe fn position(fix: &CLLocation) -> Position {
    let coordinate = unsafe { fix.coordinate() };
    let accuracy = unsafe { fix.horizontalAccuracy() };
    let taken = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_secs_f64() * 1000.0)
        .unwrap_or(0.0);
    let mut position = Position::new(
        coordinate.latitude,
        coordinate.longitude,
        accuracy.max(0.0),
        taken,
    );
    let vertical = unsafe { fix.verticalAccuracy() };
    if vertical >= 0.0 {
        position = position.with_altitude(unsafe { fix.altitude() }, vertical);
    }
    let speed = unsafe { fix.speed() };
    if speed >= 0.0 {
        position = position.with_speed(speed);
    }
    let course = unsafe { fix.course() };
    if course >= 0.0 {
        position = position.with_heading(course);
    }
    position
}

fn report(control: &EngineControl, id: u32, event: LocationEvent) {
    control.dispatch_event(
        ROOT_ID,
        EventKind::Location,
        EventPayload::Location { id, event },
    );
}
