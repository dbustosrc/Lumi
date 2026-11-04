use super::*;

use windows::Win32::Graphics::Gdi::{CreateRectRgn, DeleteObject, HRGN, PtInRegion};

struct Region(HRGN);

impl Region {
    fn rectangle(left: i32, top: i32, right: i32, bottom: i32) -> Self {
        let region = unsafe { CreateRectRgn(left, top, right, bottom) };
        assert!(!region.is_invalid(), "CreateRectRgn should return a region");
        Self(region)
    }

    fn contains(&self, x: i32, y: i32) -> bool {
        unsafe { PtInRegion(self.0, x, y) }.as_bool()
    }
}

impl Drop for Region {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteObject(self.0.into());
        }
    }
}

#[test]
fn centered_input_region_excludes_sides_for_compact_peek_and_open_states() {
    assert_eq!(centered_region_edges(584, 174), (205, 379));
    assert_eq!(centered_region_edges(584, 346), (119, 465));
    assert_eq!(centered_region_edges(584, 584), (0, 584));

    let compact = Region::rectangle(205, 0, 379, 34);
    assert!(!compact.contains(204, 10));
    assert!(compact.contains(205, 10));
    assert!(compact.contains(378, 10));
    assert!(!compact.contains(379, 10));
    let center_sample_offset = 93;
    assert!(!compact.contains(292 - center_sample_offset, 10));
    assert!(!compact.contains(292 + center_sample_offset, 10));

    let peek = Region::rectangle(119, 0, 465, 66);
    assert!(!peek.contains(118, 10));
    assert!(peek.contains(119, 10));
    assert!(peek.contains(464, 10));
    assert!(!peek.contains(465, 10));

    let expanded = Region::rectangle(0, 0, 584, 148);
    assert!(expanded.contains(0, 0));
    assert!(expanded.contains(583, 147));
    assert!(!expanded.contains(584, 147));
}

#[test]
fn centered_input_region_rounds_scaled_widths_without_shifting_center() {
    for (surface_width, region_width, expected) in [(730, 218, (256, 474)), (876, 261, (307, 568))]
    {
        let edges = centered_region_edges(surface_width, region_width);
        assert_eq!(edges, expected);
        assert!((edges.0 + edges.1 - surface_width).abs() <= 1);

        let region = Region::rectangle(edges.0, 0, edges.1, 50);
        assert!(!region.contains(edges.0 - 1, 1));
        assert!(region.contains(edges.0, 1));
        assert!(region.contains(edges.1 - 1, 1));
        assert!(!region.contains(edges.1, 1));
        if surface_width == 876 {
            let center = surface_width / 2;
            let offset = 140;
            assert!(!region.contains(center - offset, 1));
            assert!(!region.contains(center + offset, 1));
        }
    }
}
