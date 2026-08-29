//! Does a picture actually reach the model?
//!
//! Both engines take an image in a different shape, and the shape is the whole
//! of it: Claude Code takes a base64 block on its pipe, and a local model takes
//! a data URL in the message. Neither complains about the wrong one in any way
//! a person would find. The turn simply comes back as though no picture had
//! been sent, which reads as a model that cannot see rather than a message that
//! never carried anything.
//!
//!     cargo test -p errand-core --test pictures_live -- --ignored --nocapture

use std::time::Duration;

use errand_core::local::{LlmSettings, Local};
use errand_core::{claude::Claude, Engine, Event, Picture};

/// A picture with one unmistakable thing in it, drawn rather than fetched so
/// the test does not depend on a file somebody might move.
fn a_red_square() -> Picture {
    // A 64x64 solid red PNG, built by hand so there is no image crate here.
    // Each row is a filter byte and then the pixels, which is the whole of the
    // PNG scanline format and the reason this needs no image crate.
    let mut row = vec![0u8];
    for _ in 0..64 {
        row.extend_from_slice(&[220, 30, 30]);
    }
    let raw: Vec<u8> = row.repeat(64);
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    let chunk = |kind: &[u8], data: &[u8]| {
        let mut out = (data.len() as u32).to_be_bytes().to_vec();
        let body: Vec<u8> = kind.iter().chain(data).copied().collect();
        out.extend_from_slice(&body);
        out.extend_from_slice(&crc32(&body).to_be_bytes());
        out
    };
    let mut ihdr = 64u32.to_be_bytes().to_vec();
    ihdr.extend_from_slice(&64u32.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    png.extend(chunk(b"IHDR", &ihdr));
    png.extend(chunk(b"IDAT", &deflate_stored(&raw)));
    png.extend(chunk(b"IEND", b""));

    Picture {
        kind: "image/png".into(),
        base64: base64(&png),
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "spends a Claude subscription; run with --ignored"]
async fn claude_code_can_see_a_picture_that_was_attached() {
    let here = std::env::temp_dir().join(format!("errand-pic-{}", std::process::id()));
    std::fs::create_dir_all(&here).unwrap();
    let id = uuid_ish();
    let (mut it, events) = Claude::open(
        &id,
        &here,
        errand_core::claude::PickUp::New,
        "auto",
        None,
        None,
        "",
    )
    .expect("starting claude");

    it.say(
        "What single colour fills this image? Answer with one word.",
        &[a_red_square()],
    )
    .expect("saying it with a picture");

    let said = gather(&events, 120);
    println!("it said: {said}");
    it.stop().ok();
    assert!(
        said.to_lowercase().contains("red"),
        "it did not see the picture. It said: {said}"
    );
    let _ = std::fs::remove_dir_all(&here);
}

#[tokio::test(flavor = "multi_thread")]
// Needs a vision model this machine can actually reach. Most local models
// cannot see, and one that cannot will answer this question confidently and
// wrongly, so the model is named rather than found:
//
//     ERRAND_MODEL=gemma-4-31b-it ERRAND_PROVIDER=lmstudio \
//     ERRAND_BASE_URL=http://192.168.1.92:1234 \
//     cargo test -p errand-core --test pictures_live -- --ignored --nocapture
#[ignore = "needs a local model with vision; run with --ignored"]
async fn a_local_model_can_see_a_picture_that_was_attached() {
    let here = std::env::temp_dir().join(format!("errand-pic-local-{}", std::process::id()));
    std::fs::create_dir_all(&here).unwrap();
    // Named rather than found, because most local models cannot see and one
    // that cannot will answer this question confidently and wrongly.
    let model =
        std::env::var("ERRAND_MODEL").expect("ERRAND_MODEL, and it has to be one that sees");
    let mut settings = LlmSettings {
        model,
        ..Default::default()
    };
    if let Ok(where_it_lives) = std::env::var("ERRAND_BASE_URL") {
        settings.base_url = where_it_lives;
    }
    if let Ok(kind) = std::env::var("ERRAND_PROVIDER") {
        settings.provider = kind;
    }
    let (mut it, events) = Local::open(settings, here.clone(), "auto", "", None).expect("opening");

    it.say(
        "What single colour fills this image? Answer with one word.",
        &[a_red_square()],
    )
    .expect("saying it with a picture");

    let said = gather(&events, 180);
    println!("it said: {said}");
    it.stop().ok();
    assert!(
        said.to_lowercase().contains("red"),
        "it did not see the picture. It said: {said}"
    );
    let _ = std::fs::remove_dir_all(&here);
}

/// Everything it said, until the turn ends.
fn gather(events: &std::sync::mpsc::Receiver<Event>, seconds: u64) -> String {
    let deadline = std::time::Instant::now() + Duration::from_secs(seconds);
    let mut said = String::new();
    while std::time::Instant::now() < deadline {
        match events.recv_timeout(Duration::from_secs(5)) {
            Ok(Event::Said {
                text,
                settled: true,
            }) => {
                said.push_str(&text);
                said.push('\n');
            }
            Ok(event) => {
                if event.ends_the_turn() {
                    break;
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(_) => break,
        }
    }
    said
}

// A few bytes of plumbing, so the test needs no dependency of its own.

fn base64(bytes: &[u8]) -> String {
    const ABC: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for lump in bytes.chunks(3) {
        let b = [
            lump[0],
            *lump.get(1).unwrap_or(&0),
            *lump.get(2).unwrap_or(&0),
        ];
        let n = u32::from(b[0]) << 16 | u32::from(b[1]) << 8 | u32::from(b[2]);
        for i in 0..4 {
            out.push(match i <= lump.len() {
                true => ABC[(n >> (18 - i * 6) & 63) as usize] as char,
                false => '=',
            });
        }
    }
    out
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for byte in data {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = match crc & 1 {
                1 => (crc >> 1) ^ 0xedb8_8320,
                _ => crc >> 1,
            };
        }
    }
    !crc
}

/// A zlib stream that stores rather than compresses, which is legal and short.
fn deflate_stored(raw: &[u8]) -> Vec<u8> {
    let mut out = vec![0x78, 0x01];
    for (at, lump) in raw.chunks(65535).enumerate() {
        let last = (at + 1) * 65535 >= raw.len();
        out.push(u8::from(last));
        out.extend_from_slice(&(lump.len() as u16).to_le_bytes());
        out.extend_from_slice(&(!(lump.len() as u16)).to_le_bytes());
        out.extend_from_slice(lump);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for byte in raw {
        a = (a + u32::from(*byte)) % 65521;
        b = (b + a) % 65521;
    }
    out.extend_from_slice(&((b << 16) | a).to_be_bytes());
    out
}

fn uuid_ish() -> String {
    let n = std::process::id();
    format!("aaaaaaaa-bbbb-4ccc-8ddd-{n:012x}")
}
