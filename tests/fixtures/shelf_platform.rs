use std::sync::Arc;

use crate::domain::shelf::ShelfThumbnail;

#[test]
fn cloned_shelf_thumbnail_shares_its_pixel_buffer() {
    let thumbnail = ShelfThumbnail {
        width: 2,
        height: 2,
        rgba: Arc::new(vec![1, 2, 3, 4]),
    };

    let clone = thumbnail.clone();

    assert!(Arc::ptr_eq(&thumbnail.rgba, &clone.rgba));
    assert_eq!(clone.rgba.as_slice(), [1, 2, 3, 4]);
}
