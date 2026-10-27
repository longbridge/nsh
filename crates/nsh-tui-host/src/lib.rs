//! Standalone terminal transport for the Native Shell development host.

use std::{cell::RefCell, io, path::PathBuf, rc::Rc};

use clap::Parser;
use nsh_platform_tui::{AppOptions, AppView, Bundle, Limits, RunEvent, run_app};
use ratatui_core::terminal::Terminal;
use ratatui_crossterm::CrosstermBackend;

mod terminal;

pub use terminal::convert;

#[derive(Parser)]
#[command(about = "Run a prebuilt Native Shell bundle in the terminal (Ctrl+C exits)")]
struct Args {
    #[arg(long)]
    bundle: PathBuf,
    #[arg(long)]
    data_dir: Option<PathBuf>,
    #[arg(long)]
    trace: bool,
}

/// Parses host arguments and runs the selected bundle. The binary owns logging and diagnostics.
pub fn main() -> io::Result<()> {
    let args = Args::parse();
    log::set_max_level(if args.trace {
        log::LevelFilter::Info
    } else {
        log::LevelFilter::Warn
    });
    run(args)
}

fn run(args: Args) -> io::Result<()> {
    let bundle = Bundle::read(&args.bundle)?;
    let mut options = AppOptions::default();
    options.limits = Limits::standalone();
    options.data_dir = args.data_dir.or_else(nsh_engine::data_dir);
    options.trace = args.trace;
    let mut view = AppView::new(options);
    let failure = Rc::new(RefCell::new(None));
    let reported = failure.clone();
    view.on_app_error(move |error| {
        *reported.borrow_mut() = Some(error.to_string());
    });
    let session = terminal::Session::enter()?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    // Prime the area before loading so the first engine layout has the terminal's size.
    terminal.draw(|frame| view.render(frame, frame.area()))?;
    view.load(bundle)?;
    run_app(&mut terminal, &mut view, |timeout| {
        if !session.is_active() {
            Err(io::Error::other("terminal session ended after a panic"))
        } else if failure.borrow().is_some() {
            Ok(RunEvent::Exit)
        } else {
            terminal::poll(timeout)
        }
    })?;
    let error = failure.borrow_mut().take();
    match error {
        Some(error) => Err(io::Error::other(error)),
        None => Ok(()),
    }
}
