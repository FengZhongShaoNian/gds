use super::{DirectoryPicker, DirectoryPickerView};

#[test]
fn directory_picker_types_compile() {
    let _ = (DirectoryPicker::new, DirectoryPickerView);
}
