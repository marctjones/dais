//! Opt-in, single-purpose gate for #371's compose-send verification: proves a
//! real compose send from Desk actually reaches the server (mirroring what
//! `docs/guides/TESTING.md`'s conformance suite already checks at the
//! protocol layer, at the GUI layer instead).
//!
//! Unlike every other Desk test, this one emits a real federated write — the
//! post is delivered to followers and remote servers, not just read back
//! locally. It is NOT part of the default release gate
//! (`scripts/release-desk-v2.sh` must never invoke it automatically) and is
//! gated behind two independent opt-ins:
//!   - `DAIS_DESK_LIVE_COMPOSE_SEND=1` (skipped otherwise)
//!   - the configured active account's instance must be `skpt.cl` (or a
//!     `skpt.cl` subdomain) — the test refuses to run against any other
//!     host, including dais.social, as a hard safety rail against
//!     accidentally sending a real post from the production account.

use dais_desk::StoredOwnerSettings;
use slint::ComponentHandle;
use std::path::Path;

fn main() {
    if std::env::var_os("DAIS_DESK_LIVE_COMPOSE_SEND").is_none() {
        println!("SKIP: set DAIS_DESK_LIVE_COMPOSE_SEND=1 to run the live compose-send gate");
        return;
    }

    if let Err(error) = run() {
        eprintln!("live compose-send gate failed: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::var_os("SLINT_BACKEND").is_none() {
        std::env::set_var("SLINT_BACKEND", "software");
    }
    i_slint_backend_testing::init_no_event_loop();

    let settings_path = dais_desk::default_settings_path();
    let active_instance = active_instance_url(&settings_path)?;
    let host = host_of(&active_instance);
    if host != "skpt.cl" && !host.ends_with(".skpt.cl") {
        return Err(format!(
            "refusing to run a real compose-send against {active_instance:?} (host {host:?}); \
             this gate only ever runs against skpt.cl, never dais.social or any other host"
        )
        .into());
    }
    println!("compose-send target: {active_instance} (host {host:?} confirmed skpt.cl)");

    let window = dais_desk::create_live_test_window(settings_path)?;
    window
        .window()
        .set_size(slint::LogicalSize::new(1180.0, 760.0));
    dais_desk::apply_responsive_layout(&window);
    window.show()?;
    slint::platform::update_timers_and_animations();

    window.invoke_select_mode("home".into());
    window.invoke_select_screen("compose".into());
    slint::platform::update_timers_and_animations();

    // Default Followers/ActivityPub visibility (ComposeState::default()) is
    // left untouched rather than forced to Public — the smallest blast
    // radius that still exercises the real create_post() network path.
    let marker = format!(
        "dais desk automated release-gate check ({}) — safe to delete",
        std::env::var("DAIS_DESK_LIVE_COMPOSE_SEND_MARKER")
            .unwrap_or_else(|_| "unlabeled".to_string())
    );
    window.set_compose_text(marker.clone().into());
    slint::platform::update_timers_and_animations();

    window.invoke_compose_send();
    slint::platform::update_timers_and_animations();

    let status = window.get_status_message().to_string();
    println!("compose-send status: {status}");
    if !status.starts_with("Posted ") {
        return Err(format!(
            "compose-send did not report a real post delivered to the server: {status}"
        )
        .into());
    }

    window.hide()?;
    Ok(())
}

fn active_instance_url(settings_path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let raw = std::fs::read_to_string(settings_path)
        .map_err(|error| format!("could not read {}: {error}", settings_path.display()))?;
    let settings: StoredOwnerSettings = serde_json::from_str(&raw)
        .map_err(|error| format!("could not parse {}: {error}", settings_path.display()))?;
    let active = settings
        .active_account_id
        .as_deref()
        .and_then(|id| settings.accounts.iter().find(|account| account.id == id))
        .map(|account| account.instance_url.clone())
        .unwrap_or(settings.instance_url);
    Ok(active)
}

fn host_of(url: &str) -> String {
    let without_scheme = url
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    let host_and_port = without_scheme.split('/').next().unwrap_or("");
    host_and_port
        .split(':')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
}
