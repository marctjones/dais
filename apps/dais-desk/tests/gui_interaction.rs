use i_slint_backend_testing::ElementHandle;
use slint::platform::PointerEventButton;

fn click_label(window: &dais_desk::MainWindow, label: &str) {
    let matches: Vec<_> = ElementHandle::find_by_accessible_label(window, label).collect();
    assert!(
        !matches.is_empty(),
        "expected to find an accessible control labelled {label:?}"
    );
    matches[0].mock_single_click(PointerEventButton::Left);
    slint::platform::update_timers_and_animations();
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

    click_label(&window, "Conversations");
    assert_eq!(window.get_active_mode().as_str(), "home");
    assert_eq!(window.get_active_screen().as_str(), "conversations");

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
