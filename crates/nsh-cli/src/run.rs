use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use anyhow::{Context as _, bail};
use serde::Deserialize;

use crate::{dev::DevSession, ui};

const APP_NAME: &str = "NshDev";
const BUNDLE_ID: &str = "com.longbridge.nsh.host";

/// Where `nsh run` runs the app.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub(crate) enum Target {
    /// The Mac Catalyst development host (an iPhone-sized window).
    Catalyst,
    /// The development host in an iOS simulator.
    Ios,
    /// The development host on an Android device or emulator.
    Android,
    /// The Linux (GTK) development host, in a window on this machine.
    Linux,
    /// The macOS (AppKit) development host, in a window on this machine.
    Macos,
    /// The Windows development host, in a window on this machine.
    Windows,
    /// The web development host, in the browser (the dev server serves its
    /// page).
    Web,
    /// The GPUI development host, in a window on this machine (macOS,
    /// Windows or Linux).
    Gpui,
}

impl Target {
    /// The default: the host that runs on this machine (Mac Catalyst on
    /// macOS, the GTK host on Linux, the Win32 host on Windows; Android
    /// elsewhere).
    pub(crate) fn native() -> Target {
        if cfg!(target_os = "macos") {
            Target::Catalyst
        } else if cfg!(target_os = "linux") {
            Target::Linux
        } else if cfg!(windows) {
            Target::Windows
        } else {
            Target::Android
        }
    }

    fn platform(self) -> crate::runtime::Platform {
        match self {
            Target::Catalyst => crate::runtime::Platform::MacCatalyst,
            Target::Linux => crate::runtime::Platform::Linux,
            Target::Macos => crate::runtime::Platform::MacOs,
            Target::Windows => crate::runtime::Platform::Windows,
            Target::Gpui => crate::runtime::Platform::Gpui,
            Target::Ios | Target::Android | Target::Web => {
                crate::runtime::Platform::IphoneSimulator
            }
        }
    }
}

/// What the development host shows besides the app.
#[derive(Debug, Clone, Copy)]
pub(crate) struct HostFlags {
    /// Log per-commit timings.
    pub(crate) trace: bool,
    /// The performance monitor (the macOS host's).
    pub(crate) fps: bool,
}

/// Starts a dev session and runs the app in a development host, which
/// loads bundles from the dev server and reloads on change.
pub(crate) fn run(
    entry: &Path,
    port: u16,
    release: bool,
    flags: HostFlags,
    target: Target,
    device: Option<&str>,
) -> anyhow::Result<()> {
    let HostFlags { trace, fps } = flags;
    if fps && target != Target::Macos {
        bail!("--fps needs --target macos (the other hosts have no performance monitor yet)");
    }
    let work = dunce::canonicalize(entry)?
        .parent()
        .context("entry has no directory")?
        .join(".nsh");
    if target == Target::Android {
        return run_android(entry, port, release, &work, trace);
    }
    if target == Target::Web {
        return run_web(entry, port, release);
    }
    // The host a previous session left running would connect to this one
    // until it is replaced.
    let udid = match target {
        Target::Ios => {
            let udid = simulator(device)?;
            let _ = simctl(&["terminate", &udid, BUNDLE_ID]);
            Some(udid)
        }
        Target::Catalyst
        | Target::Android
        | Target::Linux
        | Target::Macos
        | Target::Windows
        | Target::Web
        | Target::Gpui => None,
    };
    let session = DevSession::start(entry, port, release)?;
    session.print_urls();
    let host = crate::runtime::file(target.platform(), false)?;
    let mut args = vec!["--dev-server".to_owned(), session.local_url()];
    if trace {
        args.push("--trace".to_owned());
    }
    if fps {
        args.push("--fps".to_owned());
    }
    match target {
        Target::Catalyst => {
            let app = package_catalyst(&work, &host)?;
            let exe = app.join("Contents/MacOS/nsh-ios-host");
            // Its own output (AppKit's notes) is kept for when it fails.
            let mut command = Command::new(&exe);
            command
                .args(&args)
                // A host that crashed must not ask to reopen its windows
                // next time (a modal dialog that blocks the window).
                .args(["-ApplePersistenceIgnoreState", "YES"]);
            run_local(&session, &mut command, "the Mac Catalyst host")
        }
        // Run in place: unlike Catalyst's, the GTK host needs no app
        // bundle around it.
        Target::Linux => {
            let mut command = Command::new(&host);
            command.args(&args);
            run_local(&session, &mut command, "the Linux host")
        }
        // In place too: the AppKit host makes itself a regular app (menu
        // bar, Dock icon) without an app bundle around it.
        Target::Macos => {
            // The menu bar and the window show the app's name.
            let entry = entry.canonicalize()?;
            let config = crate::package::read_config(&entry).unwrap_or_default();
            let (name, _) = crate::package::name_and_id(&entry, &config);
            let mut command = Command::new(&host);
            command
                .args(&args)
                .args(["--name", &name])
                .args(["-ApplePersistenceIgnoreState", "YES"]);
            run_local(&session, &mut command, "the macOS host")
        }
        Target::Windows => {
            let mut command = Command::new(&host);
            command.args(&args);
            run_local(&session, &mut command, "the Windows host")
        }
        // In place on every OS: GPUI makes the process a regular app (on
        // macOS too) without an app bundle around it.
        Target::Gpui => {
            let mut command = Command::new(&host);
            command.args(&args);
            run_local(&session, &mut command, "the GPUI host")
        }
        Target::Ios => {
            let app = package_ios(&work, &host)?;
            let udid = udid.expect("the simulator is picked before the session");
            let step = ui::Step::start("installing the development host in the simulator");
            simctl(&["install", &udid, &app.to_string_lossy()])?;
            step.finish();
            let mut launch = vec!["launch", "--terminate-running-process", &udid, BUNDLE_ID];
            launch.extend(args.iter().map(String::as_str));
            let step = ui::Step::start("launching the development host");
            simctl(&launch)?;
            step.finish();
            // The simulator shares the Mac's network: 127.0.0.1 is the dev
            // server, so a host left running would join the next session
            // of any app: Ctrl-C stops it too.
            let stoppable = ui::Stoppable::new();
            let result = session.watch(|| Ok(false));
            let _ = simctl(&["terminate", &udid, BUNDLE_ID]);
            drop(stoppable);
            result
        }
        Target::Android | Target::Web => unreachable!("handled above"),
    }
}

/// Starts a dev session that serves the web host and opens it in the
/// browser; every browser that opens the page (on this machine or, at the
/// LAN address, on a phone) runs the app until the session ends.
fn run_web(entry: &Path, port: u16, release: bool) -> anyhow::Result<()> {
    // Built first in a checkout: the session serves the runtime it finds.
    crate::runtime::web(crate::runtime::WebKind::Dev, true)?.context("no web runtime")?;
    let session = DevSession::start(entry, port, release)?;
    session.print_urls();
    // After a change of the app's Rust code the page that is open loads
    // the new runtime itself (`web::dev_host`).
    if !crate::rust_app::restarted() {
        crate::web::open_browser(&session.web_url());
    }
    session.watch(|| Ok(false))
}

/// Runs `command`, a host in a window on this machine, until its window
/// closes or the session ends; `what` names it when it fails.
fn run_local(session: &DevSession, command: &mut Command, what: &str) -> anyhow::Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        // Out of the console's Ctrl-C: this command stops the host itself,
        // so that a stop is not taken for the host failing.
        command.creation_flags(windows_sys::Win32::System::Threading::CREATE_NEW_PROCESS_GROUP);
    }
    // Its own output is kept for when it fails.
    let (mut child, output) = ui::quiet(command)
        .with_context(|| format!("launch {}", command.get_program().to_string_lossy()))?;
    // Windows ends a child with its parent only in a job: the host goes
    // with this command however it ends (closed console, killed).
    #[cfg(windows)]
    let _job = KillOnClose::assign(&child);
    // A host left running would open a second window next time: Ctrl-C and
    // SIGTERM close it too.
    let stoppable = ui::Stoppable::new();
    let result = session.watch(|| match child.try_wait()? {
        Some(status) if status.success() => Ok(true),
        Some(status) => Err(ui::failure(
            format!("{what} exited with {status}"),
            output.text(),
        )),
        None => Ok(false),
    });
    let _ = child.kill();
    let _ = child.wait();
    drop(stoppable);
    result
}

/// A job object that ends its processes when it closes: when dropped, or
/// with this process, however it ends.
#[cfg(windows)]
pub(crate) struct KillOnClose(windows_sys::Win32::Foundation::HANDLE);

#[cfg(windows)]
impl KillOnClose {
    /// Puts `child` in a new such job; `None` (the child then outlives a
    /// killed parent, as without it) if Windows refuses.
    pub(crate) fn assign(child: &std::process::Child) -> Option<KillOnClose> {
        use std::os::windows::io::AsRawHandle as _;

        use windows_sys::Win32::System::JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
            SetInformationJobObject,
        };

        // SAFETY: an unnamed job with default security; the handle is
        // owned by the returned value, which closes it.
        let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if job.is_null() {
            return None;
        }
        let job = KillOnClose(job);
        // SAFETY: zeroed is a valid "no limits" value of this plain C
        // structure.
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        // SAFETY: `info` is the structure the information class names, with
        // its size; `child`'s handle is open while it is borrowed.
        let ok = unsafe {
            SetInformationJobObject(
                job.0,
                JobObjectExtendedLimitInformation,
                (&raw const info).cast(),
                std::mem::size_of_val(&info) as u32,
            ) != 0
                && AssignProcessToJobObject(job.0, child.as_raw_handle()) != 0
        };
        ok.then_some(job)
    }
}

#[cfg(windows)]
impl Drop for KillOnClose {
    fn drop(&mut self) {
        // SAFETY: the job handle this value owns, closed once.
        unsafe { windows_sys::Win32::Foundation::CloseHandle(self.0) };
    }
}

/// Starts a dev session, builds and installs the Android dev host (when
/// it changed), forwards the dev server port to the device and starts the
/// host.
fn run_android(
    entry: &Path,
    port: u16,
    release: bool,
    work: &Path,
    trace: bool,
) -> anyhow::Result<()> {
    use crate::android::{self, Apk, Signing};
    let sdk = android::Sdk::find()?;
    let serial = android::device(&sdk)?;
    // As on iOS: the host of a previous session must not connect first.
    let _ = android::adb(&sdk, &serial, &["shell", "am", "force-stop", BUNDLE_ID]);
    let session = DevSession::start(entry, port, release)?;
    session.print_urls();
    let runtime = crate::runtime::android(true)?;
    let apk_path = work.join("android/NshDev.apk");
    fs::create_dir_all(apk_path.parent().context("apk dir")?)?;
    let stamp = work.join("android/runtime.stamp");
    // Every app's development host is the same package: another app's run
    // (an app with its own Rust crate has its own runtime) or another
    // device replaces what this app installed. The device's install time
    // tells whether the host there is still the one this app installed.
    let installed = work.join("android/installed");
    let version = runtime_version(&runtime)?;
    let built =
        apk_path.exists() && fs::read_to_string(&stamp).ok().as_deref() == Some(version.as_str());
    if !built {
        let step = ui::Step::start("building the Android development host");
        android::build_apk(
            &sdk,
            &Apk {
                package: BUNDLE_ID,
                label: APP_NAME,
                version_name: "0.1.0",
                version_code: 1,
                activity: "com.longbridge.nsh.DevActivity",
                dev: true,
                dark: false,
                runtime: &runtime,
                icon: None,
                assets: Vec::new(),
                signing: Signing::Debug,
            },
            &apk_path,
        )?;
        step.done("built the Android development host");
        // Until it is installed, no device has this build.
        let _ = fs::remove_file(&installed);
        fs::write(&stamp, &version)?;
    }
    let on_device =
        |sdk: &android::Sdk| installed_host(sdk, &serial).map(|time| format!("{serial} {time}"));
    let current = fs::read_to_string(&installed).ok();
    if current.is_none() || current != on_device(&sdk) {
        let step = ui::Step::start(format!("installing it on {serial}"));
        android::adb(
            &sdk,
            &serial,
            &["install", "-r", "-t", &apk_path.to_string_lossy()],
        )?;
        step.done(format!(
            "installed the Android development host on {serial}"
        ));
        match on_device(&sdk) {
            Some(record) => fs::write(&installed, record)?,
            None => {
                let _ = fs::remove_file(&installed);
            }
        }
    }
    // The device reaches the dev server on its own 127.0.0.1, until the
    // run ends.
    let _reverse = android::Reverse::add(&sdk, &serial, session.port())?;
    let url = session.local_url();
    let mut start = vec![
        "shell",
        "am",
        "start",
        "-S",
        "-n",
        "com.longbridge.nsh.host/com.longbridge.nsh.DevActivity",
        "--es",
        "url",
        &url,
    ];
    if trace {
        start.extend(["--ez", "trace", "true"]);
    }
    let step = ui::Step::start("launching the development host");
    android::adb(&sdk, &serial, &start)?;
    step.finish();
    let result = session.watch(|| Ok(false));
    let _ = android::adb(&sdk, &serial, &["shell", "am", "force-stop", BUNDLE_ID]);
    result
}

/// When the development host on device `serial` was installed or last
/// updated (`dumpsys package`), if it is there.
fn installed_host(sdk: &crate::android::Sdk, serial: &str) -> Option<String> {
    let text =
        crate::android::adb(sdk, serial, &["shell", "dumpsys", "package", BUNDLE_ID]).ok()?;
    text.lines()
        .find_map(|line| line.trim().strip_prefix("lastUpdateTime="))
        .map(str::to_owned)
}

/// Identifies a runtime build: the sizes and times of its files.
fn runtime_version(runtime: &Path) -> anyhow::Result<String> {
    let mut parts = Vec::new();
    let mut files = vec![runtime.join("classes.dex")];
    for entry in fs::read_dir(runtime.join("lib"))? {
        files.push(entry?.path().join("libnsh.so"));
    }
    for file in files {
        if let Ok(meta) = fs::metadata(&file) {
            let modified = meta
                .modified()?
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            parts.push(format!("{}:{}:{modified}", file.display(), meta.len()));
        }
    }
    Ok(parts.join("\n"))
}

/// Creates (or refreshes) `<work>/NshDev.app` around the host binary.
fn package_catalyst(work: &Path, host: &Path) -> anyhow::Result<PathBuf> {
    let app = work.join(format!("{APP_NAME}.app"));
    let contents = app.join("Contents");
    let macos = contents.join("MacOS");
    fs::create_dir_all(&macos)?;
    fs::write(contents.join("Info.plist"), info_plist(Target::Catalyst))?;
    install_binary(host, &macos.join("nsh-ios-host"))?;
    Ok(app)
}

/// Creates (or refreshes) `<work>/ios/NshDev.app`, a flat iOS app bundle,
/// signed ad hoc (the simulator requires a signature).
fn package_ios(work: &Path, host: &Path) -> anyhow::Result<PathBuf> {
    let app = work.join("ios").join(format!("{APP_NAME}.app"));
    fs::create_dir_all(&app)?;
    fs::write(app.join("Info.plist"), info_plist(Target::Ios))?;
    if install_binary(host, &app.join("nsh-ios-host"))? || !app.join("_CodeSignature").exists() {
        let status = Command::new("codesign")
            .args(["--force", "--sign", "-", "--timestamp=none"])
            .arg(&app)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .context("run codesign")?;
        if !status.success() {
            bail!("codesign failed for {}", app.display());
        }
    }
    Ok(app)
}

/// Copies the host binary if it changed. Returns true if it did.
fn install_binary(host: &Path, dest: &Path) -> anyhow::Result<bool> {
    if !is_stale(host, dest)? {
        return Ok(false);
    }
    // Copy then rename: overwriting in place would modify the file a
    // running host (another session) maps, and the kernel kills processes
    // whose code no longer matches its cached signature.
    let tmp = dest.with_extension("tmp");
    fs::copy(host, &tmp).with_context(|| format!("copy {}", host.display()))?;
    fs::rename(&tmp, dest)?;
    Ok(true)
}

fn is_stale(src: &Path, dest: &Path) -> anyhow::Result<bool> {
    let Ok(dest_meta) = fs::metadata(dest) else {
        return Ok(true);
    };
    let src_meta = fs::metadata(src)?;
    Ok(src_meta.len() != dest_meta.len() || src_meta.modified()? > dest_meta.modified()?)
}

pub(crate) fn simctl(args: &[&str]) -> anyhow::Result<String> {
    let output = Command::new("xcrun")
        .arg("simctl")
        .args(args)
        .output()
        .context("run xcrun simctl (is Xcode installed?)")?;
    if !output.status.success() {
        bail!(
            "simctl {} failed: {}",
            args.first().unwrap_or(&""),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[derive(Deserialize)]
struct DeviceList {
    devices: std::collections::BTreeMap<String, Vec<Device>>,
}

#[derive(Deserialize)]
struct Device {
    udid: String,
    name: String,
    state: String,
    #[serde(rename = "isAvailable", default)]
    available: bool,
}

/// Picks a simulator (`wanted` by name or UDID; else a booted one; else the
/// newest iPhone), boots it and shows the Simulator app.
pub(crate) fn simulator(wanted: Option<&str>) -> anyhow::Result<String> {
    let list: DeviceList =
        serde_json::from_str(&simctl(&["list", "devices", "available", "--json"])?)?;
    // Runtimes sort by version within a platform; newest last.
    let mut devices: Vec<&Device> = list
        .devices
        .iter()
        .filter(|(runtime, _)| runtime.contains("SimRuntime.iOS"))
        .flat_map(|(_, devices)| devices)
        .filter(|d| d.available)
        .collect();
    devices.reverse();
    let device = match wanted {
        Some(w) => devices
            .iter()
            .find(|d| d.udid == w || d.name == w)
            .with_context(|| format!("no available simulator named {w}"))?,
        None => devices
            .iter()
            .find(|d| d.state == "Booted")
            .or_else(|| {
                devices
                    .iter()
                    .find(|d| d.name.starts_with("iPhone") && d.name.contains("Pro"))
            })
            .or_else(|| devices.iter().find(|d| d.name.starts_with("iPhone")))
            .context(
                "no iOS simulator available: install one with `xcodebuild -downloadPlatform iOS`",
            )?,
    };
    let booting =
        (device.state != "Booted").then(|| ui::Step::start(format!("booting {}", device.name)));
    if booting.is_some() {
        simctl(&["boot", &device.udid])?;
    }
    // Apps launched before the boot completes are sent to the background
    // when SpringBoard comes up.
    simctl(&["bootstatus", &device.udid, "-b"])?;
    if let Some(step) = booting {
        step.done(format!("booted {}", device.name));
    }
    show_simulators();
    Ok(device.udid.clone())
}

/// Brings up the simulators' window: Simulator.app, or DeviceHub.app in
/// Xcode versions that replaced it.
fn show_simulators() {
    let quiet = |cmd: &mut Command| {
        cmd.stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    };
    if quiet(Command::new("open").args(["-b", "com.apple.iphonesimulator"])) {
        return;
    }
    let developer = Command::new("xcode-select").arg("-p").output();
    if let Ok(out) = developer {
        let developer = String::from_utf8_lossy(&out.stdout);
        let hub = Path::new(developer.trim()).join("../Applications/DeviceHub.app");
        if hub.exists() && quiet(Command::new("open").arg(&hub)) {
            return;
        }
    }
    ui::warn("no Simulator or DeviceHub app found: the simulator runs without a window");
}

fn info_plist(target: Target) -> String {
    let platform = match target {
        // 6 = Mac idiom ("Optimize for Mac"): 1:1 points instead of the 77%
        // iPad scaling.
        Target::Catalyst => {
            "<key>UIDeviceFamily</key><array><integer>6</integer></array>\n    \
             <key>LSMinimumSystemVersion</key><string>11.0</string>"
        }
        // A launch screen entry opts into the full screen (no letterboxing).
        Target::Ios
        | Target::Android
        | Target::Linux
        | Target::Macos
        | Target::Windows
        | Target::Web
        | Target::Gpui => {
            "<key>UIDeviceFamily</key><array><integer>1</integer><integer>2</integer></array>\n    \
             <key>MinimumOSVersion</key><string>15.0</string>\n    \
             <key>CFBundleSupportedPlatforms</key><array><string>iPhoneSimulator</string></array>\n    \
             <key>UILaunchScreen</key><dict/>\n    \
             <key>NSAppTransportSecurity</key><dict><key>NSAllowsLocalNetworking</key><true/></dict>"
        }
    };
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key><string>nsh-ios-host</string>
    <key>CFBundleIdentifier</key><string>{BUNDLE_ID}</string>
    <key>CFBundleName</key><string>{APP_NAME}</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>0.1.0</string>
    <key>CFBundleVersion</key><string>1</string>
    <key>UIApplicationSupportsMultipleScenes</key><false/>
    {platform}
</dict>
</plist>
"#
    )
}

#[cfg(test)]
mod tests {
    use clap::ValueEnum as _;

    use super::Target;

    #[test]
    fn gpui_is_a_target_with_its_own_host() {
        let target = Target::from_str("gpui", false).unwrap();
        assert_eq!(target, Target::Gpui);
        assert_eq!(target.platform().binary(false), "nsh-gpui-host");
    }
}
