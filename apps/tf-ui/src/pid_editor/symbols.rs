use egui::{Color32, Pos2, Rect, Stroke, Vec2};
use tf_project::schema::{ComponentKind, ControlBlockKindDef, NodeKind};

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
        NodeKind::Atmosphere { .. } => {
            // Atmosphere as open circle with a cap line
            painter.circle_stroke(center, radius * 0.8, Stroke::new(2.0, color));
            painter.line_segment(
                [
                    center + Vec2::new(-radius * 0.6, -radius * 0.2),
                    center + Vec2::new(radius * 0.6, -radius * 0.2),
                ],
                Stroke::new(2.0, color),
            );
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
    scale: f32,
) {
    let scale = scale.clamp(0.2, 8.0);
    let stroke = Stroke::new((2.0 * scale).clamp(1.0, 6.0), color);

    match component_kind {
        ComponentKind::Orifice { .. } => {
            let w = 14.0 * scale;
            painter.line_segment(
                [center + Vec2::new(-w, -w), center + Vec2::new(w, w)],
                stroke,
            );
            painter.line_segment(
                [center + Vec2::new(-w, w), center + Vec2::new(w, -w)],
                stroke,
            );
        }
        ComponentKind::Valve { .. } => {
            let w = 12.0 * scale;
            painter.line_segment(
                [center + Vec2::new(-w, -w), center + Vec2::new(0.0, 0.0)],
                stroke,
            );
            painter.line_segment(
                [center + Vec2::new(w, -w), center + Vec2::new(0.0, 0.0)],
                stroke,
            );
        }
        ComponentKind::Pipe { .. } => {
            painter.circle_stroke(center, 6.0 * scale, stroke);
        }
        ComponentKind::Pump { .. } => {
            painter.circle_stroke(center, 10.0 * scale, stroke);
            painter.line_segment([center, center + Vec2::new(10.0 * scale, 0.0)], stroke);
        }
        ComponentKind::Turbine { .. } => {
            painter.circle_stroke(center, 10.0 * scale, stroke);
            painter.line_segment([center, center + Vec2::new(-10.0 * scale, 0.0)], stroke);
        }
        ComponentKind::LineVolume { .. } => {
            let rect = egui::Rect::from_center_size(center, Vec2::new(20.0 * scale, 12.0 * scale));
            painter.rect_stroke(rect, 0.0, stroke);
        }
    }
}

pub fn draw_control_block_symbol(
    painter: &egui::Painter,
    block_kind: &ControlBlockKindDef,
    center: Pos2,
    color: Color32,
    scale: f32,
) {
    let scale = scale.clamp(0.2, 8.0);
    let half_w = 20.0 * scale;
    let half_h = 14.0 * scale;

    let rect = Rect::from_center_size(center, Vec2::new(half_w * 2.0, half_h * 2.0));
    painter.rect_stroke(
        rect,
        4.0 * scale,
        Stroke::new((2.0 * scale).clamp(1.0, 6.0), color),
    );

    match block_kind {
        ControlBlockKindDef::Constant { .. } => {
            painter.text(
                center,
                egui::Align2::CENTER_CENTER,
                "K",
                egui::FontId::proportional(12.0 * scale),
                color,
            );
        }
        ControlBlockKindDef::MeasuredVariable { .. } => {
            painter.text(
                center,
                egui::Align2::CENTER_CENTER,
                "M",
                egui::FontId::proportional(12.0 * scale),
                color,
            );
        }
        ControlBlockKindDef::PIController { .. } => {
            painter.text(
                center,
                egui::Align2::CENTER_CENTER,
                "PI",
                egui::FontId::proportional(10.0 * scale),
                color,
            );
        }
        ControlBlockKindDef::PIDController { .. } => {
            painter.text(
                center,
                egui::Align2::CENTER_CENTER,
                "PD",
                egui::FontId::proportional(10.0 * scale),
                color,
            );
        }
        ControlBlockKindDef::FirstOrderActuator { .. } => {
            painter.text(
                center,
                egui::Align2::CENTER_CENTER,
                "A",
                egui::FontId::proportional(12.0 * scale),
                color,
            );
        }
        ControlBlockKindDef::ActuatorCommand { .. } => {
            painter.text(
                center,
                egui::Align2::CENTER_CENTER,
                "C",
                egui::FontId::proportional(12.0 * scale),
                color,
            );
        }
    }
}

#[allow(dead_code)]
pub fn get_control_block_type_label(block_kind: &ControlBlockKindDef) -> &'static str {
    match block_kind {
        ControlBlockKindDef::Constant { .. } => "Constant",
        ControlBlockKindDef::MeasuredVariable { .. } => "Measured",
        ControlBlockKindDef::PIController { .. } => "PI Controller",
        ControlBlockKindDef::PIDController { .. } => "PID Controller",
        ControlBlockKindDef::FirstOrderActuator { .. } => "Actuator",
        ControlBlockKindDef::ActuatorCommand { .. } => "Command",
    }
}
