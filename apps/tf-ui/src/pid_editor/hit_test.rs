use egui::{Pos2, Rect};

pub fn hit_test_rect(center: Pos2, size: f32, point: Pos2) -> bool {
    let rect = Rect::from_center_size(center, egui::vec2(size, size));
    rect.contains(point)
}
