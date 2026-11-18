//! Running the managed Suwayomi-Server: headless, on 127.0.0.1 at a free
//! port, behind basic auth with credentials only Rust knows.
//!
//! Every setting is passed as `-Dsuwayomi.tachidesk.config.server.<key>=…`:
//! Suwayomi reads a system property of that name over its `server.conf`
//! (`SystemPropertyOverrideDelegate` in `ConfigModule.kt`), and `rootDir` is
//! only read that way (`ApplicationRootDir.kt`). Keys checked against the
//! `server-reference.conf` of v2.4.2366: `ip`, `port`, `systemTrayEnabled`,
//! `initialOpenInBrowserEnabled`, `webUIEnabled`, `kcefEnabled` (off: no
//! Chromium download), `authMode` (`BASIC_AUTH`), `authUsername`,
//! `authPassword`, `globalUpdateInterval` and `backupInterval` (`0` =
//! disabled).
//!
//! The arguments go in an argument file (`java @file`, written owner-only
//! and deleted once the server answers), so the password never shows in the
//! process list. What the server prints goes to `server.log`.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

const PROPERTY: &str = "-Dsuwayomi.tachidesk.config.server";

/// Everything a start needs.
#[derive(Clone, PartialEq, Eq)]
pub struct Launch {
    pub java: PathBuf,
    pub jar: PathBuf,
    /// The server's `rootDir`.
    pub root: PathBuf,
    pub port: u16,
    pub username: String,
    pub password: String,
}

impl std::fmt::Debug for Launch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Launch({:?}, port {})", self.jar.file_name(), self.port)
    }
}

/// The JVM's arguments, in order.
pub fn args(launch: &Launch) -> Vec<String> {
    let root = launch.root.to_string_lossy();
    let mut args = vec![
        // No AWT: no tray icon, no window, whatever the server tries.
        "-Djava.awt.headless=true".to_string(),
        format!("-Djava.io.tmpdir={}", launch.root.join("tmp").to_string_lossy()),
        format!("{PROPERTY}.rootDir={root}"),
    ];
    let settings: [(&str, String); 11] = [
        ("ip", "127.0.0.1".into()),
        ("port", launch.port.to_string()),
        ("systemTrayEnabled", "false".into()),
        ("initialOpenInBrowserEnabled", "false".into()),
        ("webUIEnabled", "false".into()),
        ("kcefEnabled", "false".into()),
        ("authMode", "BASIC_AUTH".into()),
        ("authUsername", launch.username.clone()),
        ("authPassword", launch.password.clone()),
        ("globalUpdateInterval", "0".into()),
        ("backupInterval", "0".into()),
    ];
    args.extend(settings.into_iter().map(|(key, value)| format!("{PROPERTY}.{key}={value}")));
    args.push("-jar".into());
    args.push(launch.jar.to_string_lossy().into_owned());
    args
}

/// One argument as the `java` launcher reads it from an `@file`: quoted,
/// with backslashes and quotes escaped (a Windows path keeps its
/// backslashes).
fn quote(arg: &str) -> String {
    format!("\"{}\"", arg.replace('\\', "\\\\").replace('"', "\\\""))
}

pub fn argfile(args: &[String]) -> String {
    args.iter().map(|arg| quote(arg)).collect::<Vec<_>>().join("\n") + "\n"
}

/// Writes `contents` readable by this user only.
fn write_private(path: &Path, contents: &str) -> std::io::Result<()> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(contents.as_bytes())
}

pub fn argfile_path(dir: &Path) -> PathBuf {
    dir.join("launch.args")
}

pub fn pid_path(dir: &Path) -> PathBuf {
    dir.join("server.pid")
}

pub fn log_path(dir: &Path) -> PathBuf {
    dir.join("server.log")
}

/// Starts the server, without a window. `dir` is `app_data_dir/suwayomi`.
pub fn spawn(dir: &Path, launch: &Launch) -> std::io::Result<Child> {
    std::fs::create_dir_all(launch.root.join("tmp"))?;
    let file = argfile_path(dir);
    write_private(&file, &argfile(&args(launch)))?;
    let log = std::fs::File::create(log_path(dir))?;
    let mut command = Command::new(&launch.java);
    command
        .arg(format!("@{}", file.to_string_lossy()))
        .current_dir(&launch.root)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log.try_clone()?))
        .stderr(Stdio::from(log));
    crate::platform::hide_child_window(&mut command);
    let child = command.spawn();
    if child.is_err() {
        let _ = std::fs::remove_file(&file);
    }
    let child = child?;
    let _ = std::fs::write(pid_path(dir), child.id().to_string());
    Ok(child)
}

/// A free port on 127.0.0.1 (the server binds it a moment later).
pub fn free_port() -> std::io::Result<u16> {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0))?;
    Ok(listener.local_addr()?.port())
}

/// A server left running by an app that did not close properly (a crash)
/// holds the database: it is stopped before another starts. Only a process
/// whose command line names our jar is touched.
pub fn stop_leftover(dir: &Path, jar_name: &str) {
    let path = pid_path(dir);
    let Some(pid) = std::fs::read_to_string(&path).ok().and_then(|raw| raw.trim().parse::<u32>().ok()) else { return };
    let pid = sysinfo::Pid::from_u32(pid);
    let mut system = sysinfo::System::new();
    system.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[pid]), true);
    if let Some(process) = system.process(pid) {
        let ours = process.cmd().iter().any(|arg| arg.to_string_lossy().contains("launch.args") || arg.to_string_lossy().contains(jar_name));
        if ours {
            log::info!("Suwayomi: stopping a server left from an earlier run");
            terminate(process);
        }
    }
    let _ = std::fs::remove_file(path);
}

fn terminate(process: &sysinfo::Process) {
    // A polite stop first (the JVM runs its shutdown hooks), then a hard one.
    if process.kill_with(sysinfo::Signal::Term).unwrap_or(false) {
        let started = std::time::Instant::now();
        let pid = process.pid();
        let mut system = sysinfo::System::new();
        while started.elapsed() < std::time::Duration::from_secs(5) {
            std::thread::sleep(std::time::Duration::from_millis(200));
            system.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[pid]), true);
            if system.process(pid).is_none() {
                return;
            }
        }
    }
    process.kill();
}

/// Stops a server this app started, and waits for it.
pub fn stop(dir: &Path, child: &mut Child) {
    if matches!(child.try_wait(), Ok(None)) {
        let pid = sysinfo::Pid::from_u32(child.id());
        let mut system = sysinfo::System::new();
        system.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[pid]), true);
        match system.process(pid) {
            Some(process) => terminate(process),
            None => {
                let _ = child.kill();
            }
        }
        let _ = child.kill();
    }
    let _ = child.wait();
    let _ = std::fs::remove_file(pid_path(dir));
    let _ = std::fs::remove_file(argfile_path(dir));
}

/// The last lines of `server.log`, for the app's own log when a start fails.
pub fn log_tail(dir: &Path, lines: usize) -> String {
    let text = std::fs::read_to_string(log_path(dir)).unwrap_or_default();
    let all: Vec<&str> = text.lines().collect();
    all[all.len().saturating_sub(lines)..].join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn launch() -> Launch {
        Launch {
            java: PathBuf::from("/data/suwayomi/jre/21/bin/java"),
            jar: PathBuf::from("/data/suwayomi/bin/Suwayomi-Server-v2.4.2366.jar"),
            root: PathBuf::from("/data/suwayomi/data"),
            port: 48123,
            username: "u0123".into(),
            password: "p4567".into(),
        }
    }

    #[test]
    fn the_server_starts_headless_on_loopback_behind_basic_auth() {
        let args = args(&launch());
        let has = |arg: &str| args.iter().any(|a| a == arg);
        assert!(has("-Djava.awt.headless=true"));
        assert!(has("-Dsuwayomi.tachidesk.config.server.rootDir=/data/suwayomi/data"));
        assert!(has("-Dsuwayomi.tachidesk.config.server.ip=127.0.0.1"));
        assert!(has("-Dsuwayomi.tachidesk.config.server.port=48123"));
        assert!(has("-Dsuwayomi.tachidesk.config.server.systemTrayEnabled=false"));
        assert!(has("-Dsuwayomi.tachidesk.config.server.initialOpenInBrowserEnabled=false"));
        assert!(has("-Dsuwayomi.tachidesk.config.server.webUIEnabled=false"));
        assert!(has("-Dsuwayomi.tachidesk.config.server.kcefEnabled=false"));
        assert!(has("-Dsuwayomi.tachidesk.config.server.authMode=BASIC_AUTH"));
        assert!(has("-Dsuwayomi.tachidesk.config.server.authUsername=u0123"));
        assert!(has("-Dsuwayomi.tachidesk.config.server.authPassword=p4567"));
        // `-jar <jar>` last: anything after it would go to the server, not the JVM.
        assert_eq!(&args[args.len() - 2..], ["-jar", "/data/suwayomi/bin/Suwayomi-Server-v2.4.2366.jar"]);
        // The password is not in what a log of the launch would show.
        assert!(!format!("{:?}", launch()).contains("p4567"));
    }

    #[test]
    fn the_argument_file_keeps_windows_paths_and_quotes() {
        let text = argfile(&["-Dx=C:\\Users\\A B\\data".to_string(), "say \"hi\"".to_string()]);
        assert_eq!(text, "\"-Dx=C:\\\\Users\\\\A B\\\\data\"\n\"say \\\"hi\\\"\"\n");
    }

    #[test]
    fn a_free_port_is_on_loopback() {
        let port = free_port().unwrap();
        assert!(port > 0);
        std::net::TcpListener::bind(("127.0.0.1", port)).unwrap();
    }
}
