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
    use std::task::{Context, Waker};
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
    let mut aps = JsonValue::Object(Default::default());
    aps["thread-id"] = JsonValue::String(format!("chat-{chat}"));
    let mut payload = JsonValue::Object(Default::default());
    payload["aps"] = aps;
    payload["chat"] = JsonValue::String(chat.into());
    payload
}

fn press(id: &str) -> Delivery {
    Delivery::Press(RemoteNotification::new(id, "", JsonValue::Null))
}

fn background_at(id: &str, completion: BackgroundCompletion, at: Instant) -> Delivery {
    Delivery::Background(
        RemoteNotification::new(id, "", JsonValue::Null),
        completion,
        at,
    )
}

/// What `receiver` holds now, as `(kind, id)` pairs.
fn received(receiver: &mut mpsc::UnboundedReceiver<Delivery>) -> Vec<(Kind, String)> {
    let mut out = Vec::new();
    while let Ok(delivery) = receiver.try_recv() {
        let id = match &delivery {
            Delivery::Token(token) => token.to_string(),
            Delivery::Notification(n) | Delivery::Press(n) | Delivery::Background(n, ..) => {
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
fn ios_tokens_are_the_bytes_in_hexadecimal() {
    let token = DeviceToken::from_bytes([0x00, 0x0a, 0xff, 0x10]);
    assert_eq!(token.as_str(), "000aff10");
    assert_eq!(token.to_string(), "000aff10");
    assert_eq!(token.as_bytes(), [0x00, 0x0a, 0xff, 0x10]);
    assert_eq!(DeviceToken::from_bytes(Vec::new()).as_str(), "");
    assert_eq!(
        PushToken::from(&token),
        PushToken {
            token: "000aff10".into()
        }
    );
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
        ForegroundPresentationOptions::default().presentation(),
        ForegroundPresentation::SHOWN
    );
    let quiet = ForegroundPresentationOptions {
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
}

#[test]
fn a_thread_is_named_by_a_non_empty_id() {
    let thread = |id: &str| ForegroundPresentationThread {
        thread_id: id.into(),
    };
    assert_eq!(thread("chat-1").id().unwrap(), "chat-1");
    assert!(matches!(thread("").id(), Err(JsError::Range(_))));
}

#[test]
fn threads_can_show_their_own_way() {
    let mut hub = Hub::new();
    let quiet = ForegroundPresentation::SHOWN.banner(false).sound(false);
    hub.set_thread_presentation("chat-1".into(), Some(quiet));
    assert_eq!(hub.presentation_of("chat-1"), quiet);
    assert_eq!(hub.presentation_of("chat-2"), ForegroundPresentation::SHOWN);
    // An empty thread is no thread: the app's presentation.
    hub.set_thread_presentation(String::new(), Some(quiet));
    assert_eq!(hub.presentation_of(""), ForegroundPresentation::SHOWN);
    // The app's presentation does not change the thread's.
    hub.presentation = ForegroundPresentation::SHOWN.list(false);
    assert_eq!(hub.presentation_of("chat-1"), quiet);
    assert_eq!(
        hub.presentation_of("chat-2"),
        ForegroundPresentation::SHOWN.list(false)
    );
    // Given back to the app's.
    hub.set_thread_presentation("chat-1".into(), None);
    assert_eq!(
        hub.presentation_of("chat-1"),
        ForegroundPresentation::SHOWN.list(false)
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
fn a_thread_closes_its_own_notifications_only() {
    let delivered = [
        ("a", "chat-1", false),
        ("b", "chat-2", false),
        ("c", "chat-1", false),
        // Shown by `Notifications` in the same thread: left to it.
        ("d", "chat-1", true),
        ("e", "", false),
        ("f", "", true),
    ];
    assert_eq!(in_thread(delivered, "chat-1"), ["a", "c"]);
    assert!(in_thread(delivered, "chat-3").is_empty());
    // An empty id names no thread, not every notification without one.
    assert!(in_thread(delivered, "").is_empty());
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
    // Listened to: straight there, nothing lost.
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
    let lost = hub.dispatch(press("last"), at);
    assert!(matches!(&lost[..], [Delivery::Press(n)] if n.id == "0"));
    let (id, mut receiver) = hub.subscribe();
    hub.listen(id, Kind::Press);
    let ids = received(&mut receiver);
    assert_eq!(ids.len(), MAX_PENDING_PRESSES);
    assert_eq!(ids[0].1, "1");
    assert_eq!(ids[MAX_PENDING_PRESSES - 1].1, "last");
}

#[test]
fn notifications_in_front_and_tokens_do_not_wait() {
    let mut hub = Hub::new();
    let notification = Delivery::Notification(RemoteNotification::new("n", "", JsonValue::Null));
    assert_eq!(hub.dispatch(notification, Instant::now()).len(), 1);
    let token = Delivery::Token(DeviceToken::from_bytes([1]));
    assert_eq!(hub.dispatch(token, Instant::now()).len(), 1);
    assert!(hub.pending.is_empty());
}

#[test]
fn a_background_delivery_waits_from_its_arrival() {
    let mut hub = Hub::new();
    let at = Instant::now();
    let (expired, heard_expired) = recording();
    let (waited, heard_waited) = recording();
    assert!(
        hub.dispatch(background_at("early", expired, at), at)
            .is_empty()
    );
    let later = at + Duration::from_secs(3);
    assert!(
        hub.dispatch(background_at("late", waited, later), later)
            .is_empty()
    );

    // The wait passes for the first one only: the system hears it now.
    let gone = hub.expire(at + BACKGROUND_WAIT);
    assert_eq!(gone.len(), 1);
    drop(gone);
    assert_eq!(*heard_expired.lock().unwrap(), [BackgroundResult::NoData]);

    // A listener takes the other one, whose completion it reports.
    let (id, mut receiver) = hub.subscribe();
    hub.listen(id, Kind::Background);
    let Ok(Delivery::Background(n, completion, arrived)) = receiver.try_recv() else {
        panic!("the delivery that waited was not handed over");
    };
    assert_eq!(n.id, "late");
    assert_eq!(arrived, later);
    assert!(heard_waited.lock().unwrap().is_empty());
    completion.set_result(BackgroundResult::NewData);
    drop(completion);
    assert_eq!(*heard_waited.lock().unwrap(), [BackgroundResult::NewData]);
}

/// A delivery handed to a VM whose last listener stopped before its task
/// ran goes back to the hub: its wait still counts from its arrival, so
/// the one check the platform scheduled at arrival completes it.
#[test]
fn a_redispatched_background_delivery_still_expires() {
    let mut hub = Hub::new();
    let arrival = Instant::now();
    let (completion, heard) = recording();
    // Arrives while a VM listens: in its channel.
    let (id, mut receiver) = hub.subscribe();
    hub.listen(id, Kind::Background);
    assert!(
        hub.dispatch(background_at("b", completion, arrival), arrival)
            .is_empty()
    );
    let handed = receiver.try_recv().unwrap();
    // The listener stops; 2 s later the VM's task finds none and gives it
    // back.
    hub.unlisten(id, Kind::Background);
    assert!(
        hub.dispatch(handed, arrival + Duration::from_secs(2))
            .is_empty()
    );
    // The check scheduled at arrival.
    drop(hub.expire(arrival + BACKGROUND_WAIT + Duration::from_millis(100)));
    assert_eq!(*heard.lock().unwrap(), [BackgroundResult::NoData]);
    assert!(hub.pending.is_empty());
}

/// Given back after its wait, a delivery does not wait again: the system
/// hears it at once.
#[test]
fn a_background_delivery_given_back_late_is_completed_at_once() {
    let mut hub = Hub::new();
    let arrival = Instant::now();
    let (completion, heard) = recording();
    let (id, mut receiver) = hub.subscribe();
    hub.listen(id, Kind::Background);
    drop(hub.dispatch(background_at("b", completion, arrival), arrival));
    let handed = receiver.try_recv().unwrap();
    hub.unlisten(id, Kind::Background);
    let lost = hub.dispatch(handed, arrival + BACKGROUND_WAIT + Duration::from_secs(1));
    assert_eq!(lost.len(), 1);
    assert!(hub.pending.is_empty());
    drop(lost);
    assert_eq!(*heard.lock().unwrap(), [BackgroundResult::NoData]);
}

#[test]
fn every_listening_vm_shares_a_background_completion() {
    let mut hub = Hub::new();
    let (a, mut to_a) = hub.subscribe();
    let (b, mut to_b) = hub.subscribe();
    hub.listen(a, Kind::Background);
    hub.listen(b, Kind::Background);
    let (completion, heard) = recording();
    let at = Instant::now();
    // Handed to both; nothing is lost.
    assert!(
        hub.dispatch(background_at("b", completion, at), at)
            .is_empty()
    );
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
    assert!(hub.dispatch(press("p"), Instant::now()).is_empty());
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
    assert!(hub.dispatch(press("r"), Instant::now()).is_empty());
    assert_eq!(hub.pending.len(), 1);
    assert!(hub.subscribers.iter().all(|s| s.id != presses));
    hub.unsubscribe(tokens);
    assert!(hub.subscribers.is_empty());
}

/// A press a VM did not post before it closed (a reload) goes back to the
/// hub for the next VM that listens.
#[test]
fn a_closing_vm_gives_back_what_it_did_not_post() {
    let _turn = turn();
    let pending_before = hub().pending.len();
    let (id, receiver) = hub().subscribe();
    hub().listen(id, Kind::Press);
    let listeners = Listeners {
        subscriber: Cell::new(Some(id)),
        receiver: RefCell::new(Some(receiver)),
        ..Default::default()
    };
    assert!(hub().dispatch(press("unposted"), now()).is_empty());
    listeners.close();
    let mut hub = hub();
    assert!(hub.subscribers.iter().all(|s| s.id != id));
    assert_eq!(hub.pending.len(), pending_before + 1);
    let back = hub.pending.pop();
    assert!(matches!(back, Some(Delivery::Press(n)) if n.id == "unposted"));
}

#[test]
fn a_token_is_reported_when_it_changes() {
    let _turn = turn();
    let mut hub = Hub::new();
    let (id, mut receiver) = hub.subscribe();
    hub.listen(id, Kind::Token);
    let (answer, registration) = oneshot::channel();
    hub.registering.push(answer);
    let token = DeviceToken::from_bytes([1, 2]);
    let answers = hub.registered(&token);
    assert_eq!(answers.len(), 1);
    drop(answers);
    drop(registration);
    assert_eq!(received(&mut receiver), [(Kind::Token, "0102".to_owned())]);
    // The same token again: registrations are answered, listeners hear
    // nothing new.
    drop(hub.registered(&token));
    assert!(received(&mut receiver).is_empty());
    drop(hub.registered(&DeviceToken::from_bytes([3])));
    assert_eq!(received(&mut receiver), [(Kind::Token, "03".to_owned())]);
    assert_eq!(hub.token, Some(DeviceToken::from_bytes([3])));
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
    registered(DeviceToken::from_bytes([0xab]));
    registered(DeviceToken::from_bytes([0xab]));
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
    assert_eq!(token(), Some(DeviceToken::from_bytes([0xab])));
    // The ones that wait go; the observer's result stays.
    let pending: Vec<_> = std::mem::take(&mut hub().pending);
    drop(pending);
    assert_eq!(*done.lock().unwrap(), [BackgroundResult::NewData]);
}

#[test]
fn foreground_presentations_are_decided_before_notifications_come() {
    let _turn = turn();
    let quiet = ForegroundPresentation::SHOWN.banner(false).sound(false);
    let arrive = |thread: &str| arrived(RemoteNotification::new("n", thread, JsonValue::Null));

    // The app's, and a thread's own.
    set_foreground_presentation(ForegroundPresentation::SHOWN.badge(false));
    set_thread_presentation("chat-open", Some(quiet));
    assert_eq!(arrive("chat-open"), quiet);
    assert_eq!(thread_presentation("chat-open"), quiet);
    assert_eq!(
        arrive("chat-other"),
        ForegroundPresentation::SHOWN.badge(false)
    );

    // A Rust decider has the last word, from what the settings give.
    set_foreground_presentation_decider(Some(Arc::new(|notification, settings| {
        if notification.thread_id == "chat-muted" {
            settings.list(false)
        } else {
            settings
        }
    })));
    assert_eq!(
        arrive("chat-muted"),
        ForegroundPresentation::SHOWN.badge(false).list(false)
    );
    assert_eq!(arrive("chat-open"), quiet);
    set_foreground_presentation_decider(None);

    set_thread_presentation("chat-open", None);
    set_foreground_presentation(ForegroundPresentation::default());
    assert_eq!(arrive("chat-open"), ForegroundPresentation::SHOWN);
    assert_eq!(foreground_presentation(), ForegroundPresentation::SHOWN);
}

#[cfg(not(target_os = "ios"))]
#[test]
fn there_are_no_remote_notifications_off_ios() {
    let _turn = turn();
    assert!(!ready(available()));
    let error = ready(register()).unwrap_err();
    assert_eq!(error.code.as_deref(), Some(UNAVAILABLE));
    let error = ready(close_thread("chat-1")).unwrap_err();
    assert_eq!(error.code.as_deref(), Some(UNAVAILABLE));
    // Setting up does nothing, and does not panic.
    install();
}

#[cfg(not(target_os = "ios"))]
#[test]
fn an_empty_thread_is_refused() {
    let error = ready(close_thread("")).unwrap_err();
    assert_eq!(error.code, None);
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
        "ForegroundPresentationOptions",
        "ForegroundPresentationThread",
        "PushToken",
        "PushNotification",
        "PushBackground",
        "\"newData\"",
        "Promise<void |",
        "available(",
    ] {
        assert!(ts.contains(declared), "{declared} is not declared:\n{ts}");
    }
}
