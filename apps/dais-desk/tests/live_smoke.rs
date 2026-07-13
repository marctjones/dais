//! Live smoke test: loads Dais Desk's headless window against the real
//! account configured in `owner-settings.json` (or `DAIS_DESK_SETTINGS`),
//! using the same `i-slint-backend-testing` software backend as
//! `visual_smoke.rs` — no real OS window, Dock presence, or desktop
//! interaction, unlike driving the actual native app. See issue #362.
//!
//! Skipped by default (it needs live network access and a configured
//! account); opt in with `DAIS_DESK_LIVE_SMOKE=1`.
//!
//! Pixel snapshot capture (`take_snapshot`) reliably renders blank from this
//! entry point for reasons not yet root-caused — `visual_smoke.rs`'s
//! identical `create_test_window` + resize + show sequence renders real
//! content, so something specific to this binary's window setup is at
//! fault, not the renderer generally. Rather than write a misleadingly
//! blank screenshot, this only asserts on the real signal that matters:
//! the live account's actual loaded state (status message, account label,
//! row count), read directly from the window's properties.

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
