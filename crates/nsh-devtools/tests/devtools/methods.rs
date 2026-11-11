use nsh_dev::protocol::{ClockChange, SceneQuery, TestFile, TestStep};
use nsh_devtools::{
    PROTOCOL_VERSION,
    description::description,
    methods::{METHODS, method_of, step_of},
    protocol::{ERR_BAD_PARAMS, ERR_UNKNOWN_METHOD},
};
use serde_json::json;

fn selector() -> String {
    "id:save".into()
}

fn samples() -> Vec<TestStep> {
    vec![
        TestStep::Ready { timeout_ms: 1500 },
        TestStep::Quit,
        TestStep::Press {
            selector: selector(),
        },
        TestStep::LongPress {
            selector: selector(),
        },
        TestStep::Type {
            selector: selector(),
            text: "hello".into(),
        },
        TestStep::Submit {
            selector: selector(),
        },
        TestStep::Clear {
            selector: selector(),
        },
        TestStep::Scroll {
            selector: selector(),
            distance: -120.5,
        },
        TestStep::Drag {
            selector: selector(),
            x: 12.25,
            y: -3.5,
        },
        TestStep::Hover {
            selector: selector(),
        },
        TestStep::RightClick {
            selector: selector(),
        },
        TestStep::Paste {
            selector: selector(),
            text: "pasted".into(),
            html: "<b>pasted</b>".into(),
            files: vec![TestFile::new("a.txt", "text/plain", vec![1, 2, 3])],
            uris: vec!["file:///a.txt".into()],
        },
        TestStep::PressKey {
            keys: "cmd-s".into(),
        },
        TestStep::HoldKeys {
            keys: "shift".into(),
            down: true,
        },
        TestStep::DragAndHold {
            selector: selector(),
            x: 4.5,
            y: 8.0,
        },
        TestStep::Drop,
        TestStep::DropFiles {
            selector: selector(),
            uris: vec!["file:///b.png".into()],
        },
        TestStep::Back,
        TestStep::Menu,
        TestStep::ChooseMenuItem {
            label: "Copy".into(),
        },
        TestStep::CloseMenu,
        TestStep::Clipboard,
        TestStep::OpenedUrls,
        TestStep::Clock {
            change: ClockChange::RunFor { ms: 250 },
        },
        TestStep::SetViewport {
            width: 390.0,
            height: 844.5,
        },
        TestStep::SetColorScheme { dark: true },
        TestStep::Dialogs,
        TestStep::PendingDialog,
        TestStep::AnswerDialog {
            button: Some("OK".into()),
        },
        TestStep::HostSend {
            name: "greet".into(),
            data: json!({ "who": "world" }),
        },
        TestStep::Wait { ms: 40 },
        TestStep::WaitFor {
            selector: selector(),
            timeout_ms: 900,
            gone: true,
        },
        TestStep::NextChange { timeout_ms: 700 },
        TestStep::NextFrame { ms: 16 },
        TestStep::Texts,
        TestStep::Snapshot {
            root: Some("id:list".into()),
            depth: Some(3),
            hidden: true,
        },
        TestStep::Count {
            selector: selector(),
        },
        TestStep::Shows {
            selector: selector(),
        },
        TestStep::Enabled {
            selector: selector(),
        },
        TestStep::Value {
            selector: selector(),
        },
        TestStep::SpanStyle {
            selector: selector(),
        },
        TestStep::Screenshot,
        TestStep::WindowScreenshot { id: "main".into() },
        TestStep::WindowState { id: "main".into() },
        TestStep::PressWindowClose { id: "main".into() },
        TestStep::Scene {
            query: SceneQuery::Activity,
        },
        TestStep::Scene {
            query: SceneQuery::TakeErrors,
        },
        TestStep::Scene {
            query: SceneQuery::Answered,
        },
        TestStep::Scene {
            query: SceneQuery::Storage,
        },
    ]
}

#[test]
fn every_step_round_trips_through_its_method() {
    let samples = samples();
    for step in &samples {
        let (method, params) = method_of(step).expect("mapped");
        assert_eq!(&step_of(method, params).unwrap(), step, "{method}");
    }
    // Every method of the table has a sample.
    for spec in METHODS {
        assert!(
            samples
                .iter()
                .any(|s| method_of(s).map(|(m, _)| m) == Some(spec.method)),
            "no sample for {}",
            spec.method
        );
    }
}

#[test]
fn unmapped_steps_have_no_method() {
    assert!(
        method_of(&TestStep::Scene {
            query: SceneQuery::Menu
        })
        .is_none()
    );
}

#[test]
fn params_are_the_steps_own_fields() {
    let step = TestStep::Type {
        selector: selector(),
        text: "x".into(),
    };
    let (method, params) = method_of(&step).unwrap();
    assert_eq!(method, "Input.type");
    assert_eq!(params, json!({ "selector": "id:save", "text": "x" }));
}

#[test]
fn unknown_method_and_bad_params_are_errors() {
    assert_eq!(
        step_of("Input.fly", json!({})).unwrap_err().code,
        ERR_UNKNOWN_METHOD
    );
    assert_eq!(
        step_of("Input.press", json!({ "selektor": 1 }))
            .unwrap_err()
            .code,
        ERR_BAD_PARAMS
    );
    assert_eq!(
        step_of("Input.press", json!(5)).unwrap_err().code,
        ERR_BAD_PARAMS
    );
}

#[test]
fn description_lists_every_method_and_is_experimental() {
    let d = description();
    assert_eq!(d["version"], PROTOCOL_VERSION);
    assert_eq!(d["stability"], "experimental");
    let names: Vec<_> = d["methods"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["name"].as_str().unwrap())
        .collect();
    for spec in METHODS {
        assert!(names.contains(&spec.method), "{}", spec.method);
    }
    assert_eq!(
        d["methods"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|m| m["capability"] == "privileged")
            .count(),
        2
    );
}
