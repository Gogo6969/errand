//! One thread, in a terminal, before there is a window to put it in.
//!
//! Not the product. It exists so the thing underneath the product can be
//! watched working before anybody spends a week on a window: type, watch it
//! narrate, type again while it is still going. If that reads well here it will
//! read well anywhere, and if it does not, no amount of window will save it.

use std::io::{BufRead, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::RecvTimeoutError;
use std::sync::Arc;
use std::time::{Duration, Instant};

use errand_core::{claude::Claude, Engine, Event};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let thread = uuid_v4();
    let here = std::env::current_dir()?;
    println!("errand · thread {thread}\ntype to talk to it; it can be talked to while it works. ctrl-d to leave.\n");

    let (mut claude, events) = Claude::open(&thread, &here)?;

    // Everything it says, as it says it, on its own thread so that typing is
    // never blocked by whatever it happens to be doing.
    let working = Arc::new(AtomicBool::new(false));
    let watching = working.clone();
    std::thread::spawn(move || loop {
        match events.recv_timeout(Duration::from_millis(250)) {
            Ok(event) => {
                if event.ends_the_turn() {
                    watching.store(false, Ordering::SeqCst);
                }
                show(&event);
            }
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => break,
        }
    });

    let stdin = std::io::stdin();
    loop {
        print!("› ");
        std::io::stdout().flush().ok();
        let mut line = String::new();
        if stdin.lock().read_line(&mut line)? == 0 {
            break;
        }
        let said = line.trim();
        if !said.is_empty() {
            working.store(true, Ordering::SeqCst);
            claude.say(said)?;
        }
    }

    // Leaving does not mean cutting off whatever was asked for a moment ago.
    // The input can end while a turn is still running -- ctrl-d, or a line
    // piped in from somewhere -- and killing it there would throw away work
    // that is already being paid for.
    let waited_from = Instant::now();
    while working.load(Ordering::SeqCst) && waited_from.elapsed() < Duration::from_secs(600) {
        std::thread::sleep(Duration::from_millis(100));
    }
    claude.stop()?;
    Ok(())
}

/// One event, as a person reads it.
fn show(event: &Event) {
    match event {
        Event::Started { model, .. } => println!("\n[{model}]"),
        Event::Said { text, settled } if *settled => println!("\n{text}"),
        Event::Said { .. } => {}
        Event::Doing(step) => println!("  · {}", step.what),
        Event::Did { outcome, .. } if !outcome.is_empty() => println!("    {outcome}"),
        Event::Did { .. } => {}
        Event::NeedsYou(ask) => println!("\n? {}", ask.asking),
        Event::Done { .. } => print!("\n› "),
        Event::Failed { why } => println!("\nit could not: {why}"),
    }
    std::io::stdout().flush().ok();
}

/// A session id, without taking a dependency for sixteen bytes of randomness.
fn uuid_v4() -> String {
    let mut b = [0u8; 16];
    getrandom(&mut b);
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: Vec<String> = b.iter().map(|x| format!("{x:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        h[0..4].concat(),
        h[4..6].concat(),
        h[6..8].concat(),
        h[8..10].concat(),
        h[10..16].concat()
    )
}

fn getrandom(into: &mut [u8]) {
    use std::io::Read;
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(into))
        .expect("the system's randomness");
}
