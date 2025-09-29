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

