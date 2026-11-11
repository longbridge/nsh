use nsh_devtools::console::{Collector, ConsoleQuery};

/// Records a log line (a `Record` borrows its message, so it cannot be returned).
fn emit(c: &Collector, level: log::Level, target: &str, message: &str) {
    c.record(
        &log::Record::builder()
            .level(level)
            .target(target)
            .args(format_args!("{message}"))
            .build(),
    );
}

fn query(set: impl FnOnce(&mut ConsoleQuery)) -> ConsoleQuery {
    let mut q = ConsoleQuery::default();
    set(&mut q);
    q
}

#[test]
fn collector_keeps_the_newest_and_filters() {
    let c = Collector::new(3);
    for i in 0..5 {
        emit(&c, log::Level::Info, "wtt::trade", &format!("m{i}"));
    }
    let all = c.query(&ConsoleQuery::default());
    assert_eq!(
        all.iter().map(|e| e.message.as_str()).collect::<Vec<_>>(),
        ["m2", "m3", "m4"]
    );
    emit(&c, log::Level::Warn, "js", "w");
    let warns = c.query(&query(|q| q.level = Some(log::Level::Warn)));
    assert_eq!(warns.len(), 1);
    assert_eq!(c.query(&query(|q| q.target = Some("js".into()))).len(), 1);
    assert_eq!(c.query(&query(|q| q.limit = Some(1)))[0].message, "w");
    assert_eq!(c.query(&query(|q| q.text = Some("m3".into()))).len(), 1);
}

#[test]
fn subscribers_get_entries_after_they_subscribe() {
    let c = Collector::new(10);
    emit(&c, log::Level::Info, "js", "before");
    let rx = c.subscribe();
    emit(&c, log::Level::Info, "js", "after");
    assert_eq!(rx.try_recv().unwrap().message, "after");
    assert!(rx.try_recv().is_err());
}

#[test]
fn closed_subscribers_are_dropped() {
    let c = Collector::new(10);
    drop(c.subscribe());
    emit(&c, log::Level::Info, "js", "x");
    let rx = c.subscribe();
    emit(&c, log::Level::Info, "js", "y");
    assert_eq!(rx.try_recv().unwrap().message, "y");
}
