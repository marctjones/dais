//! Live smoke test: loads Dais Desk's headless window against the real
//! account configured in `owner-settings.json` (or `DAIS_DESK_SETTINGS`),
//! using the same `i-slint-backend-testing` software backend as
//! `visual_smoke.rs` — no real OS window, Dock presence, or desktop
//! interaction, unlike driving the actual native app. See issue #362.
//!
//! Skipped by default (it needs live network access and a configured
//! account); opt in with `DAIS_DESK_LIVE_SMOKE=1`.
//!
//! Navigates through every mode_nav()/screen_nav() button by real accessible
//! click (see #371), the same way `gui_interaction.rs`'s headless-fixture
//! reachability test does, instead of jumping straight to a screen id —
//! catches a screen that's reachable against fixture data (headless tests
//! pass) but silently falls back to placeholder content or errors against a
//! real account (a class of bug no assertion against fixture data alone can
//! catch).
//!
//! Pixel snapshot capture (`take_snapshot`) previously looked blank from this
//! entry point (issue #364). Root cause: the snapshot buffer comes back with
//! alpha=0 on every pixel even though the RGB data is real content —
//! `visual_smoke.rs`'s `capture()` helper already works around this (if more
//! than half the pixels are fully transparent, force alpha to 255 before
//! saving); this entry point just wasn't doing that step yet. With the same
//! fixup applied here, it renders real content like every other headless
//! window. Screenshot capture is opt-in via `DAIS_DESK_SCREENSHOT_DIR`.
//!
//! Beyond navigation reachability, also verifies real *behavior* against the
//! live account (#371): a real encrypted conversation actually renders
//! decrypted plaintext inline rather than a ciphertext/placeholder fallback,
//! and replying through the real row action preserves the audience context
//! (direct replies keep Direct visibility and their recipient) rather than
//! resetting to a generic default. Both checks skip (rather than fail) when
//! the live account simply doesn't have the relevant data — they exist to
//! catch Desk silently losing context that was present, not to require any
//! particular fixture shape from the account under test.

use i_slint_backend_testing::ElementHandle;
use slint::platform::PointerEventButton;
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
    i_slint_backend_testing::init_no_event_loop();

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

    // `showing_fixture_data` (issue #359) reflects `self.data.api_error`,
    // which only changes on an actual refresh() — unlike `status_message`,
    // which `select_screen()` unconditionally resets to "Ready." on every
    // navigation. Checking `status_message` per-screen below would silently
    // never catch anything past the first screen; `showing_fixture_data` is
    // the signal that actually persists across real navigation.
    if window.get_showing_fixture_data() || looks_like_fallback(&status) {
        return Err(format!(
            "Desk fell back to local preview data instead of loading the live account \
             (account={account:?}): {}",
            window.get_fixture_data_reason()
        )
        .into());
    }
    if row_count == 0 {
        return Err(format!(
            "live account {account:?} loaded with zero rows — likely empty or misconfigured"
        )
        .into());
    }

    let mut broken_screens = Vec::new();
    for (mode_id, mode_title) in nav_items(window.get_mode_nav()) {
        click(&window, &mode_title)?;
        if window.get_active_mode().as_str() != mode_id {
            broken_screens.push(format!(
                "mode nav button {mode_title:?} did not navigate to mode {mode_id:?}"
            ));
            continue;
        }

        for (screen_id, screen_title) in nav_items(window.get_screen_nav()) {
            click(&window, &screen_title)?;
            if window.get_active_screen().as_str() != screen_id {
                broken_screens.push(format!(
                    "screen nav button {screen_title:?} in mode {mode_id:?} did not navigate to screen {screen_id:?}"
                ));
                continue;
            }
            if window.get_showing_fixture_data() {
                broken_screens.push(format!(
                    "{mode_id}/{screen_id}: fell back to placeholder data after real navigation: {}",
                    window.get_fixture_data_reason()
                ));
            }
        }
    }

    if !broken_screens.is_empty() {
        return Err(format!(
            "real click-through navigation found {} broken screen(s):\n{}",
            broken_screens.len(),
            broken_screens.join("\n")
        )
        .into());
    }

    check_conversation_decrypts_to_plaintext(&window)?;
    check_reply_preserves_audience_context(&window)?;

    window.hide()?;
    Ok(())
}

fn check_conversation_decrypts_to_plaintext(
    window: &dais_desk::MainWindow,
) -> Result<(), Box<dyn std::error::Error>> {
    window.invoke_select_mode("home".into());
    window.invoke_select_screen("inbox".into());
    slint::platform::update_timers_and_animations();

    let rows = ui_rows(window.get_rows());
    let Some(conversation_row) = rows.iter().find(|row| row.id.starts_with("conversation:")) else {
        println!(
            "no conversation rows on the live account; skipping decrypted-plaintext check"
        );
        return Ok(());
    };
    let row_id = conversation_row.id.to_string();

    window.invoke_select_row(row_id.clone().into());
    slint::platform::update_timers_and_animations();

    let combined = ui_rows(window.get_inspector_rows())
        .iter()
        .map(|row| format!("{}\n{}\n{}\n{}", row.title, row.subtitle, row.detail, row.meta))
        .collect::<Vec<_>>()
        .join("\n");

    if combined.contains("No local MLS state found") {
        println!(
            "conversation {row_id} has no local MLS state on this device; skipping decrypted-plaintext check"
        );
        return Ok(());
    }

    for signal in [
        "could not be decrypted",
        "Unsupported encrypted message protocol",
        "decryption failed",
    ] {
        if combined.contains(signal) {
            return Err(format!(
                "conversation {row_id} failed to render decrypted plaintext: {combined}"
            )
            .into());
        }
    }
    if combined.trim().is_empty() {
        return Err(format!("conversation {row_id} inspector had no content").into());
    }
    Ok(())
}

fn check_reply_preserves_audience_context(
    window: &dais_desk::MainWindow,
) -> Result<(), Box<dyn std::error::Error>> {
    window.invoke_select_mode("home".into());
    window.invoke_select_screen("inbox".into());
    slint::platform::update_timers_and_animations();

    let rows = ui_rows(window.get_rows());
    let Some(reply_row) = rows
        .iter()
        .find(|row| row.primary.as_str() == "Reply" || row.secondary.as_str() == "Reply")
    else {
        println!("no row in Inbox supports Reply on the live account; skipping reply-audience check");
        return Ok(());
    };
    let row_id = reply_row.id.to_string();
    let expects_direct = row_id.starts_with("conversation:") || row_id.starts_with("dm:");

    window.invoke_row_action(row_id.clone().into(), "Reply".into());
    slint::platform::update_timers_and_animations();

    if window.get_active_screen().as_str() != "compose" {
        return Err(format!("replying to {row_id} did not navigate to compose").into());
    }
    let visibility = window.get_compose_visibility().to_string();
    if !["public", "unlisted", "followers", "direct"].contains(&visibility.as_str()) {
        return Err(format!(
            "replying to {row_id} produced an unexpected compose visibility {visibility:?}"
        )
        .into());
    }
    if expects_direct {
        if visibility != "direct" {
            return Err(format!(
                "replying to direct conversation {row_id} should preserve Direct visibility, got {visibility:?}"
            )
            .into());
        }
        if window.get_compose_recipients().to_string().trim().is_empty() {
            return Err(
                format!("replying to direct conversation {row_id} lost its recipient").into(),
            );
        }
    }
    Ok(())
}

fn ui_rows(model: slint::ModelRc<dais_desk::UiRow>) -> Vec<dais_desk::UiRow> {
    (0..model.row_count())
        .filter_map(|index| model.row_data(index))
        .collect()
}

fn looks_like_fallback(status: &str) -> bool {
    status.contains("local preview data") || status.contains("401") || status.contains("403")
}

fn nav_items(model: slint::ModelRc<dais_desk::NavItem>) -> Vec<(String, String)> {
    model
        .iter()
        .map(|item| (item.id.to_string(), item.title.to_string()))
        .collect()
}

fn click(window: &dais_desk::MainWindow, label: &str) -> Result<(), Box<dyn std::error::Error>> {
    let matches: Vec<_> = ElementHandle::find_by_accessible_label(window, label).collect();
    if matches.is_empty() {
        return Err(format!("expected to find an accessible control labelled {label:?}").into());
    }
    matches[0].mock_single_click(PointerEventButton::Left);
    slint::platform::update_timers_and_animations();
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
