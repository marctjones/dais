use i_slint_backend_testing::{AccessibleRole, ElementHandle, ElementQuery};
use slint::platform::PointerEventButton;
use slint::Model;

fn click_label(window: &dais_desk::MainWindow, label: &str) {
    let matches: Vec<_> = ElementHandle::find_by_accessible_label(window, label).collect();
    assert!(
        !matches.is_empty(),
        "expected to find an accessible control labelled {label:?}"
    );
    matches[0].mock_single_click(PointerEventButton::Left);
    slint::platform::update_timers_and_animations();
}

/// Regression coverage for #363: a live macOS accessibility-tree query against
/// the running app found nearly every toolbar/nav button and the account
/// switcher reporting `name=missing value`. Assert every interactive control
/// (button, combobox, text input) has a non-empty accessible label, the same
/// way VoiceOver or automation would look for one.
#[test]
fn every_interactive_control_has_an_accessible_label() {
    i_slint_backend_testing::init_no_event_loop();
    let window = dais_desk::create_test_window().expect("test fixture window");

    let screens: &[(&str, &str)] = &[
        ("home", "today"),
        ("home", "compose"),
        ("people", "find"),
        ("people", "audience"),
        ("people", "blocks"),
        ("server", "identity"),
        ("server", "moderation"),
        ("server", "accounts"),
        ("server", "settings"),
    ];

    for (mode, screen) in screens {
        window.invoke_select_mode((*mode).into());
        window.invoke_select_screen((*screen).into());

        for role in [
            AccessibleRole::Button,
            AccessibleRole::Combobox,
            AccessibleRole::TextInput,
        ] {
            let elements = ElementQuery::from_root(&window)
                .match_accessible_role(role)
                .find_all();
            for element in elements {
                let label = element.accessible_label();
                assert!(
                    label
                        .as_deref()
                        .is_some_and(|value| !value.trim().is_empty()),
                    "on {mode}/{screen}: {role:?} control (type {:?}) has no accessible-label",
                    element.type_name()
                );
            }
        }
    }
}

/// Regression coverage for #369: the toolbar's keyboard shortcuts (already
/// wired via `KeyBinding` — see app.slint's Cmd/Ctrl+N and Cmd/Ctrl+R) were
/// undiscoverable, since nothing in the UI surfaced them. A hover tooltip
/// can't be asserted headlessly (the testing backend has no hover/pointer-
/// move mock and `take_snapshot()` doesn't capture hover state either), so
/// this asserts the one surface that is both real and testable: the
/// shortcut is spelled out in each button's `accessible-description`, which
/// screen readers and other assistive tech already read for these controls.
#[test]
fn toolbar_command_buttons_surface_their_keyboard_shortcut_in_accessible_description() {
    i_slint_backend_testing::init_no_event_loop();
    let window = dais_desk::create_test_window().expect("test fixture window");

    // Home mode's screen_nav also exposes a same-labelled "Compose" button
    // (a different control, no shortcut of its own) — the toolbar and nav
    // controls share an accessible-label, so check across every match for
    // that label rather than assuming an index or uniqueness.
    let expectations: &[(&str, &str)] = &[("Compose", "⌘N"), ("Sync", "⌘R")];
    for (label, shortcut) in expectations {
        let matches: Vec<_> = ElementHandle::find_by_accessible_label(&window, label).collect();
        assert!(
            !matches.is_empty(),
            "expected to find a toolbar control labelled {label:?}"
        );
        assert!(
            matches
                .iter()
                .any(|m| m.accessible_description().is_some_and(|d| d.contains(shortcut))),
            "no control labelled {label:?} mentioned its shortcut {shortcut:?} in accessible-description; found: {:?}",
            matches
                .iter()
                .map(|m| m.accessible_description())
                .collect::<Vec<_>>()
        );
    }
}

/// Regression coverage for #370: RowCard gained a background TouchArea
/// (driving the meta-on-hover behavior) declared before its content layout
/// specifically so the Chip and action Buttons added later stay on top for
/// hit-testing. Confirms a real simulated click still reaches the row's own
/// Save button rather than being swallowed by that background TouchArea.
#[test]
fn selected_row_action_button_click_is_not_swallowed_by_the_row_background() {
    i_slint_backend_testing::init_no_event_loop();
    let window = dais_desk::create_test_window().expect("test fixture window");
    window.invoke_select_screen("today".into());
    window.invoke_select_row("timeline:ada-week-friday-space-news".into());
    slint::platform::update_timers_and_animations();

    // Clicking the row's own Reply button navigates to compose — a real
    // Button click, since the row's own accessible-action-default (which
    // the background TouchArea also drives via `open()`) only changes
    // selection, never the active screen.
    click_label(&window, "Reply");
    assert_eq!(
        window.get_active_screen().as_str(),
        "compose",
        "clicking the row's own Reply button should have run the Reply row action, not just reopened the row"
    );
}

#[test]
fn navigates_primary_workflows_through_accessible_controls() {
    i_slint_backend_testing::init_no_event_loop();

    let window = dais_desk::create_test_window().expect("test fixture window");
    assert_eq!(window.get_active_mode().as_str(), "home");
    assert_eq!(window.get_active_screen().as_str(), "today");

    window.set_command_text("ada".into());
    click_label(&window, "Find");
    assert_eq!(window.get_active_mode().as_str(), "people");
    assert_eq!(window.get_active_screen().as_str(), "find");

    click_label(&window, "Home");
    assert_eq!(window.get_active_mode().as_str(), "home");
    assert_eq!(window.get_active_screen().as_str(), "today");

    click_label(&window, "Inbox");
    assert_eq!(window.get_active_mode().as_str(), "home");
    assert_eq!(window.get_active_screen().as_str(), "inbox");

    click_label(&window, "People");
    assert_eq!(window.get_active_mode().as_str(), "people");
    assert_eq!(window.get_active_screen().as_str(), "find");

    click_label(&window, "Requests");
    assert_eq!(window.get_active_screen().as_str(), "followers");
    assert!(window.get_window_title().contains("Follow Requests"));

    click_label(&window, "Server");
    assert_eq!(window.get_active_mode().as_str(), "server");
    assert_eq!(window.get_active_screen().as_str(), "health");

    click_label(&window, "Accounts & Tokens");
    assert_eq!(window.get_active_screen().as_str(), "accounts");
    assert!(window.get_window_title().contains("Accounts"));

    click_label(&window, "Settings");
    assert_eq!(window.get_active_screen().as_str(), "settings");

    click_label(&window, "Home");
    click_label(&window, "Compose");
    assert_eq!(window.get_active_mode().as_str(), "home");
    assert_eq!(window.get_active_screen().as_str(), "compose");

    let buttons: Vec<_> = i_slint_backend_testing::ElementQuery::from_root(&window)
        .match_accessible_role(i_slint_backend_testing::AccessibleRole::Button)
        .find_all();
    assert!(
        buttons.len() >= 8,
        "expected the simplified social shell to expose core actionable controls, found {}",
        buttons.len()
    );
}

#[test]
fn exercises_normal_owner_task_flows_through_projection() {
    let mut controller = dais_desk::DeskController::fixture_for_tests();

    controller.select_screen("today");
    let projection = controller.projection();
    assert_eq!(projection.active_mode, "home");
    assert_eq!(projection.active_screen, "today");
    assert!(projection
        .rows
        .iter()
        .any(|row| row.id == "timeline:ada-week-friday-space-news"));
    assert!(!projection
        .rows
        .iter()
        .any(|row| row.id.starts_with("delivery:")));

    controller.row_action("timeline:ada-week-friday-space-news", "Save");
    let status = controller.projection().status_message;
    assert!(
        status.contains("owner-only bookmark"),
        "unexpected save status: {status}"
    );

    controller.select_screen("inbox");
    controller.row_action("notification:notice-reply", "Reply");
    let projection = controller.projection();
    assert_eq!(projection.active_screen, "compose");
    assert_eq!(projection.compose_visibility, "followers");
    assert!(projection.status_message.contains("reply"));

    controller.select_mode("people");
    let projection = controller.projection();
    assert_eq!(projection.active_screen, "find");
    assert_eq!(projection.status_message, "Ready.");
    assert!(
        !projection.status_message.contains("reply"),
        "reply workflow status leaked into People/Find"
    );

    controller.select_screen("audience");
    controller.row_action("audience:close-friends", "Use in compose");
    let projection = controller.projection();
    assert_eq!(projection.active_mode, "home");
    assert_eq!(projection.active_screen, "compose");
    assert_eq!(projection.compose_visibility, "direct");
    assert_eq!(projection.compose_audience_list, "close-friends");

    controller.update_compose_from_ui("GUI workflow reply", "", "close-friends", "", false);
    controller.compose_send();
    let status = controller.projection().status_message;
    assert!(
        status.contains("Preview post prepared"),
        "unexpected compose status: {status}"
    );

    controller.select_mode("people");
    controller.select_screen("followers");
    controller.row_action("follower:https://new.example/users/follower", "Approve");
    assert!(controller.projection().status_message.contains("approved"));
}

/// Repro for #360: the account shown as selected in the top-right instance
/// ComboBox was observed to differ from `active_account_id` across separate
/// cold launches. Rather than only asserting on the projection (which is
/// already covered and already deterministic), read the actual rendered
/// ComboBox widget through the accessibility tree across many fresh windows,
/// the same way real automation/VoiceOver would see it.
#[test]
fn repro_360_rendered_account_combobox_matches_active_account_on_every_cold_launch() {
    i_slint_backend_testing::init_no_event_loop();

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let settings_path = temp_dir.path().join("owner-settings.json");
    std::fs::write(
        &settings_path,
        serde_json::json!({
            "instance_url": "https://account-b.invalid.example",
            "owner_token": "token-b",
            "accounts": [
                {
                    "id": "account-a",
                    "label": "Account A",
                    "instance_url": "https://account-a.invalid.example",
                    "owner_token": "token-a"
                },
                {
                    "id": "account-b",
                    "label": "Account B",
                    "instance_url": "https://account-b.invalid.example",
                    "owner_token": "token-b"
                }
            ],
            "active_account_id": "account-b"
        })
        .to_string(),
    )
    .expect("write settings");

    for iteration in 0..15 {
        let window = dais_desk::create_live_test_window(settings_path.clone())
            .expect("construct live window");

        assert_eq!(
            window.get_active_account_label().as_str(),
            "Account B",
            "iteration {iteration}: projection-level active account label drifted"
        );

        let combo = ElementQuery::from_root(&window)
            .match_accessible_role(AccessibleRole::Combobox)
            .find_first()
            .expect("account switcher ComboBox should be reachable in the accessibility tree");
        let rendered_value = combo.accessible_value();
        assert!(
            rendered_value
                .as_deref()
                .is_some_and(|value| value.contains("Account B")),
            "iteration {iteration}: rendered ComboBox showed {rendered_value:?} instead of Account B"
        );
        assert!(
            !rendered_value
                .as_deref()
                .is_some_and(|value| value.contains("Account A")),
            "iteration {iteration}: rendered ComboBox showed the inactive account instead"
        );
    }
}

fn nav_items(model: slint::ModelRc<dais_desk::NavItem>) -> Vec<(String, String)> {
    model
        .iter()
        .map(|item| (item.id.to_string(), item.title.to_string()))
        .collect()
}

/// #371: navigate the way a real user does — click every `mode_nav()` button,
/// then click every `screen_nav()` button it exposes — instead of jumping
/// straight to a screen id via `invoke_select_screen`. This is the mechanism
/// that would have caught #365/#366 (Server mode and several People screens
/// silently unreachable while their own tests, which called
/// `invoke_select_screen` directly, kept passing).
#[test]
fn every_mode_and_screen_nav_button_is_reachable_by_clicking_through() {
    i_slint_backend_testing::init_no_event_loop();
    let window = dais_desk::create_test_window().expect("test fixture window");

    let modes = nav_items(window.get_mode_nav());
    assert!(
        !modes.is_empty(),
        "expected at least one entry in mode_nav()"
    );

    let mut visited_screens = Vec::new();

    for (mode_id, mode_title) in &modes {
        click_label(&window, mode_title);
        assert_eq!(
            window.get_active_mode().as_str(),
            mode_id.as_str(),
            "clicking mode nav button {mode_title:?} did not navigate to mode {mode_id:?}"
        );

        let screens = nav_items(window.get_screen_nav());
        assert!(
            !screens.is_empty(),
            "mode {mode_id:?} exposed no screens via screen_nav()"
        );

        for (screen_id, screen_title) in &screens {
            click_label(&window, screen_title);
            assert_eq!(
                window.get_active_screen().as_str(),
                screen_id.as_str(),
                "clicking screen nav button {screen_title:?} in mode {mode_id:?} did not navigate to screen {screen_id:?}"
            );
            assert_eq!(
                window.get_active_mode().as_str(),
                mode_id.as_str(),
                "navigating to screen {screen_id:?} unexpectedly changed the active mode"
            );
            visited_screens.push(screen_id.clone());
        }
    }

    // Screens that exist in the row-rendering code but aren't exposed by any
    // mode_nav()/screen_nav() entry would silently vanish from this list
    // instead of failing loudly — assert the ones dais_desk::expected_reachable_screens()
    // (shared with visual_smoke.rs's screenshot-coverage check, see #373) describe
    // are all present, so a future screen_nav edit that drops one is caught
    // here rather than discovered by manual review months later.
    for screen in dais_desk::expected_reachable_screens() {
        assert!(
            visited_screens.iter().any(|id| id == screen),
            "expected screen {screen:?} to be reachable via real mode_nav/screen_nav navigation, \
             but it was not in {visited_screens:?}"
        );
    }
}
