//! `nsh`'s `openAuthSession` and `closeAuthSession`: a platform that shows
//! sign-in sessions in the app gets the page, and its answer resolves the
//! promise with the callback URL or `null`; refused requests reject before
//! anything shows, and `closeAuthSession` resolves the waiting one with
//! `null`.

use std::time::Duration;

use nsh_core::{Command, EventKind, MountItem, ROOT_ID, auth_session::AuthSessionOutcome};
use nsh_engine::Environment;

const APP: &str = r#"import { mount, signal, Text, openAuthSession, closeAuthSession } from "nsh";

const log = signal<string[]>([]);
const say = (s: string) => log.set([...log(), s]);
const show = (name: string) => (url: string | null) => say(`${name} ${url}`);

openAuthSession("https://example.com/authorize?a=1", { callbackScheme: "demo", ephemeral: true })
  .then(show("first"), (e: Error) => say(`first ${e.name}`));
openAuthSession("javascript:alert(1)", { callbackScheme: "demo" })
  .catch((e: Error) => say(`refused ${e.name}`));
openAuthSession("https://example.com/authorize", { callbackScheme: "https" })
  .catch((e: Error) => say(`refused ${e.name}`));
setTimeout(() => {
  openAuthSession("https://example.com/again", { callbackScheme: "demo" }).then(show("second"));
  setTimeout(() => closeAuthSession(), 50);
}, 200);

mount(() => <Text>{log().join(" | ")}</Text>);
"#;

#[test]
fn sessions_resolve_with_the_callback_or_null() {
    let environment = Environment {
        auth_session: true,
        ..Default::default()
    };
    let (engine, rx) = crate::components::start_with(APP, environment, None, None);
    let mut last = String::new();
    let mut pages = Vec::new();
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while std::time::Instant::now() < deadline && !last.contains("second null") {
        let Ok(batch) = rx.recv_timeout(Duration::from_millis(100)) else {
            continue;
        };
        for item in batch.items {
            match item {
                MountItem::Text { text, .. } => last = text.text.to_string(),
                MountItem::Command {
                    id,
                    command: Command::AuthSession { id: request, page },
                } => {
                    assert_eq!(id, ROOT_ID);
                    pages.push((
                        page.url.clone(),
                        page.callback_scheme.clone(),
                        page.ephemeral,
                    ));
                    // The first ends with its callback; the second stays
                    // open until the app closes it.
                    if pages.len() == 1 {
                        engine.control().dispatch_event(
                            ROOT_ID,
                            EventKind::AuthSession,
                            AuthSessionOutcome::Completed("demo://done?code=7".into())
                                .payload(request),
                        );
                    }
                }
                _ => {}
            }
        }
    }
    assert!(last.contains("first demo://done?code=7"), "{last}");
    assert!(last.contains("second null"), "{last}");
    assert_eq!(last.matches("refused TypeError").count(), 2, "{last}");
    assert_eq!(
        pages,
        [
            (
                "https://example.com/authorize?a=1".into(),
                "demo".into(),
                true
            ),
            ("https://example.com/again".into(), "demo".into(), false),
        ]
    );
}
