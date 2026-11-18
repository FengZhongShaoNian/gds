use super::{fit_size, initial_size, minimum_size};

#[test]
fn window_size_constants_are_ordered() {
    let init = initial_size();
    let min = minimum_size();
    assert!(init.width.0 > min.width.0);
    assert!(init.height.0 > min.height.0);
    assert_eq!(fit_size(init), init);
}
