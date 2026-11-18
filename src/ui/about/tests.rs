use super::Application;

#[test]
fn registered_files_application_metadata_is_present() {
    let app = Application::cargo("gds-files");
    assert_eq!(app.name, "GDS Files");
}
