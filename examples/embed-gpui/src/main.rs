//! A GPUI Kit app embedding nsh apps: the Longbridge Pro / Allsum Desktop
//! case of native UI delivered at run time into a desktop app built with
//! GPUI Kit.
//!
//! The window is GPUI Kit's (its `Root`, a sidebar, buttons, an input, a
//! dialog); the two panes of the resizable split are [`AppView`]s, each
//! running its own nsh engine. The host's theme reaches them on every
//! change ([`theme::nsh_theme`], `observe_global::<Theme>`), Tab moves
//! through the host's widgets and the surfaces' fields alike, GPUI Kit's
//! dialogs and notifications draw above the surfaces, closing a pane
//! drops its view, which stops its engine, and the apps' watchlist and
//! "Extended hours" switch are GPUI Kit widgets this app registers as
//! native components ([`watchlist`], [`switch`]).
//!
//! The host's desk ([`trading`]: the selected symbol, the orders, the
//! positions) is shared both ways: the apps reach it by name
//! (`nativeModule("host/trading")`, served from the set this app gives
//! `set_ui_sets`) and subscribe to it, and the sidebar and toolbar follow
//! and change it, so a symbol or an order chosen anywhere shows
//! everywhere. "Show layers" outlines what is GPUI Kit's and what is
//! NSH's ([`layers`]).
//!
//! ```sh
//! cargo run                          # here
//! cargo run -p example-embed-gpui    # from the repository's root
//! ```
//!
//! `build.rs` bundles and compiles `app/` as `nsh build` does, so the app
//! is built with the example. `cargo run -- <bundle.nshc or its
//! directory>` runs another bundle;
//! `--alone` runs one surface in a window of its own (no host widgets).

mod layers;
mod switch;
mod theme;
mod trading;
mod watchlist;

use std::path::{Path, PathBuf};

use gpui_kit::{
    assets::Assets,
    component::{
        ActiveTheme, IconName, Sizable, Theme, ThemeMode, WindowExt,
        button::{Button, ButtonVariants},
        h_flex,
        input::{Input, InputEvent, InputState},
        notification::Notification,
        resizable::{h_resizable, resizable_panel},
        sidebar::{Sidebar, SidebarGroup, SidebarHeader, SidebarMenu, SidebarMenuItem},
        switch::Switch,
        v_flex,
    },
    prelude::FluentBuilder,
    *,
};
use nsh_platform_gpui::{App as NshApp, AppOptions, AppView, AssetBase, Bundle};

/// Which panes show.
#[derive(Clone, Copy, PartialEq)]
enum Layout {
    /// Both, side by side.
    Split,
    /// One of them.
    Single(usize),
}

/// One embedded app.
struct Pane {
    title: SharedString,
    /// `None` once closed: dropping the view stopped its engine.
    view: Option<Entity<AppView>>,
}

struct Shell {
    bundle: PathBuf,
    panes: Vec<Pane>,
    layout: Layout,
    search: Entity<InputState>,
    /// The host's desk as last seen ([`trading::watch`]).
    desk: trading::Desk,
    _theme: Subscription,
    _search: Subscription,
    _desk: Task<()>,
}

impl Shell {
    fn new(bundle: PathBuf, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Symbol, Enter selects"));
        // Enter selects the symbol on the desk, for every ticket.
        let search_events = cx.subscribe_in(
            &search,
            window,
            |this: &mut Shell, search, event: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    let symbol = search.read(cx).value().to_string();
                    this.select(&symbol, window, cx);
                }
            },
        );
        // Every theme change (mode, theme) goes to the embedded apps.
        let theme = cx.observe_global::<Theme>(|this: &mut Shell, cx| this.apply_theme(cx));
        // The desk's changes, from the tickets or from here.
        let mut desk = trading::watch();
        let follow = cx.spawn(async move |this, cx| {
            while desk.changed().await.is_ok() {
                let now = desk.borrow_and_update().clone();
                if this
                    .update(cx, |this, cx| {
                        this.desk = now;
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
        let mut shell = Shell {
            bundle,
            panes: Vec::new(),
            layout: Layout::Split,
            search,
            desk: trading::desk(),
            _theme: theme,
            _search: search_events,
            _desk: follow,
        };
        for title in ["Ticket A", "Ticket B"] {
            let view = shell.open_app(window, cx);
            shell.panes.push(Pane {
                title: title.into(),
                view,
            });
        }
        shell
    }

    /// A new view running the bundle, in the host's theme.
    fn open_app(&self, window: &mut Window, cx: &mut Context<Self>) -> Option<Entity<AppView>> {
        open_app(&self.bundle, window, cx)
    }

    fn apply_theme(&mut self, cx: &mut Context<Self>) {
        let theme = cx.theme();
        let (nsh_theme, appearance) = (theme::nsh_theme(theme), theme::appearance(theme.mode));
        for view in self.panes.iter().filter_map(|p| p.view.clone()) {
            view.update(cx, |view, cx| {
                view.set_theme(Some(nsh_theme.clone()), cx);
                view.set_appearance(appearance, cx);
            });
        }
        cx.notify();
    }

    fn toggle_mode(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mode = if cx.theme().mode.is_dark() {
            ThemeMode::Light
        } else {
            ThemeMode::Dark
        };
        Theme::change(mode, Some(window), cx);
    }

    /// Selects `symbol` on the desk (every ticket follows), or says why not.
    fn select(&mut self, symbol: &str, window: &mut Window, cx: &mut Context<Self>) {
        if let Err(err) = trading::select(symbol) {
            window.push_notification(Notification::new().message(err), cx);
        }
    }

    /// Shows or hides the layers' outlines, here and in the surfaces'
    /// GPUI widgets (which read the global when they draw).
    fn toggle_layers(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let shown = layers::shown(cx);
        cx.set_global(layers::Layers(!shown));
        window.refresh();
    }

    /// Switches to the themes registered from `themes/violet.json` (an nsh
    /// theme file, loaded into GPUI Kit's registry as it is), or back to
    /// GPUI Kit's default.
    fn set_violet(&mut self, violet: bool, window: &mut Window, cx: &mut Context<Self>) {
        let registry = gpui_kit::component::ThemeRegistry::global(cx);
        let (light, dark) = if violet {
            let themes = registry.themes();
            match (themes.get("Violet Light"), themes.get("Violet Dark")) {
                (Some(l), Some(d)) => (l.clone(), d.clone()),
                _ => return,
            }
        } else {
            (
                registry.default_light_theme().clone(),
                registry.default_dark_theme().clone(),
            )
        };
        let mode = cx.theme().mode;
        let theme = Theme::global_mut(cx);
        theme.light_theme = light;
        theme.dark_theme = dark;
        Theme::change(mode, Some(window), cx);
    }

    /// Closes pane `i` (its engine stops) or opens it again.
    fn toggle_pane(&mut self, i: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.panes[i].view.take().is_none() {
            self.panes[i].view = self.open_app(window, cx);
        }
        cx.notify();
    }

    fn pane(&self, i: usize, cx: &mut Context<Self>) -> impl IntoElement {
        let pane = &self.panes[i];
        let theme = cx.theme();
        v_flex()
            .size_full()
            .border_1()
            .border_color(theme.border)
            .rounded(theme.radius_lg)
            .overflow_hidden()
            .child(
                h_flex()
                    .px_3()
                    .py_1()
                    .gap_2()
                    .border_b_1()
                    .border_color(theme.border)
                    .bg(theme.title_bar)
                    .child(div().flex_1().text_sm().child(pane.title.clone()))
                    .child(
                        Button::new(("close", i))
                            .ghost()
                            .xsmall()
                            .label(if pane.view.is_some() { "Close" } else { "Open" })
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.toggle_pane(i, window, cx)
                            })),
                    ),
            )
            .child(match &pane.view {
                Some(view) => div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .child(view.clone())
                    .when(layers::shown(cx), |d| d.child(layers::nsh_overlay(cx))),
                None => div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(theme.muted_foreground)
                    .child("Closed: its engine stopped. Open starts a new one."),
            })
    }
}

impl Render for Shell {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let layout = self.layout;
        let item = |label: &'static str, target: Layout, cx: &mut Context<Self>| {
            let shell = cx.entity().downgrade();
            SidebarMenuItem::new(label)
                .active(layout == target)
                .on_click(move |_, _, cx| {
                    let _ = shell.update(cx, |this, cx| {
                        this.layout = target;
                        cx.notify();
                    });
                })
        };
        let menu = SidebarMenu::new().children([
            item("Both apps", Layout::Split, cx).icon(IconName::LayoutDashboard),
            item("Ticket A", Layout::Single(0), cx).icon(IconName::SquareTerminal),
            item("Ticket B", Layout::Single(1), cx).icon(IconName::SquareTerminal),
        ]);
        // The host's watchlist: the desk's symbol, picked here or in a
        // ticket.
        let watchlist = SidebarMenu::new().children(trading::SYMBOLS.map(|(symbol, price)| {
            let shell = cx.entity().downgrade();
            SidebarMenuItem::new(symbol)
                .active(self.desk.symbol == symbol)
                .suffix(move |_, cx| {
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(format!("${price:.2}"))
                })
                .on_click(move |_, window, cx| {
                    let _ = shell.update(cx, |this, cx| this.select(symbol, window, cx));
                })
        }));
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        // The desk's orders, from every ticket.
        let orders = v_flex()
            .gap_1()
            .px_2()
            .child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child("Orders, all tickets"),
            )
            .children(self.desk.orders.iter().map(|o| {
                let (side, color) = match o.side {
                    trading::Side::Buy => ("Buy", theme.chart_bullish),
                    trading::Side::Sell => ("Sell", theme.chart_bearish),
                };
                h_flex()
                    .gap_1()
                    .text_xs()
                    .child(div().text_color(muted).child(format!("#{}", o.id)))
                    .child(div().text_color(color).child(side))
                    .child(format!("{} {}", o.quantity, o.symbol))
            }))
            .when(self.desk.orders.is_empty(), |d| {
                d.child(div().text_xs().text_color(muted).child("None yet"))
            });
        let positions = if self.desk.positions.is_empty() {
            "No positions".to_owned()
        } else {
            self.desk
                .positions
                .iter()
                .map(|p| format!("{} {:+}", p.symbol, p.shares))
                .collect::<Vec<_>>()
                .join(" · ")
        };
        let dark = theme.mode.is_dark();
        let shown = layers::shown(cx);
        let toolbar = h_flex()
            .gap_2()
            .child(div().w(px(200.)).child(Input::new(&self.search).small()))
            .child(
                Button::new("mode")
                    .small()
                    .outline()
                    .label(if dark { "Light" } else { "Dark" })
                    .on_click(cx.listener(|this, _, window, cx| this.toggle_mode(window, cx))),
            )
            .child(
                Button::new("violet")
                    .small()
                    .outline()
                    .label("Violet theme")
                    .on_click(cx.listener(|this, _, window, cx| this.set_violet(true, window, cx))),
            )
            .child(
                Button::new("default")
                    .small()
                    .outline()
                    .label("Default theme")
                    .on_click(
                        cx.listener(|this, _, window, cx| this.set_violet(false, window, cx)),
                    ),
            )
            // A switch, so its state shows: on, the outlines and the legend.
            .child(
                Switch::new("layers")
                    .small()
                    .checked(shown)
                    .label("Show layers")
                    .on_change(
                        cx.listener(|this, _: &bool, window, cx| this.toggle_layers(window, cx)),
                    ),
            )
            .child(
                Button::new("dialog")
                    .small()
                    .primary()
                    .label("Dialog")
                    .on_click(|_, window, cx| {
                        window.open_dialog(cx, |dialog, _, _| {
                            dialog
                                .title("A GPUI Kit dialog")
                                .child("It draws above the nsh surfaces, which keep running.")
                        });
                        window.push_notification(
                            Notification::new().message("A notification above the surfaces"),
                            cx,
                        );
                    }),
            )
            .child(div().flex_1())
            .child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .child(format!("Positions: {positions}")),
            );
        // A gap on each side of the divider: the panes never touch it.
        let content = match layout {
            Layout::Split => h_resizable("panes")
                .child(resizable_panel().child(div().size_full().pr_2().child(self.pane(0, cx))))
                .child(resizable_panel().child(div().size_full().pl_2().child(self.pane(1, cx))))
                .into_any_element(),
            Layout::Single(i) => self.pane(i, cx).into_any_element(),
        };
        let legend = h_flex()
            .gap_4()
            .child(layers::legend_item("GPUI Kit host", cx.theme().border, cx))
            .child(layers::legend_item(
                "NSH app (TSX)",
                layers::nsh_colors(cx).0,
                cx,
            ))
            .child(layers::legend_item(
                "GPUI widget inside the NSH app",
                layers::widget_colors(cx).0,
                cx,
            ));
        h_flex()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(
                Sidebar::new("sidebar")
                    .w(px(220.))
                    .header(SidebarHeader::new().child("nsh in GPUI Kit"))
                    .child(SidebarGroup::new("Embedded apps").child(menu))
                    .child(SidebarGroup::new("Watchlist").child(watchlist))
                    .footer(orders),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .p_3()
                    .gap_3()
                    .child(toolbar)
                    .child(div().flex_1().min_h_0().child(content))
                    .when(shown, |d| d.child(legend)),
            )
    }
}

/// A new view running `path`'s bundle, in the host's theme.
fn open_app(path: &Path, window: &mut Window, cx: &mut App) -> Option<Entity<AppView>> {
    // SAFETY: the bundle is this app's own build output (`build.rs`), or
    // one its developer names, the same trust as the app's resources.
    let bundle = match unsafe { Bundle::read_trusted(path) } {
        Ok(bundle) => bundle,
        Err(err) => {
            log::error!("cannot read {}: {err}", path.display());
            return None;
        }
    };
    let options = AppOptions {
        // The build copies imported assets next to the bundle.
        asset_base: path
            .parent()
            .map(|dir| AssetBase::Directory(dir.join("assets"))),
        ..AppOptions::default()
    };
    let view = cx.new(|cx| AppView::new(options, window, cx));
    // The app's switch is GPUI Kit's, drawn by this app.
    view.read(cx)
        .register_component(switch::NAME, std::rc::Rc::new(switch::HostSwitch::make));
    view.read(cx).register_component(
        watchlist::NAME,
        std::rc::Rc::new(watchlist::Watchlist::make),
    );
    let theme = cx.theme();
    let (nsh_theme, appearance) = (theme::nsh_theme(theme), theme::appearance(theme.mode));
    view.update(cx, |view, cx| {
        // Before the app starts: its first frame is in the host's theme.
        view.set_theme(Some(nsh_theme), cx);
        view.set_appearance(appearance, cx);
        view.on_error(|message| log::error!("embedded app: {message}"));
        view.load(NshApp::Js(bundle));
    });
    Some(view)
}

/// The bundle to run: the argument (a `.nshc` or its directory), else
/// `app/` as `build.rs` built it.
fn bundle_path() -> PathBuf {
    let arg = std::env::args_os()
        .skip(1)
        .find(|a| a != "--alone")
        .map(PathBuf::from);
    let path = arg.unwrap_or_else(|| Path::new(env!("OUT_DIR")).join("app"));
    if path.is_dir() {
        path.join("bundle.nshc")
    } else {
        path
    }
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    // What the apps reach by name: NSH's UI set and the host's desk
    // (`nativeModule("host/trading")`), served by every surface.
    nsh_platform_gpui::set_ui_sets(vec![nsh_ui_mobile::registry(), trading::registry()]);
    let bundle = bundle_path();
    gpui_kit::application().with_assets(Assets).run(move |cx| {
        gpui_kit::init(cx);
        cx.set_global(layers::Layers(true));
        // nsh's theme format is gpui-kit's: a theme file with a name per
        // entry loads into GPUI Kit's registry as it is.
        if let Err(err) = gpui_kit::component::ThemeRegistry::global_mut(cx)
            .load_themes_from_str(include_str!("../themes/violet.json"))
        {
            log::error!("violet theme: {err}");
        }
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::centered(size(px(1280.), px(820.)), cx)),
            titlebar: Some(TitlebarOptions {
                title: Some("nsh in GPUI Kit".into()),
                ..Default::default()
            }),
            // GPUI draws unfocused windows at 30 fps by default, which
            // would halve the embedded apps' animations whenever another
            // window has the keyboard.
            inactive_frame_interval: None,
            ..Default::default()
        };
        if std::env::args().any(|a| a == "--alone") {
            // One surface filling a window with no host widgets: Tab wraps
            // around its stops, as in a standalone app.
            cx.open_window(options, |window, cx| {
                let view = open_app(&bundle, window, cx).expect("the bundle loads");
                window.focus(&view.focus_handle(cx), cx);
                view
            })
            .expect("a window opens");
        } else {
            gpui_kit::open_window(options, cx, |window, cx| {
                cx.new(|cx| Shell::new(bundle, window, cx))
            })
            .expect("a window opens");
        }
        cx.activate(true);
    });
}
