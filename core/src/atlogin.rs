//! Being there when the machine is.
//!
//! Everything this app does on its own it does while it is open. Routines fire
//! from a clock inside the process, watches look while the window is there, and
//! a background command dies with it. That is said out loud in each of those
//! places rather than hidden, and it is still the same sentence three times:
//! close the app and the standing jobs stop being standing.
//!
//! The whole fix is a machine that is not this one, and that is not built. This
//! is the part of it that is: the app comes back by itself after a restart, so
//! the only way the jobs stop is somebody deciding they should.
//!
//! A file, in the folder the system already reads at login. No daemon of our
//! own, nothing asking for a password, nothing that outlives being dragged to
//! the bin: an app that is not there cannot be started by a file that names it,
//! and the system quietly gives up, which is the right thing to happen.

use std::path::{Path, PathBuf};

/// What the file is called.
///
/// The bundle's own name, because this file belongs to that app and a second
/// Errand installed elsewhere should replace this entry rather than quietly
/// adding a second one that starts a second copy.
pub const NAMED: &str = "com.errandai.errand";

/// Where the system looks at login.
pub fn where_it_goes(home: &Path) -> PathBuf {
    home.join("Library")
        .join("LaunchAgents")
        .join(format!("{NAMED}.plist"))
}

/// What to start, given what is running now.
///
/// The bundle rather than the binary inside it, where there is one. Started by
/// its path, a bundled app runs without the things macOS hands an application
/// it launched properly, and the first surprise is that it has no name in the
/// menu bar. Under `cargo run` there is no bundle and the binary is the answer.
pub fn what_to_start(running: &Path) -> Vec<String> {
    // .../Errand.app/Contents/MacOS/errand-app -> .../Errand.app
    let bundle = running
        .ancestors()
        .find(|up| up.extension().is_some_and(|kind| kind == "app"));
    match bundle {
        Some(app) => vec![
            "/usr/bin/open".to_string(),
            "-a".to_string(),
            app.to_string_lossy().to_string(),
        ],
        None => vec![running.to_string_lossy().to_string()],
    }
}

/// The file itself.
///
/// Deliberately the smallest thing that works. `RunAtLoad` and nothing else: no
/// `KeepAlive`, because an app somebody quit should stay quit, and a window
/// that comes back every time it is closed is not a feature anybody asked for.
pub fn plist(start: &[String]) -> String {
    let arguments = start
        .iter()
        .map(|word| format!("    <string>{}</string>", escaped(word)))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>{NAMED}</string>
  <key>ProgramArguments</key>
  <array>
{arguments}
  </array>
  <key>RunAtLoad</key>
  <true/>
</dict>
</plist>
"#
    )
}

/// Start it at login, from wherever it is running now.
pub fn turn_on(home: &Path, running: &Path) -> std::io::Result<()> {
    let at = where_it_goes(home);
    if let Some(folder) = at.parent() {
        std::fs::create_dir_all(folder)?;
    }
    std::fs::write(at, plist(&what_to_start(running)))
}

/// Stop starting it at login.
///
/// A file that is already gone is the state being asked for, not a failure.
pub fn turn_off(home: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(where_it_goes(home)) {
        Err(gone) if gone.kind() == std::io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
}

/// Whether it starts at login, and whether it is this copy that would.
///
/// Read from the file rather than from anything we wrote down, because the file
/// is what the system obeys. A note in our own settings saying "on" while the
/// file says otherwise is a lie somebody would only find out about at a login.
pub fn how_it_stands(home: &Path, running: &Path) -> AtLogin {
    let Ok(found) = std::fs::read_to_string(where_it_goes(home)) else {
        return AtLogin::No;
    };
    match what_to_start(running)
        .iter()
        .all(|word| found.contains(&escaped(word)))
    {
        true => AtLogin::Yes,
        // The same app from somewhere else, or a copy that has since moved.
        // Worth telling apart: turning it on again is what fixes it, and
        // "it is already on" would be the one answer that does not.
        false => AtLogin::SomethingElse,
    }
}

/// What the file says, if anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AtLogin {
    Yes,
    No,
    /// It starts something at login, and it is not this copy.
    SomethingElse,
}

/// The three characters that would otherwise end the file early.
fn escaped(word: &str) -> String {
    word.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn somewhere(named: &str) -> PathBuf {
        let at = std::env::temp_dir().join(format!("errand-at-login-{named}"));
        let _ = std::fs::remove_dir_all(&at);
        std::fs::create_dir_all(at.join("Library").join("LaunchAgents")).expect("a folder");
        at
    }

    #[test]
    fn a_bundled_app_is_started_as_an_app_and_not_as_the_file_inside_it() {
        // Started by the path of the binary, a bundled app runs without the
        // things macOS hands an application it launched properly, and the first
        // surprise is that it has no name in the menu bar.
        let started = what_to_start(Path::new(
            "/Applications/Errand.app/Contents/MacOS/errand-app",
        ));
        assert_eq!(
            started,
            vec!["/usr/bin/open", "-a", "/Applications/Errand.app"]
        );
    }

    #[test]
    fn something_that_is_not_in_a_bundle_is_started_as_itself() {
        // What `cargo run` produces. Nothing about this feature should need a
        // release build to try.
        let started = what_to_start(Path::new("/Users/me/errand/target/debug/errand-app"));
        assert_eq!(started, vec!["/Users/me/errand/target/debug/errand-app"]);
    }

    #[test]
    fn turning_it_on_writes_a_file_the_system_will_read() {
        let home = somewhere("on");
        let me = Path::new("/Applications/Errand.app/Contents/MacOS/errand-app");
        turn_on(&home, me).expect("it wrote");

        let written = std::fs::read_to_string(where_it_goes(&home)).expect("a file");
        assert!(
            written.contains("<string>/Applications/Errand.app</string>"),
            "{written}"
        );
        assert!(written.contains("<key>RunAtLoad</key>"), "{written}");
        // An app somebody quit should stay quit. A window that comes back every
        // time it is closed is not a feature anybody asked for.
        assert!(!written.contains("KeepAlive"), "{written}");
        // Where the system actually looks.
        assert!(
            where_it_goes(&home).ends_with("Library/LaunchAgents/com.errandai.errand.plist"),
            "{:?}",
            where_it_goes(&home)
        );
    }

    #[test]
    fn what_it_says_is_read_from_the_file_and_not_from_anything_we_remember() {
        // The file is what the system obeys. A note in our own settings saying
        // "on" while the file says otherwise is a lie found out at a login.
        let home = somewhere("reading");
        let me = Path::new("/Applications/Errand.app/Contents/MacOS/errand-app");
        assert_eq!(how_it_stands(&home, me), AtLogin::No);

        turn_on(&home, me).expect("it wrote");
        assert_eq!(how_it_stands(&home, me), AtLogin::Yes);

        turn_off(&home).expect("it went");
        assert_eq!(how_it_stands(&home, me), AtLogin::No);
    }

    #[test]
    fn a_copy_somewhere_else_starting_at_login_is_not_this_one_being_on() {
        // Worth telling apart from off: turning it on again is what fixes it,
        // and "it is already on" is the one answer that would not.
        let home = somewhere("elsewhere");
        turn_on(
            &home,
            Path::new("/Users/me/Downloads/Errand.app/Contents/MacOS/errand-app"),
        )
        .expect("it wrote");
        assert_eq!(
            how_it_stands(
                &home,
                Path::new("/Applications/Errand.app/Contents/MacOS/errand-app")
            ),
            AtLogin::SomethingElse
        );
    }

    #[test]
    fn turning_it_off_when_it_is_already_off_is_not_a_failure() {
        // The state being asked for is the state it is in.
        let home = somewhere("off");
        turn_off(&home).expect("nothing to do");
        turn_off(&home).expect("still nothing to do");
    }

    #[test]
    fn a_path_with_something_in_it_that_would_end_the_file_early_does_not() {
        // An app in a folder called `Ben & Jerry`, which is a legal name for a
        // folder and not a legal thing to put in this file unescaped.
        let started = what_to_start(Path::new(
            "/Users/me/Ben & Jerry/Errand.app/Contents/MacOS/x",
        ));
        let written = plist(&started);
        assert!(written.contains("Ben &amp; Jerry"), "{written}");
        assert!(!written.contains("Ben & Jerry"), "{written}");
    }
}
