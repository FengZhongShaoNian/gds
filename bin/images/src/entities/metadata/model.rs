//! Draft state holding inspected container metadata, 7 editable EXIF/PNG fields, and desktop tags.

use gds::service::image::metadata::{EditableFields, MetadataBundle};

#[derive(Clone, Debug)]
pub struct MetadataState {
    pub bundle: MetadataBundle,
    pub draft: EditableFields,
    pub original_tags: Vec<String>,
    pub tags: Vec<String>,
    pub metadata_dirty: bool,
    pub tags_dirty: bool,
}

impl MetadataState {
    pub fn from_bundle(bundle: MetadataBundle, tags: Vec<String>) -> Self {
        let draft = bundle.editable.clone();
        Self {
            bundle,
            draft,
            original_tags: tags.clone(),
            tags,
            metadata_dirty: false,
            tags_dirty: false,
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.metadata_dirty || self.tags_dirty
    }
}
