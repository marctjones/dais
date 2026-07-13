use image::{ColorType, ImageFormat};
use slint::{ComponentHandle, Model, ModelRc};
use std::collections::BTreeSet;
use std::error::Error;
use std::path::{Path, PathBuf};

fn main() {
    if let Err(error) = run() {
        eprintln!("visual smoke failed: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    if std::env::var_os("SLINT_BACKEND").is_none() {
        std::env::set_var("SLINT_BACKEND", "software");
    }

    let output_dir = std::env::var_os("DAIS_DESK_SCREENSHOT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/dais-desk-screenshots")
        });
    std::fs::create_dir_all(&output_dir)?;

    let window = dais_desk::create_test_window()?;
    set_smoke_size(&window, 1180.0, 760.0);
    window.show()?;

    assert_screen_content(
        &window,
        "today",
        "Feed",
        "timeline:ada-week-friday-space-news",
    );
    capture(&window, &output_dir, "home")?;

    // Hover has no headless test signal (no pointer-move mock, and
    // take_snapshot() doesn't capture hover state), so the tooltip's real
    // content is verified through this screenshot-only override instead of
    // a real hover (#369).
    window.set_debug_show_toolbar_tooltip_for("compose".into());
    capture(&window, &output_dir, "home-toolbar-tooltip")?;
    window.set_debug_show_toolbar_tooltip_for("".into());

    set_smoke_size(&window, 920.0, 660.0);
    assert!(
        window.get_inspector_compact(),
        "inspector should collapse at the minimum visual-smoke width"
    );
    capture(&window, &output_dir, "home-min-width")?;
    set_smoke_size(&window, 1440.0, 860.0);
    assert!(
        !window.get_inspector_compact(),
        "inspector should be expanded at the wide visual-smoke width"
    );
    capture(&window, &output_dir, "home-wide")?;
    set_smoke_size(&window, 1180.0, 760.0);

    window.invoke_select_screen("compose".into());
    assert_compose_surface(&window);
    capture(&window, &output_dir, "home-compose-media")?;
    set_smoke_size(&window, 920.0, 660.0);
    assert!(
        window.get_inspector_compact(),
        "compose minimum-width screenshot should use the compact inspector rail"
    );
    assert_compose_surface(&window);
    capture(&window, &output_dir, "home-compose-min-width")?;
    set_smoke_size(&window, 1180.0, 760.0);
    window.invoke_select_screen("inbox".into());
    window.invoke_select_row("notification:notice-reply".into());
    assert_screen_content(&window, "inbox", "Inbox", "notification:notice-reply");
    capture(&window, &output_dir, "home-inbox-notifications")?;
    window.invoke_row_action("timeline:ada-week-friday-space-news".into(), "Save".into());
    capture(&window, &output_dir, "workflow-save-post")?;
    window.invoke_select_screen("today".into());
    capture(&window, &output_dir, "home-today")?;
    // Direct/encrypted conversations merged into Inbox per #367 (IA doc
    // §4.1/§5) — no more standalone "conversations" screen.
    window.invoke_select_screen("inbox".into());
    assert_screen_content(&window, "inbox", "Inbox", "conversation:peer:");
    assert_encrypted_conversation_content(&window);
    capture(&window, &output_dir, "home-inbox-conversations")?;

    window.invoke_select_screen("posts".into());
    assert_screen_content(&window, "posts", "My Posts", "post:");
    window.invoke_select_row("post:fixture-private-post".into());
    assert_inspector_content(&window, "Thread detail", ":reply:");
    capture(&window, &output_dir, "home-post-thread")?;

    window.invoke_select_screen("saved".into());
    assert_screen_content(&window, "saved", "Saved", "saved:");
    capture(&window, &output_dir, "home-saved")?;

    window.invoke_select_screen("inbox".into());
    window.invoke_row_action("notification:notice-reply".into(), "Reply".into());
    capture(&window, &output_dir, "workflow-reply-compose")?;

    window.invoke_select_mode("people".into());
    window.invoke_select_screen("find".into());
    assert_screen_content(&window, "find", "Find", "bundle:science-news");
    assert!(
        !window.get_inspector_compact(),
        "find smoke width should keep the inspector open"
    );
    assert!(
        window.get_find_compact_form(),
        "find smoke width should use single-column filter fields"
    );
    capture(&window, &output_dir, "people-find-search")?;

    window.invoke_select_screen("friends".into());
    assert_screen_content(&window, "friends", "Friends", "actor:");
    capture(&window, &output_dir, "people-friends")?;
    window.invoke_select_screen("followers".into());
    assert_screen_content(&window, "followers", "Follow Requests", "follower:");
    capture(&window, &output_dir, "people-followers")?;
    window.invoke_select_screen("following".into());
    assert_screen_content(&window, "following", "Following", "following:");
    capture(&window, &output_dir, "people-following")?;
    window.invoke_select_mode("people".into());
    window.invoke_select_screen("followers".into());
    window.invoke_row_action(
        "follower:https://new.example/users/follower".into(),
        "Approve".into(),
    );
    capture(&window, &output_dir, "workflow-follower-approve")?;

    window.invoke_select_screen("accounts".into());
    assert_screen_content(&window, "accounts", "Accounts & Tokens", "account:");
    capture(&window, &output_dir, "settings-accounts")?;

    window.invoke_select_screen("settings".into());
    assert_screen_content(&window, "settings", "Settings", "settings:");
    capture(&window, &output_dir, "settings-privacy")?;

    window.invoke_select_screen("security".into());
    assert_screen_content(&window, "security", "Security", "security:");
    capture(&window, &output_dir, "settings-security")?;

    window.hide()?;

    // Owner API 401/403 fallback must show a persistent, hard-to-miss warning
    // banner rather than only a status-bar line (see issue #359).
    let fixture_error_window = dais_desk::create_test_window_with_api_error(
        "owner API returned 401 Unauthorized: Owner bearer token required".to_string(),
    )?;
    set_smoke_size(&fixture_error_window, 1180.0, 760.0);
    fixture_error_window.show()?;
    assert!(
        fixture_error_window.get_showing_fixture_data(),
        "api_error should surface as showing_fixture_data on the projection"
    );
    assert!(
        fixture_error_window
            .get_fixture_data_reason()
            .contains("401"),
        "fixture data reason should include the underlying API error"
    );
    capture(&fixture_error_window, &output_dir, "home-fixture-data-warning")?;
    fixture_error_window.hide()?;

    Ok(())
}

fn capture(
    window: &dais_desk::MainWindow,
    output_dir: &Path,
    name: &str,
) -> Result<(), Box<dyn Error>> {
    slint::platform::update_timers_and_animations();
    let snapshot = window.window().take_snapshot()?;
    let width = snapshot.width();
    let height = snapshot.height();
    let mut bytes = snapshot.as_bytes().to_vec();

    assert!(width >= 900, "{name} snapshot width is too small: {width}");
    assert!(
        height >= 600,
        "{name} snapshot height is too small: {height}"
    );
    assert_eq!(
        bytes.len(),
        width as usize * height as usize * 4,
        "{name} snapshot byte length does not match RGBA dimensions"
    );

    let transparent_pixels = bytes.chunks_exact(4).filter(|pixel| pixel[3] == 0).count();
    if transparent_pixels * 2 > width as usize * height as usize {
        for pixel in bytes.chunks_exact_mut(4) {
            pixel[3] = 255;
        }
    }

    let mut sampled_colors = BTreeSet::new();
    let stride = ((width as usize * height as usize) / 6000).max(1);
    for pixel in bytes.chunks_exact(4).step_by(stride) {
        sampled_colors.insert([pixel[0], pixel[1], pixel[2], pixel[3]]);
    }
    assert!(
        sampled_colors.len() > 32,
        "{name} snapshot looks visually blank: only {} sampled colors",
        sampled_colors.len()
    );
    assert!(
        bytes
            .chunks_exact(4)
            .any(|pixel| pixel[0] < 245 && pixel[1] < 245 && pixel[2] < 245),
        "{name} snapshot is missing visible foreground content"
    );

    let path = output_dir.join(format!("{name}.png"));
    image::save_buffer_with_format(
        &path,
        &bytes,
        width,
        height,
        ColorType::Rgba8,
        ImageFormat::Png,
    )?;
    println!("wrote {}", path.display());
    Ok(())
}

fn set_smoke_size(window: &dais_desk::MainWindow, width: f32, height: f32) {
    window
        .window()
        .set_size(slint::LogicalSize::new(width, height));
    dais_desk::apply_responsive_layout(window);
}

fn assert_screen_content(
    window: &dais_desk::MainWindow,
    expected_screen: &str,
    expected_title: &str,
    expected_row_prefix: &str,
) {
    assert_eq!(window.get_active_screen().as_str(), expected_screen);
    let title = window.get_window_title();
    assert!(
        title.contains(expected_title),
        "{expected_screen} title {title:?} did not include {expected_title:?}"
    );
    assert!(
        !window.get_active_account_label().trim().is_empty(),
        "{expected_screen} must expose the active account label"
    );
    let rows = ui_rows(window.get_rows());
    assert!(
        rows.iter()
            .any(|row| row.id.as_str().starts_with(expected_row_prefix)),
        "{expected_screen} did not expose a visible row with prefix {expected_row_prefix:?}; rows: {:?}",
        rows.iter()
            .map(|row| row.id.to_string())
            .collect::<Vec<_>>()
    );
    assert!(
        rows.iter().all(|row| !row.title.trim().is_empty()),
        "{expected_screen} has a visible row without a title"
    );
}

fn assert_encrypted_conversation_content(window: &dais_desk::MainWindow) {
    let rows = ui_rows(window.get_rows());
    let combined = rows
        .iter()
        .map(|row| {
            format!(
                "{}\n{}\n{}\n{}",
                row.title, row.subtitle, row.detail, row.meta
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        combined.contains("backyard telescope"),
        "conversations should show realistic social context instead of an encrypted placeholder: {combined}"
    );
    assert!(
        !combined.contains("locked encrypted message"),
        "conversation fixture should not be rendered as a locked encrypted placeholder: {combined}"
    );
}

fn assert_inspector_content(
    window: &dais_desk::MainWindow,
    expected_title: &str,
    expected_row_fragment: &str,
) {
    let rows = ui_rows(window.get_inspector_rows());
    let combined = rows
        .iter()
        .map(|row| format!("{}\n{}\n{}\n{}", row.id, row.title, row.detail, row.meta))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        combined.contains(expected_title),
        "inspector did not include {expected_title:?}: {combined}"
    );
    assert!(
        combined.contains(expected_row_fragment),
        "inspector did not include row fragment {expected_row_fragment:?}: {combined}"
    );
}

fn ui_rows(model: ModelRc<dais_desk::UiRow>) -> Vec<dais_desk::UiRow> {
    (0..model.row_count())
        .filter_map(|index| model.row_data(index))
        .collect()
}

fn assert_compose_surface(window: &dais_desk::MainWindow) {
    assert_eq!(window.get_active_screen().as_str(), "compose");
    let action = window.get_compose_primary_action_text();
    assert!(
        matches!(
            action.as_str(),
            "Send" | "Send message" | "Send encrypted" | "Post publicly"
        ),
        "compose primary action text is missing or unexpected: {action}"
    );
    assert!(
        !window.get_compose_audience_summary().trim().is_empty(),
        "compose audience summary must be available in the compact compose surface"
    );
}
