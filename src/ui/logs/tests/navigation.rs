use super::super::navigation::NavigationStep;

#[test]
fn log_navigation_steps_exist() {
    assert_ne!(NavigationStep::Previous, NavigationStep::Next);
}
