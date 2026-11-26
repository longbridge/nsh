//! The syntaxes the built-in highlighting uses, as an app gets them for
//! highlighting of its own: loaded from a task, then the same set every
//! call answers with.

use nsh_core::prelude::*;
use nsh_engine::EngineConfig;
use nsh_testing::Headless;

#[component]
fn LoadsSyntaxes() -> impl IntoElement {
    let found = signal(String::from("waiting"));
    spawn(async move {
        let Some(set) = nsh_base::load_syntax_set().await else {
            found.set("none".into());
            return;
        };
        let same = nsh_base::syntax_set().is_some_and(|now| std::ptr::eq(now, set));
        let rust = set
            .find_syntax_by_token("rs")
            .map_or("unknown", |syntax| syntax.name.as_str());
        found.set(format!("{rust} {same}"));
    });
    view().child(text(move || format!("Syntax {}", found.get())))
}

#[test]
fn an_app_loads_the_built_in_syntaxes_from_a_task() {
    let mut app = Headless::start(EngineConfig::new(
        nsh_testing::windowed_root::<LoadsSyntaxes>(),
    ))
    .unwrap();
    app.wait_for("Syntax Rust true").unwrap();
    // Loaded, every call answers at once, with the same set.
    let set = nsh_base::syntax_set().unwrap();
    assert!(std::ptr::eq(set, nsh_base::syntax_set().unwrap()));
    // Bat's set (two-face's): TSX too, which syntect's own set lacks.
    assert!(set.find_syntax_by_token("tsx").is_some());
}
