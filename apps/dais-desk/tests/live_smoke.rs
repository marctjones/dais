//! Live smoke test: loads Dais Desk's headless window against the real
//! account configured in `owner-settings.json` (or `DAIS_DESK_SETTINGS`),
//! using the same `i-slint-backend-testing` software backend as
//! `visual_smoke.rs` — no real OS window, Dock presence, or desktop
//! interaction, unlike driving the actual native app. See issue #362.
//!
//! Skipped by default (it needs live network access and a configured
//! account); opt in with `DAIS_DESK_LIVE_SMOKE=1`.
//!
//! Pixel snapshot capture (`take_snapshot`) previously looked blank from this
//! entry point (issue #364). Root cause: the snapshot buffer comes back with
//! alpha=0 on every pixel even though the RGB data is real content —
//! `visual_smoke.rs`'s `capture()` helper already works around this (if more
//! than half the pixels are fully transparent, force alpha to 255 before
//! saving); this entry point just wasn't doing that step yet. With the same
//! fixup applied here, it renders real content like every other headless
//! window. Screenshot capture is opt-in via `DAIS_DESK_SCREENSHOT_DIR`.

use slint::{ComponentHandle, Model};

fn main() {
    if std::env::var_os("DAIS_DESK_LIVE_SMOKE").is_none() {
        println!("SKIP: set DAIS_DESK_LIVE_SMOKE=1 to run the live Desk smoke test");
        return;
    }

    if let Err(error) = run() {
        eprintln!("live smoke failed: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::var_os("SLINT_BACKEND").is_none() {
        std::env::set_var("SLINT_BACKEND", "software");
    }

    let settings_path = dais_desk::default_settings_path();
    println!("loading live account from {}", settings_path.display());
    let window = dais_desk::create_live_test_window(settings_path)?;
    window
        .window()
        .set_size(slint::LogicalSize::new(1180.0, 760.0));
    dais_desk::apply_responsive_layout(&window);
    window.show()?;
    slint::platform::update_timers_and_animations();

    let status = window.get_status_message().to_string();
    let account = window.get_active_account_label().to_string();
    let row_count = window.get_rows().row_count();
    println!("account: {account}");
    println!("status: {status}");
    println!("row count: {row_count}");

    if let Some(dir) = std::env::var_os("DAIS_DESK_SCREENSHOT_DIR") {
        capture(&window, &std::path::PathBuf::from(dir), "live-home")?;
    }

    let looks_like_fallback =
        status.contains("local preview data") || status.contains("401") || status.contains("403");
    if looks_like_fallback {
        return Err(format!(
            "Desk fell back to local preview data instead of loading the live account \
             (account={account:?}): {status}"
        )
        .into());
    }
    if row_count == 0 {
        return Err(format!(
            "live account {account:?} loaded with zero rows — likely empty or misconfigured"
        )
        .into());
    }

    window.hide()?;
    Ok(())
}

fn capture(
    window: &dais_desk::MainWindow,
    output_dir: &std::path::Path,
    name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all(output_dir)?;
    slint::platform::update_timers_and_animations();
    let snapshot = window.window().take_snapshot()?;
    let width = snapshot.width();
    let height = snapshot.height();
    let mut bytes = snapshot.as_bytes().to_vec();

    // The software renderer's snapshot buffer can come back with alpha=0 on
    // every pixel even when the RGB data is real content (issue #364) — force
    // opaque before saving so the PNG isn't misleadingly blank.
    let transparent_pixels = bytes.chunks_exact(4).filter(|pixel| pixel[3] == 0).count();
    if transparent_pixels * 2 > width as usize * height as usize {
        for pixel in bytes.chunks_exact_mut(4) {
            pixel[3] = 255;
        }
    }

    let path = output_dir.join(format!("{name}.png"));
    image::save_buffer_with_format(
        &path,
        &bytes,
        width,
        height,
        image::ColorType::Rgba8,
        image::ImageFormat::Png,
    )?;
    println!("wrote {}", path.display());
    Ok(())
}
