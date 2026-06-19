use i_slint_backend_testing::ElementHandle;
use slint::platform::PointerEventButton;

fn click_label(window: &dais_desk::MainWindow, label: &str) {
    let matches: Vec<_> = ElementHandle::find_by_accessible_label(window, label).collect();
    assert!(
        !matches.is_empty(),
        "expected to find an accessible control labelled {label:?}"
    );
    matches[0].mock_single_click(PointerEventButton::Left);
}

#[test]
fn navigates_primary_workflows_through_accessible_controls() {
    i_slint_backend_testing::init_no_event_loop();

    let window = dais_desk::create_test_window().expect("test fixture window");
    assert_eq!(window.get_active_mode().as_str(), "home");
    assert_eq!(window.get_active_screen().as_str(), "today");

    click_label(&window, "People");
    assert_eq!(window.get_active_mode().as_str(), "people");
    assert_eq!(window.get_active_screen().as_str(), "find");

    click_label(&window, "Followers");
    assert_eq!(window.get_active_screen().as_str(), "followers");
    assert!(window.get_window_title().contains("Followers"));

    click_label(&window, "Server");
    assert_eq!(window.get_active_mode().as_str(), "server");
    assert_eq!(window.get_active_screen().as_str(), "health");

    click_label(&window, "Accounts & Tokens");
    assert_eq!(window.get_active_screen().as_str(), "accounts");
    assert!(window
        .get_window_subtitle()
        .contains("Multiple Dais instances"));

    let buttons: Vec<_> = i_slint_backend_testing::ElementQuery::from_root(&window)
        .match_accessible_role(i_slint_backend_testing::AccessibleRole::Button)
        .find_all();
    assert!(
        buttons.len() >= 12,
        "expected the feature-complete shell to expose many actionable controls, found {}",
        buttons.len()
    );
}
