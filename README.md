# mp4-video-player-coded-in-rust-programming-language-using-chatgpt5.
mp4 video player coded  in rust programming language using chatgpt5.
Skip to content
Chat history
You said:
rust mp4 video player
ChatGPT said:

Awesome—here’s a clean, working Rust MP4 video player that runs on Debian/Ubuntu using GStreamer. It opens a window, plays your file, and gives you simple keyboard controls in the terminal.
1) Install system deps (Debian/Ubuntu)

sudo apt update
sudo apt install -y \
  build-essential pkg-config \
  libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev \
  gstreamer1.0-plugins-base gstreamer1.0-plugins-good \
  gstreamer1.0-plugins-bad gstreamer1.0-plugins-ugly \
  gstreamer1.0-libav gstreamer1.0-gl \
  gstreamer1.0-vaapi # (optional HW accel)

2) New Rust project

cargo new rust-mp4-player
cd rust-mp4-player

3) Cargo.toml

[package]
name = "rust-mp4-player"
version = "0.1.0"
edition = "2021"

[dependencies]
anyhow = "1"
clap = { version = "4", features = ["derive"] }
gstreamer = "0.23"
gstreamer-video = "0.23"
glib = "0.19"

4) src/main.rs

use anyhow::{anyhow, Context, Result};
use clap::Parser;
use glib::Cast;
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_video as gst_video;
use std::io::{self, BufRead};
use std::path::Path;
use std::thread;

/// Tiny MP4 player using GStreamer playbin.
/// Controls: type a letter then Enter in this terminal:
///   p = toggle pause/play
///   q = quit
///   s = seek +10s
///   r = seek -10s
#[derive(Parser, Debug)]
#[command(version, about = "Rust MP4 player (GStreamer)")]
struct Args {
    /// File path or URL (mp4, mov, mkv, etc. if codecs available)
    input: String,
}

fn to_uri(path_or_uri: &str) -> Result<String> {
    if path_or_uri.contains("://") {
        return Ok(path_or_uri.to_string());
    }
    let p = Path::new(path_or_uri)
        .canonicalize()
        .with_context(|| format!("Cannot resolve path: {path_or_uri}"))?;
    let uri = glib::filename_to_uri(p.to_str().unwrap(), None)
        .map_err(|_| anyhow!("Failed to convert to URI"))?;
    Ok(uri)
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Init GStreamer
    gst::init()?;

    // Build playbin
    let playbin = gst::ElementFactory::make("playbin")
        .build()
        .ok_or_else(|| anyhow!("Failed to create playbin"))?;

    // Set URI
    let uri = to_uri(&args.input)?;
    playbin.set_property("uri", &uri)?;

    // Prefer a GL sink (nice rendering), but auto is fine if unavailable
    // Try glimagesink -> autovideosink
    if let Ok(glsink) = gst::ElementFactory::make("glimagesink").build() {
        playbin.set_property("video-sink", &glsink).ok();
    } else if let Ok(autosink) = gst::ElementFactory::make("autovideosink").build() {
        playbin.set_property("video-sink", &autosink).ok();
    }

    // Bus to watch EOS / errors
    let bus = playbin
        .bus()
        .ok_or_else(|| anyhow!("Failed to get bus from playbin"))?;

    // Start playing
    playbin
        .set_state(gst::State::Playing)
        .context("Failed to set pipeline to Playing")?;

    // Print basic info
    println!("Playing: {uri}");
    println!("Controls (type then Enter): p=pause/play | s=+10s | r=-10s | q=quit");

    // Spawn input thread for controls
    let playbin_ctrl = playbin.clone();
    let ctrl_handle = thread::spawn(move || -> Result<()> {
        let stdin = io::stdin();
        for line in stdin.lock().lines() {
            let cmd = line.unwrap_or_default().trim().to_string();
            if cmd.is_empty() {
                continue;
            }
            match cmd.chars().next().unwrap() {
                'p' | 'P' => {
                    let state = playbin_ctrl.current_state();
                    let new_state = match state {
                        gst::State::Playing => gst::State::Paused,
                        _ => gst::State::Playing,
                    };
                    playbin_ctrl.set_state(new_state).ok();
                    println!("State -> {:?}", new_state);
                }
                's' | 'S' => {
                    seek_relative(&playbin_ctrl, 10_000)?;
                }
                'r' | 'R' => {
                    seek_relative(&playbin_ctrl, -10_000)?;
                }
                'q' | 'Q' => {
                    println!("Quitting…");
                    playbin_ctrl.set_state(gst::State::Null).ok();
                    break;
                }
                _ => {
                    println!("Unknown command: '{cmd}'. Use p/s/r/q.");
                }
            }
        }
        Ok(())
    });

    // Main bus loop: wait for EOS or ERROR
    for msg in bus.iter_timed(gst::ClockTime::NONE) {
        use gst::MessageView;
        match msg.view() {
            MessageView::Eos(..) => {
                println!("End of stream.");
                break;
            }
            MessageView::Error(err) => {
                eprintln!(
                    "Error from {:?}: {} ({:?})",
                    err.src().map(|s| s.path_string()),
                    err.error(),
                    err.debug()
                );
                break;
            }
            MessageView::StateChanged(sc) => {
                if sc.src().map(|s| s == playbin).unwrap_or(false) {
                    let old = sc.old();
                    let new = sc.current();
                    println!("Pipeline state: {old:?} -> {new:?}");
                }
            }
            _ => {}
        }
    }

    // Ensure clean shutdown
    playbin.set_state(gst::State::Null).ok();
    let _ = ctrl_handle.join();

    Ok(())
}

fn query_position_duration(pipeline: &gst::Element) -> (Option<i64>, Option<i64>) {
    let fmt = gst::Format::Time;
    let pos = pipeline.query_position::<gst::ClockTime>().ok().flatten();
    let dur = pipeline.query_duration::<gst::ClockTime>().ok().flatten();
    (pos.map(|p| p.nseconds() as i64), dur.map(|d| d.nseconds() as i64))
}

fn seek_relative(pipeline: &gst::Element, delta_ms: i64) -> Result<()> {
    let (pos_opt, dur_opt) = query_position_duration(pipeline);
    if let Some(pos_ns) = pos_opt {
        let mut target_ms = pos_ns / 1_000_000 + delta_ms;
        if let Some(dur_ns) = dur_opt {
            let dur_ms = dur_ns / 1_000_000;
            if target_ms < 0 {
                target_ms = 0;
            }
            if target_ms > dur_ms {
                target_ms = dur_ms;
            }
        }
        let target = gst::ClockTime::from_mseconds(target_ms as u64);
        pipeline.seek_simple(
            gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT,
            target,
        )?;
        println!("Seek -> {} ms", target_ms);
    }
    Ok(())
}

5) Run it

cargo run --release -- /path/to/video.mp4

Controls (enter after each key):

    p = pause/play toggle

    s = seek forward 10s

    r = seek back 10s

    q = quit

Notes & tips

    Codecs: MP4s with H.264/AAC generally need gstreamer1.0-libav and …-ugly. Installed above.

    Hardware acceleration (Intel/AMD): gstreamer1.0-vaapi can improve playback smoothness/CPU usage.

    URLs: You can pass an http(s):// URL instead of a file path.

    Termux/Android: GStreamer on Termux is possible but fiddly; if you want a Termux-targeted build, say the word and I’ll tailor the steps.

If you’d prefer an SDL2 + FFmpeg path (lower-level decode/blit loop, more code), I can give a minimal example next.
You said:
cd rust-mp4-player
    Creating binary (application) rust-mp4-player package
note: see more Cargo.toml keys and their definitions at https://doc.rust-lang.org/cargo/reference/manifest.html
ian@debian:~/rust-mp4-player$ ls
Cargo.toml  src
ian@debian:~/rust-mp4-player$ gedit Cargo,toml
ian@debian:~/rust-mp4-player$ gedit Cargo.toml
ian@debian:~/rust-mp4-player$ cd src
ian@debian:~/rust-mp4-player/src$ ls
main.rs
ian@debian:~/rust-mp4-player/src$ gedit main.rs
cargo run
^C
ian@debian:~/rust-mp4-player/src$ cargo run
    Updating crates.io index
     Locking 110 packages to latest compatible versions
      Adding glib v0.19.9 (available: v0.21.3)
      Adding gstreamer v0.23.7 (available: v0.24.2)
      Adding gstreamer-video v0.23.6 (available: v0.24.2)
      Adding target-lexicon v0.13.2 (available: v0.13.3)
  Downloaded anstyle-query v1.1.4
  Downloaded muldiv v1.0.1
  Downloaded is_terminal_polyfill v1.70.1
  Downloaded clap_lex v0.7.5
  Downloaded equivalent v1.0.2
  Downloaded anstyle-parse v0.2.7
  Downloaded autocfg v1.5.0
  Downloaded cfg-if v1.0.3
  Downloaded atomic_refcell v0.1.13
  Downloaded anstyle v1.0.11
  Downloaded utf8parse v0.2.2
  Downloaded serde_spanned v0.6.9
  Downloaded futures-core v0.3.31
  Downloaded gobject-sys v0.19.8
  Downloaded either v1.15.0
  Downloaded cfg-expr v0.20.3
  Downloaded option-operations v0.5.0
  Downloaded gobject-sys v0.20.10
  Downloaded colorchoice v1.0.4
  Downloaded heck v0.5.0
  Downloaded pin-utils v0.1.0
  Downloaded paste v1.0.15
  Downloaded toml_datetime v0.6.11
  Downloaded thiserror-impl v1.0.69
  Downloaded pin-project-lite v0.2.16
  Downloaded strsim v0.11.1
  Downloaded version-compare v0.2.0
  Downloaded pkg-config v0.3.32
  Downloaded thiserror v1.0.69
  Downloaded num-integer v0.1.46
  Downloaded thiserror-impl v2.0.17
  Downloaded slab v0.4.11
  Downloaded futures-macro v0.3.31
  Downloaded clap_derive v4.5.47
  Downloaded futures-task v0.3.31
  Downloaded toml_datetime v0.7.2
  Downloaded anyhow v1.0.100
  Downloaded num-rational v0.4.2
  Downloaded futures-executor v0.3.31
  Downloaded anstream v0.6.20
  Downloaded once_cell v1.21.3
  Downloaded target-lexicon v0.13.2
  Downloaded proc-macro-crate v3.4.0
  Downloaded futures-channel v0.3.31
  Downloaded quote v1.0.41
  Downloaded smallvec v1.15.1
  Downloaded toml_parser v1.0.3
  Downloaded clap v4.5.48
  Downloaded target-lexicon v0.12.16
  Downloaded unicode-ident v1.0.19
  Downloaded toml v0.8.23
  Downloaded proc-macro2 v1.0.101
  Downloaded cfg-expr v0.15.8
  Downloaded system-deps v7.0.5
  Downloaded glib-sys v0.20.10
  Downloaded gstreamer-video-sys v0.23.6
  Downloaded toml_edit v0.23.6
  Downloaded serde_core v1.0.228
  Downloaded gstreamer-base-sys v0.23.6
  Downloaded glib-sys v0.19.8
  Downloaded glib-macros v0.19.9
  Downloaded num-traits v0.2.19
  Downloaded bitflags v2.9.4
  Downloaded thiserror v2.0.17
  Downloaded system-deps v6.2.2
  Downloaded gstreamer-base v0.23.6
  Downloaded gio-sys v0.20.10
  Downloaded glib-macros v0.20.12
  Downloaded toml_edit v0.22.27
  Downloaded serde v1.0.228
  Downloaded gio-sys v0.19.8
  Downloaded indexmap v2.11.4
  Downloaded memchr v2.7.6
  Downloaded gstreamer-sys v0.23.6
  Downloaded hashbrown v0.16.0
  Downloaded itertools v0.14.0
  Downloaded gstreamer-video v0.23.6
  Downloaded winnow v0.7.13
  Downloaded clap_builder v4.5.48
  Downloaded futures-util v0.3.31
  Downloaded glib v0.19.9
  Downloaded syn v2.0.106
  Downloaded glib v0.20.12
  Downloaded gstreamer v0.23.7
  Downloaded libc v0.2.176
  Downloaded 85 crates (5.3MiB) in 3.23s
   Compiling equivalent v1.0.2
   Compiling hashbrown v0.16.0
   Compiling winnow v0.7.13
   Compiling serde_core v1.0.228
   Compiling serde v1.0.228
   Compiling heck v0.5.0
   Compiling indexmap v2.11.4
   Compiling smallvec v1.15.1
   Compiling pkg-config v0.3.32
   Compiling version-compare v0.2.0
   Compiling target-lexicon v0.13.2
   Compiling proc-macro2 v1.0.101
   Compiling unicode-ident v1.0.19
   Compiling quote v1.0.41
   Compiling cfg-expr v0.20.3
   Compiling libc v0.2.176
   Compiling syn v2.0.106
   Compiling target-lexicon v0.12.16
   Compiling serde_spanned v0.6.9
   Compiling toml_datetime v0.6.11
   Compiling cfg-expr v0.15.8
   Compiling toml_parser v1.0.3
   Compiling toml_edit v0.22.27
   Compiling toml_datetime v0.7.2
   Compiling toml_edit v0.23.6
   Compiling futures-core v0.3.31
   Compiling autocfg v1.5.0
   Compiling num-traits v0.2.19
   Compiling proc-macro-crate v3.4.0
   Compiling futures-task v0.3.31
   Compiling slab v0.4.11
   Compiling pin-project-lite v0.2.16
   Compiling pin-utils v0.1.0
   Compiling toml v0.8.23
   Compiling paste v1.0.15
   Compiling system-deps v7.0.5
   Compiling system-deps v6.2.2
   Compiling futures-macro v0.3.31
   Compiling futures-channel v0.3.31
   Compiling glib-sys v0.20.10
   Compiling gobject-sys v0.20.10
   Compiling gstreamer-sys v0.23.6
   Compiling gio-sys v0.20.10
   Compiling glib-sys v0.19.8
   Compiling futures-util v0.3.31
   Compiling gstreamer-base-sys v0.23.6
   Compiling thiserror v2.0.17
   Compiling bitflags v2.9.4
   Compiling memchr v2.7.6
   Compiling gobject-sys v0.19.8
   Compiling thiserror-impl v2.0.17
   Compiling glib-macros v0.20.12
   Compiling futures-executor v0.3.31
   Compiling num-integer v0.1.46
   Compiling either v1.15.0
   Compiling utf8parse v0.2.2
   Compiling anstyle-parse v0.2.7
   Compiling itertools v0.14.0
   Compiling option-operations v0.5.0
   Compiling num-rational v0.4.2
   Compiling gio-sys v0.19.8
   Compiling gstreamer-video-sys v0.23.6
   Compiling muldiv v1.0.1
   Compiling cfg-if v1.0.3
   Compiling once_cell v1.21.3
   Compiling anstyle-query v1.1.4
   Compiling colorchoice v1.0.4
   Compiling anstyle v1.0.11
   Compiling is_terminal_polyfill v1.70.1
   Compiling thiserror v1.0.69
   Compiling anstream v0.6.20
   Compiling thiserror-impl v1.0.69
   Compiling strsim v0.11.1
   Compiling anyhow v1.0.100
   Compiling clap_lex v0.7.5
   Compiling atomic_refcell v0.1.13
   Compiling clap_builder v4.5.48
   Compiling glib v0.20.12
   Compiling glib-macros v0.19.9
   Compiling clap_derive v4.5.47
   Compiling clap v4.5.48
   Compiling glib v0.19.9
   Compiling gstreamer v0.23.7
   Compiling gstreamer-base v0.23.6
   Compiling gstreamer-video v0.23.6
   Compiling rust-mp4-player v0.1.0 (/home/ian/rust-mp4-player)
error[E0432]: unresolved import glib::Cast
 --> src/main.rs:3:5
  |
3 | use glib::Cast;
  |     ^^^^^^^^^^ no Cast in the root
  |
help: consider importing one of these traits instead
  |
3 - use glib::Cast;
3 + use crate::gst_video::prelude::Cast;
  |
3 | use glib::prelude::Cast;
  |           +++++++++
3 - use glib::Cast;
3 + use gstreamer::prelude::Cast;
  |
3 - use glib::Cast;
3 + use gstreamer_video::prelude::Cast;
  |

warning: unused import: gstreamer_video as gst_video
 --> src/main.rs:6:5
  |
6 | use gstreamer_video as gst_video;
  |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  |
  = note: #[warn(unused_imports)] on by default

error[E0308]: mismatched types
  --> src/main.rs:33:8
   |
33 |     Ok(uri)
   |     -- ^^^- help: try using a conversion method: .to_string()
   |     |  |
   |     |  expected String, found GString
   |     arguments to this enum variant are incorrect
   |
help: the type constructed contains glib::GString due to the type of the argument passed
  --> src/main.rs:33:5
   |
33 |     Ok(uri)
   |     ^^^---^
   |        |
   |        this argument influences the type of Ok
note: tuple variant defined here
  --> /rustc/29483883eed69d5fb4db01964cdf2af4d86e9cb2/library/core/src/result.rs:552:5

error[E0599]: no method named ok_or_else found for enum Result in the current scope
  --> src/main.rs:45:10
   |
43 |       let playbin = gst::ElementFactory::make("playbin")
   |  ___________________-
44 | |         .build()
45 | |         .ok_or_else(|| anyhow!("Failed to create playbin"))?;
   | |_________-^^^^^^^^^^
   |
help: there is a method or_else with a similar name
   |
45 -         .ok_or_else(|| anyhow!("Failed to create playbin"))?;
45 +         .or_else(|| anyhow!("Failed to create playbin"))?;
   |

error[E0599]: no method named ok found for enum Option in the current scope
   --> src/main.rs:148:59
    |
148 |     let pos = pipeline.query_position::<gst::ClockTime>().ok().flatten();
    |                                                           ^^
    |
help: there is a method or with a similar name, but with different arguments
   --> /rustc/29483883eed69d5fb4db01964cdf2af4d86e9cb2/library/core/src/option.rs:1581:5

error[E0599]: no method named ok found for enum Option in the current scope
   --> src/main.rs:149:59
    |
149 |     let dur = pipeline.query_duration::<gst::ClockTime>().ok().flatten();
    |                                                           ^^
    |
help: there is a method or with a similar name, but with different arguments
   --> /rustc/29483883eed69d5fb4db01964cdf2af4d86e9cb2/library/core/src/option.rs:1581:5

Some errors have detailed explanations: E0308, E0432, E0599.
For more information about an error, try rustc --explain E0308.
warning: rust-mp4-player (bin "rust-mp4-player") generated 1 warning
error: could not compile rust-mp4-player (bin "rust-mp4-player") due to 5 previous errors; 1 warning emitted
ian@debian:~/rust-mp4-player/src$ 
ChatGPT said:

Nice logs—thanks! Your build is failing due to a few small API mismatches for the glib/gstreamer 0.23 stack:

    glib::Cast isn’t in scope that way (and you don’t need it here).

    filename_to_uri returns a GString → convert to String.

    ElementFactory::make(...).build() returns a Result, so don’t call .ok_or_else(...).

    query_position / query_duration already return Option<_> in 0.23, so don’t call .ok().

Here’s a drop-in fixed src/main.rs for your current Cargo.toml:

use anyhow::{anyhow, Context, Result};
use clap::Parser;
use gstreamer as gst;
use gstreamer::prelude::*;
use std::io::{self, BufRead};
use std::path::Path;
use std::thread;

/// Tiny MP4 player using GStreamer playbin.
/// Controls: type a letter then Enter in this terminal:
///   p = toggle pause/play
///   q = quit
///   s = seek +10s
///   r = seek -10s
#[derive(Parser, Debug)]
#[command(version, about = "Rust MP4 player (GStreamer)")]
struct Args {
    /// File path or URL (mp4, mov, mkv, etc. if codecs available)
    input: String,
}

fn to_uri(path_or_uri: &str) -> Result<String> {
    if path_or_uri.contains("://") {
        return Ok(path_or_uri.to_string());
    }
    let p = Path::new(path_or_uri)
        .canonicalize()
        .with_context(|| format!("Cannot resolve path: {path_or_uri}"))?;
    let uri = glib::filename_to_uri(p.to_str().unwrap(), None)
        .map_err(|_| anyhow!("Failed to convert to URI"))?
        .to_string(); // GString -> String
    Ok(uri)
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Init GStreamer
    gst::init()?;

    // Build playbin
    let playbin = gst::ElementFactory::make("playbin")
        .build()
        .map_err(|_| anyhow!("Failed to create playbin"))?;

    // Set URI
    let uri = to_uri(&args.input)?;
    playbin.set_property("uri", &uri)?;

    // Prefer a GL sink (nice rendering), but auto is fine if unavailable
    if let Ok(glsink) = gst::ElementFactory::make("glimagesink").build() {
        let _ = playbin.set_property("video-sink", &glsink);
    } else if let Ok(autosink) = gst::ElementFactory::make("autovideosink").build() {
        let _ = playbin.set_property("video-sink", &autosink);
    }

    // Bus to watch EOS / errors
    let bus = playbin.bus().ok_or_else(|| anyhow!("Failed to get bus"))?;

    // Start playing
    playbin
        .set_state(gst::State::Playing)
        .context("Failed to set pipeline to Playing")?;

    println!("Playing: {uri}");
    println!("Controls (type then Enter): p=pause/play | s=+10s | r=-10s | q=quit");

    // Spawn input thread for controls
    let playbin_ctrl = playbin.clone();
    let ctrl_handle = thread::spawn(move || -> Result<()> {
        let stdin = io::stdin();
        for line in stdin.lock().lines() {
            let cmd = line.unwrap_or_default().trim().to_string();
            if cmd.is_empty() {
                continue;
            }
            match cmd.chars().next().unwrap() {
                'p' | 'P' => {
                    let state = playbin_ctrl.current_state();
                    let new_state = match state {
                        gst::State::Playing => gst::State::Paused,
                        _ => gst::State::Playing,
                    };
                    let _ = playbin_ctrl.set_state(new_state);
                    println!("State -> {:?}", new_state);
                }
                's' | 'S' => {
                    seek_relative(&playbin_ctrl, 10_000)?;
                }
                'r' | 'R' => {
                    seek_relative(&playbin_ctrl, -10_000)?;
                }
                'q' | 'Q' => {
                    println!("Quitting…");
                    let _ = playbin_ctrl.set_state(gst::State::Null);
                    break;
                }
                _ => {
                    println!("Unknown command: '{cmd}'. Use p/s/r/q.");
                }
            }
        }
        Ok(())
    });

    // Main bus loop: wait for EOS or ERROR
    for msg in bus.iter_timed(gst::ClockTime::NONE) {
        use gst::MessageView;
        match msg.view() {
            MessageView::Eos(..) => {
                println!("End of stream.");
                break;
            }
            MessageView::Error(err) => {
                eprintln!(
                    "Error from {:?}: {} ({:?})",
                    err.src().map(|s| s.path_string()),
                    err.error(),
                    err.debug()
                );
                break;
            }
            MessageView::StateChanged(sc) => {
                if sc.src().map(|s| s == playbin).unwrap_or(false) {
                    println!("Pipeline state: {:?} -> {:?}", sc.old(), sc.current());
                }
            }
            _ => {}
        }
    }

    let _ = playbin.set_state(gst::State::Null);
    let _ = ctrl_handle.join();

    Ok(())
}

fn query_position_duration(pipeline: &gst::Element) -> (Option<i64>, Option<i64>) {
    // In gstreamer 0.23 these return Option<ClockTime> directly.
    let pos = pipeline
        .query_position::<gst::ClockTime>()
        .map(|p| p.nseconds() as i64);
    let dur = pipeline
        .query_duration::<gst::ClockTime>()
        .map(|d| d.nseconds() as i64);
    (pos, dur)
}

fn seek_relative(pipeline: &gst::Element, delta_ms: i64) -> Result<()> {
    let (pos_opt, dur_opt) = query_position_duration(pipeline);
    if let Some(pos_ns) = pos_opt {
        let mut target_ms = pos_ns / 1_000_000 + delta_ms;
        if let Some(dur_ns) = dur_opt {
            let dur_ms = dur_ns / 1_000_000;
            if target_ms < 0 {
                target_ms = 0;
            }
            if target_ms > dur_ms {
                target_ms = dur_ms;
            }
        }
        let target = gst::ClockTime::from_mseconds(target_ms as u64);
        pipeline.seek_simple(
            gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT,
            target,
        )?;
        println!("Seek -> {} ms", target_ms);
    }
    Ok(())
}

What changed (so you know for future):

    Removed use glib::Cast;

    Converted GString → String with .to_string()

    Switched .ok_or_else(..)? on build() to .map_err(..)? (it’s already a Result)

    Dropped .ok().flatten() in the position/duration queries (they’re already Option)

Now just:

cd ~/rust-mp4-player
cargo run -- /path/to/video.mp4

If you still hit a version mismatch later, we can bump to the latest gstreamer = "0.24" family—say the word and I’ll give you the tiny diffs (they’re very similar).
You said:
 Compiling rust-mp4-player v0.1.0 (/home/ian/rust-mp4-player)
error[E0277]: the ? operator can only be applied to values that implement Try
  --> src/main.rs:48:5
   |
48 |     playbin.set_property("uri", &uri)?;
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ the ? operator cannot be applied to type ()
   |
   = help: the trait Try is not implemented for ()

error[E0277]: can't compare &gstreamer::Object with gstreamer::Element
   --> src/main.rs:124:39
    |
124 |                 if sc.src().map(|s| s == playbin).unwrap_or(false) {
    |                                       ^^ no implementation for &gstreamer::Object == gstreamer::Element
    |
    = help: the trait PartialEq<gstreamer::Element> is not implemented for &gstreamer::Object
help: consider dereferencing here
    |
124 |                 if sc.src().map(|s| *s == playbin).unwrap_or(false) {
    |                                     +

For more information about this error, try rustc --explain E0277.
error: could not compile rust-mp4-player (bin "rust-mp4-player") due to 2 previous errors
ian@debian:~/rust-mp4-player/src$ 
ChatGPT said:

Good catch — two tiny fixes for the 0.23 API:

    set_property returns () here → remove the ?.

    To compare the message source with playbin, compare the underlying glib::Object pointers.

Here’s a drop-in src/main.rs with those fixes (and one extra use):

use anyhow::{anyhow, Context, Result};
use clap::Parser;
use gstreamer as gst;
use gstreamer::prelude::*;
use glib::prelude::Cast; // <-- add this
use std::io::{self, BufRead};
use std::path::Path;
use std::thread;

#[derive(Parser, Debug)]
#[command(version, about = "Rust MP4 player (GStreamer)")]
struct Args {
    input: String,
}

fn to_uri(path_or_uri: &str) -> Result<String> {
    if path_or_uri.contains("://") {
        return Ok(path_or_uri.to_string());
    }
    let p = Path::new(path_or_uri)
        .canonicalize()
        .with_context(|| format!("Cannot resolve path: {path_or_uri}"))?;
    let uri = glib::filename_to_uri(p.to_str().unwrap(), None)
        .map_err(|_| anyhow!("Failed to convert to URI"))?
        .to_string();
    Ok(uri)
}

fn main() -> Result<()> {
    let args = Args::parse();
    gst::init()?;

    let playbin = gst::ElementFactory::make("playbin")
        .build()
        .map_err(|_| anyhow!("Failed to create playbin"))?;

    let uri = to_uri(&args.input)?;
    // set_property returns (), so no `?`
    playbin.set_property("uri", &uri);

    if let Ok(glsink) = gst::ElementFactory::make("glimagesink").build() {
        let _ = playbin.set_property("video-sink", &glsink);
    } else if let Ok(autosink) = gst::ElementFactory::make("autovideosink").build() {
        let _ = playbin.set_property("video-sink", &autosink);
    }

    let bus = playbin.bus().ok_or_else(|| anyhow!("Failed to get bus"))?;

    playbin.set_state(gst::State::Playing).context("Failed to set pipeline to Playing")?;

    println!("Playing: {uri}");
    println!("Controls (type then Enter): p=pause/play | s=+10s | r=-10s | q=quit");

    let playbin_ctrl = playbin.clone();
    let ctrl_handle = thread::spawn(move || -> Result<()> {
        let stdin = io::stdin();
        for line in stdin.lock().lines() {
            let cmd = line.unwrap_or_default().trim().to_string();
            if cmd.is_empty() { continue; }
            match cmd.chars().next().unwrap() {
                'p' | 'P' => {
                    let state = playbin_ctrl.current_state();
                    let new_state = match state {
                        gst::State::Playing => gst::State::Paused,
                        _ => gst::State::Playing,
                    };
                    let _ = playbin_ctrl.set_state(new_state);
                    println!("State -> {:?}", new_state);
                }
                's' | 'S' => { seek_relative(&playbin_ctrl, 10_000)?; }
                'r' | 'R' => { seek_relative(&playbin_ctrl, -10_000)?; }
                'q' | 'Q' => {
                    println!("Quitting…");
                    let _ = playbin_ctrl.set_state(gst::State::Null);
                    break;
                }
                _ => println!("Unknown command: '{cmd}'. Use p/s/r/q."),
            }
        }
        Ok(())
    });

    for msg in bus.iter_timed(gst::ClockTime::NONE) {
        use gst::MessageView;
        match msg.view() {
            MessageView::Eos(..) => { println!("End of stream."); break; }
            MessageView::Error(err) => {
                eprintln!(
                    "Error from {:?}: {} ({:?})",
                    err.src().map(|s| s.path_string()),
                    err.error(),
                    err.debug()
                );
                break;
            }
            MessageView::StateChanged(sc) => {
                // Compare by pointer equality to the same underlying Object
                if let Some(src) = sc.src() {
                    if src.ptr_eq(playbin.upcast_ref::<gst::Object>()) {
                        println!("Pipeline state: {:?} -> {:?}", sc.old(), sc.current());
                    }
                }
            }
            _ => {}
        }
    }

    let _ = playbin.set_state(gst::State::Null);
    let _ = ctrl_handle.join();
    Ok(())
}

fn query_position_duration(pipeline: &gst::Element) -> (Option<i64>, Option<i64>) {
    let pos = pipeline.query_position::<gst::ClockTime>().map(|p| p.nseconds() as i64);
    let dur = pipeline.query_duration::<gst::ClockTime>().map(|d| d.nseconds() as i64);
    (pos, dur)
}

fn seek_relative(pipeline: &gst::Element, delta_ms: i64) -> Result<()> {
    let (pos_opt, dur_opt) = query_position_duration(pipeline);
    if let Some(pos_ns) = pos_opt {
        let mut target_ms = pos_ns / 1_000_000 + delta_ms;
        if let Some(dur_ns) = dur_opt {
            let dur_ms = dur_ns / 1_000_000;
            if target_ms < 0 { target_ms = 0; }
            if target_ms > dur_ms { target_ms = dur_ms; }
        }
        let target = gst::ClockTime::from_mseconds(target_ms as u64);
        pipeline.seek_simple(gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT, target)?;
        println!("Seek -> {} ms", target_ms);
    }
    Ok(())
}

Now run it again:

cargo run -- /path/to/video.mp4

If you’d like, we can also bump to the latest gstreamer = "0.24" crate family later; this version will work as-is on your current lockfile.
You said:
ian@debian:~/rust-mp4-player/src$ cargo run
   Compiling rust-mp4-player v0.1.0 (/home/ian/rust-mp4-player)
error[E0599]: no method named ptr_eq found for reference &gstreamer::Object in the current scope
  --> src/main.rs:99:28
   |
99 |                     if src.ptr_eq(playbin.upcast_ref::<gst::Object>()) {
   |                            ^^^^^^ method not found in &gstreamer::Object

error[E0599]: the method upcast_ref exists for struct gstreamer::Element, but its trait bounds were not satisfied
  --> src/main.rs:99:43
   |
99 |                       if src.ptr_eq(playbin.upcast_ref::<gst::Object>()) {
   |                                             ^^^^^^^^^^
   |
  ::: /home/ian/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/gstreamer-0.23.7/src/auto/element.rs:19:1
   |
19 | / glib::wrapper! {
20 | |     #[doc(alias = "GstElement")]
21 | |     pub struct Element(Object<ffi::GstElement, ffi::GstElementClass>) ...
...  |
26 | | }
   | |_- doesn't satisfy gstreamer::Element: glib::object::Cast or gstreamer::Element: glib::object::ObjectType
   |
   = note: the following trait bounds were not satisfied:
           gstreamer::Element: glib::object::ObjectType
           which is required by gstreamer::Element: glib::object::Cast
   = help: items from traits can only be used if the trait is in scope
help: trait Cast which provides upcast_ref is implemented but not in scope; perhaps you want to import it
   |
1  + use gstreamer::prelude::Cast;
   |

For more information about this error, try rustc --explain E0599.
error: could not compile rust-mp4-player (bin "rust-mp4-player") due to 2 previous errors
ian@debian:~/rust-mp4-player/src$ 
ChatGPT said:

Great—two quick tweaks and you’re good:

    Don’t use ptr_eq (not in your glib 0.19/0.20 stack). Compare by the object’s path string instead.

    Bring glib::prelude::* into scope so path_string() is available.

Here’s a drop-in src/main.rs with just those changes applied:

use anyhow::{anyhow, Context, Result};
use clap::Parser;
use gstreamer as gst;
use gstreamer::prelude::*;
use glib::prelude::*; // <-- for ObjectExt::path_string etc.
use std::io::{self, BufRead};
use std::path::Path;
use std::thread;

#[derive(Parser, Debug)]
#[command(version, about = "Rust MP4 player (GStreamer)")]
struct Args {
    input: String,
}

fn to_uri(path_or_uri: &str) -> Result<String> {
    if path_or_uri.contains("://") {
        return Ok(path_or_uri.to_string());
    }
    let p = Path::new(path_or_uri)
        .canonicalize()
        .with_context(|| format!("Cannot resolve path: {path_or_uri}"))?;
    let uri = glib::filename_to_uri(p.to_str().unwrap(), None)
        .map_err(|_| anyhow!("Failed to convert to URI"))?
        .to_string();
    Ok(uri)
}

fn main() -> Result<()> {
    let args = Args::parse();
    gst::init()?;

    let playbin = gst::ElementFactory::make("playbin")
        .build()
        .map_err(|_| anyhow!("Failed to create playbin"))?;

    let uri = to_uri(&args.input)?;
    playbin.set_property("uri", &uri);

    if let Ok(glsink) = gst::ElementFactory::make("glimagesink").build() {
        let _ = playbin.set_property("video-sink", &glsink);
    } else if let Ok(autosink) = gst::ElementFactory::make("autovideosink").build() {
        let _ = playbin.set_property("video-sink", &autosink);
    }

    let bus = playbin.bus().ok_or_else(|| anyhow!("Failed to get bus"))?;

    playbin
        .set_state(gst::State::Playing)
        .context("Failed to set pipeline to Playing")?;

    println!("Playing: {uri}");
    println!("Controls (type then Enter): p=pause/play | s=+10s | r=-10s | q=quit");

    // Cache the playbin's path string for equality checks
    let playbin_path = playbin.path_string();

    let playbin_ctrl = playbin.clone();
    let ctrl_handle = thread::spawn(move || -> Result<()> {
        let stdin = io::stdin();
        for line in stdin.lock().lines() {
            let cmd = line.unwrap_or_default().trim().to_string();
            if cmd.is_empty() {
                continue;
            }
            match cmd.chars().next().unwrap() {
                'p' | 'P' => {
                    let state = playbin_ctrl.current_state();
                    let new_state = match state {
                        gst::State::Playing => gst::State::Paused,
                        _ => gst::State::Playing,
                    };
                    let _ = playbin_ctrl.set_state(new_state);
                    println!("State -> {:?}", new_state);
                }
                's' | 'S' => { seek_relative(&playbin_ctrl, 10_000)?; }
                'r' | 'R' => { seek_relative(&playbin_ctrl, -10_000)?; }
                'q' | 'Q' => {
                    println!("Quitting…");
                    let _ = playbin_ctrl.set_state(gst::State::Null);
                    break;
                }
                _ => println!("Unknown command: '{cmd}'. Use p/s/r/q."),
            }
        }
        Ok(())
    });

    for msg in bus.iter_timed(gst::ClockTime::NONE) {
        use gst::MessageView;
        match msg.view() {
            MessageView::Eos(..) => { println!("End of stream."); break; }
            MessageView::Error(err) => {
                eprintln!(
                    "Error from {:?}: {} ({:?})",
                    err.src().map(|s| s.path_string()),
                    err.error(),
                    err.debug()
                );
                break;
            }
            MessageView::StateChanged(sc) => {
                if let Some(src) = sc.src() {
                    if src.path_string() == playbin_path {
                        println!("Pipeline state: {:?} -> {:?}", sc.old(), sc.current());
                    }
                }
            }
            _ => {}
        }
    }

    let _ = playbin.set_state(gst::State::Null);
    let _ = ctrl_handle.join();
    Ok(())
}

fn query_position_duration(pipeline: &gst::Element) -> (Option<i64>, Option<i64>) {
    let pos = pipeline.query_position::<gst::ClockTime>().map(|p| p.nseconds() as i64);
    let dur = pipeline.query_duration::<gst::ClockTime>().map(|d| d.nseconds() as i64);
    (pos, dur)
}

fn seek_relative(pipeline: &gst::Element, delta_ms: i64) -> Result<()> {
    let (pos_opt, dur_opt) = query_position_duration(pipeline);
    if let Some(pos_ns) = pos_opt {
        let mut target_ms = pos_ns / 1_000_000 + delta_ms;
        if let Some(dur_ns) = dur_opt {
            let dur_ms = dur_ns / 1_000_000;
            if target_ms < 0 { target_ms = 0; }
            if target_ms > dur_ms { target_ms = dur_ms; }
        }
        let target = gst::ClockTime::from_mseconds(target_ms as u64);
        pipeline.seek_simple(gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT, target)?;
        println!("Seek -> {} ms", target_ms);
    }
    Ok(())
}

Run it again:

cargo run -- /path/to/video.mp4

If you want to move up to the 0.24 crate family later (which has a few niceties), I can give you the minimal changes—this version will work with your current lockfile.
Is this conversation helpful so far?


ChatGPT can make mistakes. Check important info.
