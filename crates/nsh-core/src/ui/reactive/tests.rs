//! Signals, memos, effects and owners on a Rust runtime.

use std::{cell::RefCell, rc::Rc};

use super::*;

/// Makes what `setup` makes in a root of a fresh runtime (its effects run
/// once the root is made, as in Solid), then runs `check` on it.
fn run<T>(setup: impl FnOnce() -> T, check: impl FnOnce(T)) {
    let runtime = Runtime::default();
    let _entered = runtime.enter();
    let (owner, made) = create_root(setup);
    check(made);
    owner.dispose();
}

#[test]
fn signals_drive_memos_and_effects() {
    let seen = Rc::new(RefCell::new(Vec::new()));
    let log = seen.clone();
    run(
        move || {
            let count = signal(1);
            let doubled = computed(move || count.get() * 2);
            effect(move || log.borrow_mut().push(doubled.get()));
            (count, doubled)
        },
        |(count, doubled)| {
            count.set(2);
            count.set(2);
            count.update(|n| *n += 1);
            assert_eq!(*seen.borrow(), [2, 4, 6]);
            assert_eq!(doubled.get_untracked(), 6);
        },
    );
}

#[test]
fn signals_without_equality_notify_every_write() {
    let runs = Rc::new(RefCell::new(0));
    let r = runs.clone();
    run(
        move || {
            let ticks = signal_with(Vec::<u8>::new(), SignalEquals::Never);
            effect(move || {
                ticks.with(|_| ());
                *r.borrow_mut() += 1;
            });
            ticks
        },
        |ticks| {
            ticks.set(Vec::new());
            assert_eq!(*runs.borrow(), 2);
        },
    );
}

#[test]
fn what_an_owner_made_goes_with_it() {
    let runtime = Runtime::default();
    let _entered = runtime.enter();
    let cleaned = Rc::new(RefCell::new(false));
    let c = cleaned.clone();
    let (owner, count) = create_root(move || {
        let count = signal(0);
        cleanup(move || *c.borrow_mut() = true);
        count
    });
    assert_eq!(count.try_get(), Some(0));
    owner.dispose();
    assert!(*cleaned.borrow());
    assert_eq!(count.try_get(), None, "the signal went with its owner");
    assert!(!count.try_set(1));
}

#[test]
#[should_panic(expected = "after its owner was disposed")]
fn reading_a_disposed_signal_panics() {
    let runtime = Runtime::default();
    let _entered = runtime.enter();
    let (owner, count) = create_root(|| signal(0));
    owner.dispose();
    count.get();
}

#[test]
fn context_reaches_what_is_made_below() {
    #[derive(Clone, PartialEq, Debug)]
    struct Accent(&'static str);
    run(
        || {
            provide_context(Accent("blue"));
            let (_, found) = create_root(use_context::<Accent>);
            (found, use_context::<u32>())
        },
        |(found, missing)| {
            assert_eq!(found, Some(Accent("blue")));
            assert_eq!(missing, None);
        },
    );
}

#[test]
fn a_panicking_effect_is_reported_with_its_owner() {
    let runtime = Runtime::default();
    let (owner, ()) = {
        let _entered = runtime.enter();
        create_root(|| effect(|| panic!("boom")))
    };
    let failures = runtime.take_failures();
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].1, "boom");
    assert_ne!(failures[0].0, owner, "the effect's own node ran it");
}

#[test]
fn batches_and_untracked_reads() {
    let runs = Rc::new(RefCell::new(0));
    let r = runs.clone();
    run(
        move || {
            let a = signal(0);
            let b = signal(0);
            effect(move || {
                a.get();
                untracked(|| b.get());
                *r.borrow_mut() += 1;
            });
            (a, b)
        },
        |(a, b)| {
            batch(|| {
                a.set(1);
                a.set(2);
            });
            b.set(5);
            assert_eq!(*runs.borrow(), 2);
        },
    );
}
