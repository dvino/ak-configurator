// Offline-only original-client probe. Build directly with rustc and the existing
// serde/serde_json rlibs; see scripts/test_openrgb_interop.py. Never opens HID.
#![allow(dead_code)]
#[path = "../src/ak820_openrgb_bridge.rs"]
mod ak820_openrgb_bridge;
#[path = "../src/ak820_rgb.rs"]
mod ak820_rgb;
#[path = "../src/ak820_rgb_compositor.rs"]
mod ak820_rgb_compositor;
use ak820_openrgb_bridge::{Bridge, Status};
use std::io::{BufRead, Write};
fn main() {
    let bridge = Bridge::start(0).unwrap();
    println!("{}", bridge.port());
    std::io::stdout().flush().unwrap();
    // Test coordinator requests snapshots only after original SDK update/readback
    // barriers, so the probe examines real accepted source frames.
    for line in std::io::stdin().lock().lines() {
        let line = line.unwrap();
        let snapshot = bridge.poll();
        if line == "frame" {
            assert_eq!(snapshot.status, Status::Connected);
            let frame = snapshot.frame.expect("new RGB frame");
            println!("{}", serde_json::to_string(&frame.to_vec()).unwrap());
        } else if line == "disconnected" {
            assert_eq!(snapshot.status, Status::Disconnected);
            assert!(snapshot.frame.is_none());
            println!("disconnected");
        } else if line == "done" {
            break;
        }
        std::io::stdout().flush().unwrap();
    }
}
