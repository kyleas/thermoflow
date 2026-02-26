use egui::{Color32, Pos2, Rect, Stroke, Vec2};
use tf_project::schema::{ComponentKind, NodeKind};

pub fn draw_node_symbol(
    painter: &egui::Painter,
    node_kind: &NodeKind,
    center: Pos2,
    radius: f32,
    color: Color32,
    is_boundary: bool,
) {
    match node_kind {
        NodeKind::Junction => {
            // Junction as solid dot
            painter.circle_filled(center, radius * 0.6, color);
        }
        NodeKind::ControlVolume { .. } => {
            let size = Vec2::new(radius * 2.2, radius * 1.4);
            let rect = Rect::from_center_size(center, size);
            painter.rect_stroke(rect, radius * 0.3, Stroke::new(2.0, color));
        }
    }

    if is_boundary {
        let bar = Rect::from_center_size(
            center + Vec2::new(-radius * 1.2, 0.0),
            Vec2::new(radius * 0.3, radius * 1.5),
        );
        painter.rect_filled(bar, 0.0, color);
    }
}

pub fn draw_component_symbol(
    painter: &egui::Painter,
    component_kind: &ComponentKind,
    center: Pos2,
    color: Color32,
) {
    match component_kind {
        ComponentKind::Orifice { .. } => {
            let w = 14.0;
            painter.line_segment(
                [center + Vec2::new(-w, -w), center + Vec2::new(w, w)],
                Stroke::new(2.0, color),
            );
            painter.line_segment(
                [center + Vec2::new(-w, w), center + Vec2::new(w, -w)],
                Stroke::new(2.0, color),
            );
        }
        ComponentKind::Valve { .. } => {
            let w = 12.0;
            painter.line_segment(
                [center + Vec2::new(-w, -w), center + Vec2::new(0.0, 0.0)],
                Stroke::new(2.0, color),
            );
            painter.line_segment(
                [center + Vec2::new(w, -w), center + Vec2::new(0.0, 0.0)],
                Stroke::new(2.0, color),
            );
        }
        ComponentKind::Pipe { .. } => {
            painter.circle_stroke(center, 6.0, Stroke::new(2.0, color));
        }
        ComponentKind::Pump { .. } => {
            painter.circle_stroke(center, 10.0, Stroke::new(2.0, color));
            painter.line_segment(
                [center, center + Vec2::new(10.0, 0.0)],
                Stroke::new(2.0, color),
            );
        }
        ComponentKind::Turbine { .. } => {
            painter.circle_stroke(center, 10.0, Stroke::new(2.0, color));
            painter.line_segment(
                [center, center + Vec2::new(-10.0, 0.0)],
                Stroke::new(2.0, color),
            );
        }
    }
}
