use std::sync::Mutex;

use super::*;

/// The module's state is process-wide: the tests that touch it take turns.
static HUB_TESTS: Mutex<()> = Mutex::new(());

fn turn() -> MutexGuard<'static, ()> {
    HUB_TESTS.lock().unwrap_or_else(|e| e.into_inner())
}

/// Polls `future`, which must finish without waiting.
#[cfg(not(target_os = "ios"))]
fn ready<T>(future: impl std::future::Future<Output = T>) -> T {
    use std::task::{Context, Poll, Waker};
    let mut future = std::pin::pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("the future waited"),
    }
}

fn payload(chat: &str) -> JsonValue {
    let mut aps = nsh_core::JsonValue::Object(Default::default());
    aps["thread-id"] = JsonValue::String(format!("chat-{chat}"));
    let mut payload = JsonValue::Object(Default::default());
    payload["aps"] = aps;
    payload["chat"] = JsonValue::String(chat.into());
    payload
}

fn press(id: &str) -> Delivery {
    Delivery::Press(RemoteNotification::new(id, "", JsonValue::Null))
}

/// What `receiver` holds now, as `(kind, id)` pairs.
fn received(receiver: &mut mpsc::UnboundedReceiver<Delivery>) -> Vec<(Kind, String)> {
    let mut out = Vec::new();
    while let Ok(delivery) = receiver.try_recv() {
        let id = match &delivery {
            Delivery::Token(token) => token.to_hex(),
            Delivery::Notification(n) | Delivery::Press(n) | Delivery::Background(n, _) => {
                n.id.clone()
            }
        };
        out.push((delivery.kind(), id));
    }
    out
}

/// A completion that records the result the system would hear.
fn recording() -> (BackgroundCompletion, Arc<Mutex<Vec<BackgroundResult>>>) {
    let heard = Arc::new(Mutex::new(Vec::new()));
    let record = heard.clone();
    let completion = BackgroundCompletion::new(move |result| record.lock().unwrap().push(result));
    (completion, heard)
}

#[test]
fn tokens_are_hexadecimal() {
    let token = DeviceToken::new([0x00, 0x0a, 0xff, 0x10]);
    assert_eq!(token.to_hex(), "000aff10");
    assert_eq!(token.to_string(), "000aff10");
    assert_eq!(token.as_bytes(), [0x00, 0x0a, 0xff, 0x10]);
    assert_eq!(DeviceToken::new(Vec::new()).to_hex(), "");
}

#[test]
fn a_background_delivery_takes_its_thread_from_the_payload() {
    let delivery = RemoteNotification::background(payload("42"));
    assert_eq!(delivery.id, "");
    assert_eq!(delivery.thread_id, "chat-42");
    assert_eq!(delivery.payload["chat"], "42");
    let none = RemoteNotification::background(JsonValue::Object(Default::default()));
    assert_eq!(none.thread_id, "");
}

#[test]
fn presentation_options_leave_out_what_shows() {
    assert_eq!(
        PushPresentationOptions::default().presentation(),
        ForegroundPresentation::SHOWN
    );
    let quiet = PushPresentationOptions {
        banner: Some(false),
        sound: Some(false),
        ..Default::default()
    }
    .presentation();
    assert_eq!(
        quiet,
        ForegroundPresentation::default().banner(false).sound(false)
    );
    assert!(quiet.list && quiet.badge);
    assert_eq!(
        PushPresentation::from(quiet),
        PushPresentation {
            banner: false,
            list: true,
            sound: false,
            badge: true,
        }
    );
}

#[test]
fn errors_keep_their_codes() {
    assert_eq!(
        PushError::unavailable("no").to_js().code(),
        Some("ERR_FEATURE_UNAVAILABLE_ON_PLATFORM")
    );
    assert_eq!(PushError::failed("refused").to_js().code(), None);
}

#[test]
fn results_combine_by_significance() {
    use BackgroundResult::*;
    assert_eq!(NoData.merge(NoData), NoData);
    assert_eq!(NoData.merge(Failed), Failed);
    assert_eq!(Failed.merge(NoData), Failed);
    assert_eq!(Failed.merge(NewData), NewData);
    assert_eq!(NewData.merge(Failed), NewData);
    assert_eq!(BackgroundResult::default(), NoData);
}

#[test]
fn the_system_hears_once_when_the_last_clone_goes() {
    let (completion, heard) = recording();
    let kept = completion.clone();
    completion.set_result(BackgroundResult::Failed);
    drop(completion);
    assert!(heard.lock().unwrap().is_empty());
    kept.set_result(BackgroundResult::NewData);
    drop(kept);
    assert_eq!(*heard.lock().unwrap(), [BackgroundResult::NewData]);

    // Untouched: nothing new.
    let (completion, heard) = recording();
    drop(completion);
    assert_eq!(*heard.lock().unwrap(), [BackgroundResult::NoData]);
}

#[test]
fn threads_are_filtered_by_identifier() {
    let delivered = [("a", "chat-1"), ("b", "chat-2"), ("c", "chat-1"), ("d", "")];
    assert_eq!(in_thread(delivered, "chat-1"), ["a", "c"]);
    assert_eq!(in_thread(delivered, ""), ["d"]);
    assert!(in_thread(delivered, "chat-3").is_empty());
}

#[test]
fn presses_wait_for_the_first_listener_in_order() {
    let mut hub = Hub::new();
    let at = Instant::now();
    // The press that launched the app comes before any JS.
    assert!(hub.dispatch(press("first"), at).is_empty());
    assert!(hub.dispatch(press("second"), at).is_empty());
    let (id, mut receiver) = hub.subscribe();
    // Listening to something else takes none of them.
    hub.listen(id, Kind::Token);
    assert!(received(&mut receiver).is_empty());
    hub.listen(id, Kind::Press);
    assert_eq!(
        received(&mut receiver),
        [
            (Kind::Press, "first".to_owned()),
            (Kind::Press, "second".to_owned())
        ]
    );
    assert!(hub.pending.is_empty());
    // Listened to: straight there.
    assert!(hub.dispatch(press("third"), at).is_empty());
    assert_eq!(received(&mut receiver), [(Kind::Press, "third".to_owned())]);
}

#[test]
fn at_most_sixteen_presses_wait() {
    let mut hub = Hub::new();
    let at = Instant::now();
    for i in 0..MAX_PENDING_PRESSES {
        assert!(hub.dispatch(press(&i.to_string()), at).is_empty());
    }
    let dropped = hub.dispatch(press("last"), at);
    assert!(matches!(&dropped[..], [Delivery::Press(n)] if n.id == "0"));
    let (id, mut receiver) = hub.subscribe();
    hub.listen(id, Kind::Press);
    let ids = received(&mut receiver);
    assert_eq!(ids.len(), MAX_PENDING_PRESSES);
    assert_eq!(ids[0].1, "1");
    assert_eq!(ids[MAX_PENDING_PRESSES - 1].1, "last");
}

#[test]
fn notifications_in_front_do_not_wait() {
    let mut hub = Hub::new();
    let notification = Delivery::Notification(RemoteNotification::new("n", "", JsonValue::Null));
    assert_eq!(hub.dispatch(notification, Instant::now()).len(), 1);
    assert!(hub.pending.is_empty());
}

#[test]
fn a_background_delivery_waits_a_while_for_a_listener() {
    let mut hub = Hub::new();
    let at = Instant::now();
    let (waited, heard_waited) = recording();
    let (expired, heard_expired) = recording();
    let early = Delivery::Background(
        RemoteNotification::new("early", "", JsonValue::Null),
        expired,
    );
    let late = Delivery::Background(RemoteNotification::new("late", "", JsonValue::Null), waited);
    assert!(hub.dispatch(early, at).is_empty());
    assert!(hub.dispatch(late, at + Duration::from_secs(3)).is_empty());

    // The wait passes for the first one only: the system hears it now.
    let gone = hub.expire(at + BACKGROUND_WAIT);
    assert_eq!(gone.len(), 1);
    drop(gone);
    assert_eq!(*heard_expired.lock().unwrap(), [BackgroundResult::NoData]);

    // A listener takes the other one, whose completion it reports.
    let (id, mut receiver) = hub.subscribe();
    hub.listen(id, Kind::Background);
    let Ok(Delivery::Background(n, completion)) = receiver.try_recv() else {
        panic!("the delivery that waited was not handed over");
    };
    assert_eq!(n.id, "late");
    assert!(heard_waited.lock().unwrap().is_empty());
    completion.set_result(BackgroundResult::NewData);
    drop(completion);
    assert_eq!(*heard_waited.lock().unwrap(), [BackgroundResult::NewData]);
}

#[test]
fn every_listening_vm_shares_a_background_completion() {
    let mut hub = Hub::new();
    let (a, mut to_a) = hub.subscribe();
    let (b, mut to_b) = hub.subscribe();
    hub.listen(a, Kind::Background);
    hub.listen(b, Kind::Background);
    let (completion, heard) = recording();
    let delivery = Delivery::Background(RemoteNotification::default(), completion);
    // Handed to both; the hub keeps no copy.
    let kept = hub.dispatch(delivery, Instant::now());
    drop(kept);
    let first = to_a.try_recv().unwrap();
    let second = to_b.try_recv().unwrap();
    drop(first);
    assert!(heard.lock().unwrap().is_empty());
    drop(second);
    assert_eq!(*heard.lock().unwrap(), [BackgroundResult::NoData]);
}

#[test]
fn only_vms_listening_to_a_kind_get_it() {
    let mut hub = Hub::new();
    let (tokens, mut to_tokens) = hub.subscribe();
    let (presses, mut to_presses) = hub.subscribe();
    hub.listen(tokens, Kind::Token);
    hub.listen(presses, Kind::Press);
    drop(hub.dispatch(press("p"), Instant::now()));
    assert!(received(&mut to_tokens).is_empty());
    assert_eq!(received(&mut to_presses).len(), 1);

    // Stopped listening: it waits again.
    hub.unlisten(presses, Kind::Press);
    assert!(hub.dispatch(press("q"), Instant::now()).is_empty());
    assert_eq!(hub.pending.len(), 1);

    // A VM that went is forgotten, and what it would get waits.
    hub.listen(presses, Kind::Press);
    assert_eq!(received(&mut to_presses).len(), 1);
    drop(to_presses);
    let dropped = hub.dispatch(press("r"), Instant::now());
    assert_eq!(dropped.len(), 1);
    assert_eq!(hub.pending.len(), 1);
    assert!(hub.subscribers.iter().all(|s| s.id != presses));
    hub.unsubscribe(tokens);
    assert!(hub.subscribers.is_empty());
}

#[test]
fn a_token_is_reported_when_it_changes() {
    let _turn = turn();
    let mut hub = Hub::new();
    let (id, mut receiver) = hub.subscribe();
    hub.listen(id, Kind::Token);
    let (answer, registration) = oneshot::channel();
    hub.registering.push(answer);
    let token = DeviceToken::new([1, 2]);
    let answers = hub.registered(&token);
    assert_eq!(answers.len(), 1);
    drop(answers);
    drop(registration);
    assert_eq!(received(&mut receiver), [(Kind::Token, "0102".to_owned())]);
    // The same token again: registrations are answered, listeners hear
    // nothing new.
    drop(hub.registered(&token));
    assert!(received(&mut receiver).is_empty());
    drop(hub.registered(&DeviceToken::new([3])));
    assert_eq!(received(&mut receiver), [(Kind::Token, "03".to_owned())]);
    assert_eq!(hub.token, Some(DeviceToken::new([3])));
}

#[test]
fn rust_observers_hear_every_event() {
    let _turn = turn();
    let heard = Arc::new(Mutex::new(Vec::new()));
    let record = heard.clone();
    on_event(move |event| {
        let what = match event {
            PushEvent::Token(token) => format!("token {token}"),
            PushEvent::RegistrationFailed(error) => format!("failed {error}"),
            PushEvent::Notification(n) => format!("notification {}", n.id),
            PushEvent::Background {
                notification,
                completion,
            } => {
                completion.set_result(BackgroundResult::NewData);
                format!("background {}", notification.thread_id)
            }
            PushEvent::Press(n) => format!("press {}", n.id),
        };
        record.lock().unwrap().push(what);
    });
    registered(DeviceToken::new([0xab]));
    registered(DeviceToken::new([0xab]));
    registration_failed("no entitlement");
    let shown = arrived(RemoteNotification::new("n1", "t", JsonValue::Null));
    assert_eq!(shown, foreground_presentation());
    let (completion, done) = recording();
    background(RemoteNotification::background(payload("7")), completion);
    pressed(RemoteNotification::new("n2", "t", JsonValue::Null));
    let heard = heard.lock().unwrap().clone();
    assert_eq!(
        heard,
        [
            "token ab",
            "token ab",
            "failed the device was not registered: no entitlement",
            "notification n1",
            "background chat-7",
            "press n2",
        ]
    );
    assert_eq!(device_token(), Some(DeviceToken::new([0xab])));
    // The pending ones go with the wait; the observer's result stays.
    let pending: Vec<_> = std::mem::take(&mut hub().pending);
    drop(pending);
    assert_eq!(*done.lock().unwrap(), [BackgroundResult::NewData]);
}

#[test]
fn the_foreground_presentation_is_set_before_notifications_come() {
    let _turn = turn();
    set_foreground_presentation(ForegroundPresentation::SHOWN.banner(false));
    let shown = arrived(RemoteNotification::new("quiet", "", JsonValue::Null));
    set_foreground_presentation(ForegroundPresentation::default());
    assert!(!shown.banner);
    assert!(shown.list && shown.sound && shown.badge);
    assert_eq!(foreground_presentation(), ForegroundPresentation::SHOWN);
}

#[cfg(not(target_os = "ios"))]
#[test]
fn there_are_no_remote_notifications_off_ios() {
    let _turn = turn();
    assert!(!ready(available()));
    let error = ready(register()).unwrap_err();
    assert_eq!(error.code.as_deref(), Some(UNAVAILABLE));
    let error = ready(close_thread("chat-1".into())).unwrap_err();
    assert_eq!(error.code.as_deref(), Some(UNAVAILABLE));
    // Setting up does nothing, and does not panic.
    install();
}

#[test]
fn the_set_declares_push() {
    let ts = nsh_core::element::declarations(&crate::registry(), &[]);
    for declared in [
        "Push",
        "register(",
        "token(",
        "closeThread(",
        "setForegroundPresentation(",
        "foregroundPresentation(",
        "PushPresentationOptions",
        "PushNotification",
        "PushBackground",
        "\"newData\"",
        "Promise<void |",
        "available(",
    ] {
        assert!(ts.contains(declared), "{declared} is not declared:\n{ts}");
    }
}
