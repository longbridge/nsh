//! Standalone terminal transport for the Native Shell development host.

use std::{cell::RefCell, io, path::PathBuf, rc::Rc};

use clap::Parser;
use nsh_platform_tui::{AppOptions, AppView, Bundle, CellSize, Limits, RunEvent, run_app};
use ratatui_core::terminal::Terminal;
use ratatui_crossterm::CrosstermBackend;

mod logging;
mod shutdown;
mod terminal;

pub use terminal::convert;

#[derive(Parser)]
#[command(about = "Run Native Shell in the terminal (C-h e opens logs; Ctrl+C exits)")]
struct Args {
    #[arg(long, conflicts_with = "dev_server")]
    bundle: Option<PathBuf>,
    #[arg(long)]
    dev_server: Option<String>,
    #[arg(long, default_value = "8x16")]
    points_per_cell: CellSize,
    #[arg(long)]
    log_file: Option<PathBuf>,
    #[arg(long)]
    data_dir: Option<PathBuf>,
    #[arg(long)]
    trace: bool,
}

/// Parses host arguments and runs the selected bundle. The binary owns logging and diagnostics.
pub fn main() -> io::Result<()> {
    main_with_options(AppOptions::default())
}

/// Runs the host with an app's registered UI sets and optional Rust root.
pub fn main_with_options(mut options: AppOptions) -> io::Result<()> {
    let args = Args::parse();
    let log_file = args.log_file.unwrap_or_else(|| {
        std::env::temp_dir().join(format!("nsh-tui-host-{}.log", std::process::id()))
    });
    logging::init(&log_file, args.trace)?;
    options.limits = Limits::standalone();
    options.data_dir = args.data_dir.or_else(nsh_engine::data_dir);
    options.trace = args.trace;
    options.cells = args.points_per_cell;
    let mut view = AppView::new(options);
    view.set_dev_log_path(log_file);
    let is_dev = args.bundle.is_none();
    let failure = Rc::new(RefCell::new(None));
    let reported = failure.clone();
    view.on_app_error(move |error| {
        *reported.borrow_mut() = Some(error.to_string());
    });
    let _shutdown = shutdown::Guard::install()?;
    let session = terminal::Session::enter()?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    // Prime the area before loading so the first engine layout has the terminal's size.
    terminal.draw(|frame| view.render(frame, frame.area()))?;
    if let Some(path) = args.bundle {
        view.load(Bundle::read(&path)?)?;
    } else {
        view.connect_dev_server(
            args.dev_server
                .unwrap_or_else(|| "ws://127.0.0.1:8790/ws".into()),
        )?;
    }
    run_app(&mut terminal, &mut view, |timeout| {
        if !session.is_active() {
            Err(io::Error::other("terminal session ended after a panic"))
        } else if shutdown::requested() || (!is_dev && failure.borrow().is_some()) {
            Ok(RunEvent::Exit)
        } else {
            terminal::poll(timeout)
        }
    })?;
    let error = if is_dev {
        None
    } else {
        failure.borrow_mut().take()
    };
    match error {
        Some(error) => Err(io::Error::other(error)),
        None => Ok(()),
    }
}
