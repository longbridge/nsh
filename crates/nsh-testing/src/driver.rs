//! What `nsh/test`'s `App` drives: an app on a headless engine
//! ([`Headless`]), or on a real platform's host (`nsh test --target`),
//! whose test agent takes the same steps ([`crate::agent`]) through the
//! dev server ([`Host`]).

use std::time::Duration;

use nsh_core::{
    Dialog,
    geometry::{Points, Size},
    platform::ColorScheme,
};
use nsh_dev::protocol::TestStep;
use serde_json::Value;

use crate::headless::{DialogAnswer, Headless};

/// An app a test opened, driven by steps and read by queries.
pub(crate) trait Driver: Send {
    /// Names the step the batches that come next belong to (`--trace`).
    fn step(&mut self, name: &str);
    fn press(&mut self, selector: &str) -> Result<(), String>;
    fn long_press(&mut self, selector: &str) -> Result<(), String>;
    fn type_text(&mut self, selector: &str, text: &str) -> Result<(), String>;
    fn submit(&mut self, selector: &str) -> Result<(), String>;
    fn clear(&mut self, selector: &str) -> Result<(), String>;
    fn scroll(&mut self, selector: &str, distance: Points) -> Result<(), String>;
    fn back(&mut self) -> Result<(), String>;
    fn wait(&mut self, time: Duration) -> Result<(), String>;
    fn wait_for_within(&mut self, selector: &str, timeout: Duration) -> Result<(), String>;
    fn wait_for_gone(&mut self, selector: &str, timeout: Duration) -> Result<(), String>;
    fn next_change(&mut self, timeout: Duration) -> Result<(), String>;
    fn set_viewport(&mut self, size: Size<Points>) -> Result<(), String>;
    fn set_color_scheme(&mut self, scheme: ColorScheme) -> Result<(), String>;
    fn run_for(&mut self, time: Duration) -> Result<(), String>;
    fn fast_forward(&mut self, time: Duration) -> Result<(), String>;
    fn set_system_time(&mut self, at: std::time::SystemTime) -> Result<(), String>;
    /// The oldest dialog not answered yet.
    fn pending_dialog(&mut self) -> Option<Dialog>;
    fn answer_dialog(&mut self, answer: DialogAnswer) -> Result<(), String>;
    fn texts(&mut self) -> Result<Vec<String>, String>;
    fn count(&mut self, selector: &str) -> Result<usize, String>;
    fn enabled(&mut self, selector: &str) -> Result<bool, String>;
    fn shows(&mut self, selector: &str) -> Result<bool, String>;
    fn value(&mut self, selector: &str) -> Result<Option<String>, String>;
    fn dialogs(&mut self) -> Result<Vec<Dialog>, String>;
    fn clipboard(&mut self) -> Result<Option<String>, String>;
    /// A picture of the app's window, as a PNG.
    fn screenshot(&mut self) -> Result<Vec<u8>, String>;
}

impl Driver for Headless {
    fn step(&mut self, name: &str) {
        Headless::step(self, name);
    }
    fn press(&mut self, selector: &str) -> Result<(), String> {
        Headless::press(self, selector)
    }
    fn long_press(&mut self, selector: &str) -> Result<(), String> {
        Headless::long_press(self, selector)
    }
    fn type_text(&mut self, selector: &str, text: &str) -> Result<(), String> {
        Headless::type_text(self, selector, text)
    }
    fn submit(&mut self, selector: &str) -> Result<(), String> {
        Headless::submit(self, selector)
    }
    fn clear(&mut self, selector: &str) -> Result<(), String> {
        Headless::clear(self, selector)
    }
    fn scroll(&mut self, selector: &str, distance: Points) -> Result<(), String> {
        Headless::scroll(self, selector, distance)
    }
    fn back(&mut self) -> Result<(), String> {
        Headless::back(self)
    }
    fn wait(&mut self, time: Duration) -> Result<(), String> {
        Headless::wait(self, time)
    }
    fn wait_for_within(&mut self, selector: &str, timeout: Duration) -> Result<(), String> {
        Headless::wait_for_within(self, selector, timeout)
    }
    fn wait_for_gone(&mut self, selector: &str, timeout: Duration) -> Result<(), String> {
        Headless::wait_for_gone(self, selector, timeout)
    }
    fn next_change(&mut self, timeout: Duration) -> Result<(), String> {
        Headless::next_change(self, timeout)
    }
    fn set_viewport(&mut self, size: Size<Points>) -> Result<(), String> {
        Headless::set_viewport(self, size)
    }
    fn set_color_scheme(&mut self, scheme: ColorScheme) -> Result<(), String> {
        let name = match scheme {
            ColorScheme::Dark => "dark",
            _ => "light",
        };
        self.set_environment(&format!("setColorScheme({name})"), |e| {
            e.color_scheme = scheme;
        })
    }
    fn run_for(&mut self, time: Duration) -> Result<(), String> {
        Headless::run_for(self, time)
    }
    fn fast_forward(&mut self, time: Duration) -> Result<(), String> {
        Headless::fast_forward(self, time)
    }
    fn set_system_time(&mut self, at: std::time::SystemTime) -> Result<(), String> {
        Headless::set_system_time(self, at)
    }
    fn pending_dialog(&mut self) -> Option<Dialog> {
        Headless::pending_dialog(self).cloned()
    }
    fn answer_dialog(&mut self, answer: DialogAnswer) -> Result<(), String> {
        Headless::answer_dialog(self, answer)
    }
    fn texts(&mut self) -> Result<Vec<String>, String> {
        self.catch_up();
        Ok(self
            .screen()
            .texts()
            .into_iter()
            .map(str::to_owned)
            .collect())
    }
    fn count(&mut self, selector: &str) -> Result<usize, String> {
        self.catch_up();
        self.screen().count(selector)
    }
    fn enabled(&mut self, selector: &str) -> Result<bool, String> {
        self.catch_up();
        self.screen().enabled(selector)
    }
    fn shows(&mut self, selector: &str) -> Result<bool, String> {
        self.catch_up();
        self.screen().matches(selector)
    }
    fn value(&mut self, selector: &str) -> Result<Option<String>, String> {
        self.catch_up();
        Ok(self.screen().input_text(selector)?.map(str::to_owned))
    }
    fn dialogs(&mut self) -> Result<Vec<Dialog>, String> {
        self.catch_up();
        Ok(self.screen().dialogs().to_vec())
    }
    fn clipboard(&mut self) -> Result<Option<String>, String> {
        self.catch_up();
        Ok(self.screen().clipboard().map(str::to_owned))
    }
    fn screenshot(&mut self) -> Result<Vec<u8>, String> {
        Err(
            "a screenshot needs a real platform, which draws the app: run the test with `--target`"
                .into(),
        )
    }
}

/// A real platform's host running an app for a test (`nsh test --target`):
/// it takes [`TestStep`]s through its test agent. Dropping it closes the
/// app.
pub trait Host: Send {
    /// Asks the host's agent to take `step`, waiting `timeout` at most:
    /// what a query gives (`null` for actions), or why it failed.
    fn step(&self, step: TestStep, timeout: Duration) -> Result<Value, String>;
}

/// How long a step may take on a host before the test gives up on it: its
/// own waits (a `waitFor`'s timeout, a quiet app) plus room for the round
/// trip.
const STEP_TIMEOUT: Duration = Duration::from_secs(30);

/// An app on a real platform's host.
pub(crate) struct Remote {
    host: Box<dyn Host>,
}

impl Remote {
    pub(crate) fn new(host: Box<dyn Host>) -> Self {
        Remote { host }
    }

    /// Takes `step` on the host.
    pub(crate) fn take(&self, step: TestStep) -> Result<Value, String> {
        self.host.step(step, STEP_TIMEOUT)
    }

    fn action(&self, step: TestStep) -> Result<(), String> {
        self.take(step).map(drop)
    }

    fn query<T: serde::de::DeserializeOwned>(&self, step: TestStep) -> Result<T, String> {
        let value = self.take(step)?;
        serde_json::from_value(value).map_err(|e| format!("the host answered oddly: {e}"))
    }
}

/// What a test asks of a real platform that only headless runs do.
pub(crate) fn headless_only(what: &str) -> String {
    format!("{what} runs headless only for now: run the test without `--target`")
}

fn ms(time: Duration) -> u64 {
    time.as_millis().min(u128::from(u64::MAX)) as u64
}

impl Driver for Remote {
    fn step(&mut self, _: &str) {}
    fn press(&mut self, selector: &str) -> Result<(), String> {
        self.action(TestStep::Press {
            selector: selector.into(),
        })
    }
    fn long_press(&mut self, selector: &str) -> Result<(), String> {
        self.action(TestStep::LongPress {
            selector: selector.into(),
        })
    }
    fn type_text(&mut self, selector: &str, text: &str) -> Result<(), String> {
        self.action(TestStep::Type {
            selector: selector.into(),
            text: text.into(),
        })
    }
    fn submit(&mut self, selector: &str) -> Result<(), String> {
        self.action(TestStep::Submit {
            selector: selector.into(),
        })
    }
    fn clear(&mut self, selector: &str) -> Result<(), String> {
        self.action(TestStep::Clear {
            selector: selector.into(),
        })
    }
    fn scroll(&mut self, selector: &str, distance: Points) -> Result<(), String> {
        self.action(TestStep::Scroll {
            selector: selector.into(),
            distance: distance.0,
        })
    }
    fn back(&mut self) -> Result<(), String> {
        self.action(TestStep::Back)
    }
    fn wait(&mut self, time: Duration) -> Result<(), String> {
        self.action(TestStep::Wait { ms: ms(time) })
    }
    fn wait_for_within(&mut self, selector: &str, timeout: Duration) -> Result<(), String> {
        self.action(TestStep::WaitFor {
            selector: selector.into(),
            timeout_ms: ms(timeout),
            gone: false,
        })
    }
    fn wait_for_gone(&mut self, selector: &str, timeout: Duration) -> Result<(), String> {
        self.action(TestStep::WaitFor {
            selector: selector.into(),
            timeout_ms: ms(timeout),
            gone: true,
        })
    }
    fn next_change(&mut self, timeout: Duration) -> Result<(), String> {
        self.action(TestStep::NextChange {
            timeout_ms: ms(timeout),
        })
    }
    fn set_viewport(&mut self, size: Size<Points>) -> Result<(), String> {
        self.action(TestStep::SetViewport {
            width: size.width.0,
            height: size.height.0,
        })
    }
    fn set_color_scheme(&mut self, scheme: ColorScheme) -> Result<(), String> {
        self.action(TestStep::SetColorScheme {
            dark: scheme == ColorScheme::Dark,
        })
    }
    // A real platform's clock is the system's: time passes as it does.
    fn run_for(&mut self, time: Duration) -> Result<(), String> {
        self.action(TestStep::Wait { ms: ms(time) })
    }
    fn fast_forward(&mut self, _: Duration) -> Result<(), String> {
        Err(headless_only("Jumping the clock (`clock.fastForward`)"))
    }
    fn set_system_time(&mut self, _: std::time::SystemTime) -> Result<(), String> {
        Err(headless_only("Setting the clock (`clock.setSystemTime`)"))
    }
    fn pending_dialog(&mut self) -> Option<Dialog> {
        self.query(TestStep::PendingDialog).ok().flatten()
    }
    fn answer_dialog(&mut self, answer: DialogAnswer) -> Result<(), String> {
        let button = match answer {
            DialogAnswer::Press(text) => Some(text),
            DialogAnswer::Dismiss => None,
        };
        self.action(TestStep::AnswerDialog { button })
    }
    fn texts(&mut self) -> Result<Vec<String>, String> {
        self.query(TestStep::Texts)
    }
    fn count(&mut self, selector: &str) -> Result<usize, String> {
        self.query(TestStep::Count {
            selector: selector.into(),
        })
    }
    fn enabled(&mut self, selector: &str) -> Result<bool, String> {
        self.query(TestStep::Enabled {
            selector: selector.into(),
        })
    }
    fn shows(&mut self, selector: &str) -> Result<bool, String> {
        self.query(TestStep::Shows {
            selector: selector.into(),
        })
    }
    fn value(&mut self, selector: &str) -> Result<Option<String>, String> {
        self.query(TestStep::Value {
            selector: selector.into(),
        })
    }
    fn dialogs(&mut self) -> Result<Vec<Dialog>, String> {
        self.query(TestStep::Dialogs)
    }
    fn clipboard(&mut self) -> Result<Option<String>, String> {
        Err(headless_only("Reading the clipboard"))
    }
    fn screenshot(&mut self) -> Result<Vec<u8>, String> {
        use base64::Engine as _;
        let png: String = self.query(TestStep::Screenshot)?;
        base64::engine::general_purpose::STANDARD
            .decode(png)
            .map_err(|e| format!("the host sent a broken picture: {e}"))
    }
}

impl Drop for Remote {
    fn drop(&mut self) {
        // The host exits; dropping the host handle reaps it.
        let _ = self.host.step(TestStep::Quit, Duration::from_secs(5));
    }
}
