//! `nsh test`'s runner on a test file of a small TSX app
//! (`todo/App.test.tsx`).

use std::path::Path;

use nsh_testing::{TestResult, TestRun, compiler, run_file};

fn todo() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/testing/todo")
}

fn test_run(filter: Option<&str>) -> TestRun {
    TestRun {
        ui_sets: Vec::new(),
        compile: compiler(Vec::new()),
        work_dir: std::env::temp_dir().join(format!("nsh-testing-{}", std::process::id())),
        filter: filter.map(str::to_owned),
        trace: None,
    }
}

#[test]
fn a_test_file_runs_its_tests_in_order_with_what_they_set_up() {
    let mut reported = Vec::new();
    let results = run_file(
        &test_run(None),
        &todo().join("App.test.tsx"),
        &mut |r: &TestResult| reported.push(r.name.clone()),
    )
    .unwrap();
    assert_eq!(
        reported,
        [
            "adds a todo",
            "is offline without the network",
            "fails with what is on screen"
        ]
    );
    assert_eq!(results[0].failure, None, "{:?}", results[0]);
    assert_eq!(results[1].failure, None, "{:?}", results[1]);
    let failure = results[2].failure.as_ref().unwrap();
    assert_eq!(
        failure.message,
        r#"expected ["offline","Add","0 left"] to contain "Nope""#
    );
    // Where it failed, in the test file.
    let stack = failure.stack.as_deref().unwrap();
    assert!(
        stack.contains("(tests/testing/todo/App.test.tsx:30:23)"),
        "{stack}"
    );
    assert_eq!(
        failure.screen.as_deref(),
        Some(&["offline".to_owned(), "Add".into(), "0 left".into()][..])
    );
}

#[test]
fn a_filter_runs_the_tests_it_names() {
    let results = run_file(
        &test_run(Some("offline")),
        &todo().join("App.test.tsx"),
        &mut |_| {},
    )
    .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "is offline without the network");
}

#[test]
fn a_file_that_throws_as_it_loads_fails() {
    let error = run_file(&test_run(None), &todo().join("Broken.test.ts"), &mut |_| {}).unwrap_err();
    assert!(error.contains("the file is broken"), "{error}");
}
