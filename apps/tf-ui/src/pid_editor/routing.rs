use egui::Pos2;

pub fn autoroute(from: Pos2, to: Pos2) -> Vec<Pos2> {
    if (from.x - to.x).abs() < f32::EPSILON || (from.y - to.y).abs() < f32::EPSILON {
        return vec![from, to];
    }

    let mid_x = Pos2::new(to.x, from.y);
    let mid_y = Pos2::new(from.x, to.y);

    let mut path = vec![from, mid_x, to];
    path = normalize_orthogonal(&path);

    if path.len() >= 2 {
        return path;
    }

    normalize_orthogonal(&[from, mid_y, to])
}

pub fn normalize_orthogonal(points: &[Pos2]) -> Vec<Pos2> {
    if points.is_empty() {
        return Vec::new();
    }

    let mut out: Vec<Pos2> = Vec::new();
    for point in points {
        if out
            .last()
            .map(|p| (p.x - point.x).abs() < f32::EPSILON && (p.y - point.y).abs() < f32::EPSILON)
            .unwrap_or(false)
        {
            continue;
        }
        out.push(*point);
    }

    // Remove collinear points
    let mut i = 1usize;
    while i + 1 < out.len() {
        let prev = out[i - 1];
        let curr = out[i];
        let next = out[i + 1];
        let collinear = (prev.x - curr.x).abs() < f32::EPSILON
            && (curr.x - next.x).abs() < f32::EPSILON
            || (prev.y - curr.y).abs() < f32::EPSILON && (curr.y - next.y).abs() < f32::EPSILON;
        if collinear {
            out.remove(i);
        } else {
            i += 1;
        }
    }

    out
}

pub fn is_orthogonal(points: &[Pos2]) -> bool {
    if points.len() < 2 {
        return true;
    }

    for window in points.windows(2) {
        let a = window[0];
        let b = window[1];
        let dx = (a.x - b.x).abs();
        let dy = (a.y - b.y).abs();
        if dx > f32::EPSILON && dy > f32::EPSILON {
            return false;
        }
    }

    true
}

pub fn polyline_midpoint(points: &[Pos2]) -> Pos2 {
    if points.len() < 2 {
        return points.first().copied().unwrap_or(Pos2::ZERO);
    }

    let mut total_len = 0.0;
    for segment in points.windows(2) {
        total_len += (segment[1] - segment[0]).length();
    }
    if total_len <= f32::EPSILON {
        return points[0];
    }

    let half = total_len * 0.5;
    let mut accum = 0.0;
    for segment in points.windows(2) {
        let seg_len = (segment[1] - segment[0]).length();
        if accum + seg_len >= half {
            let t = (half - accum) / seg_len;
            return segment[0] + (segment[1] - segment[0]) * t;
        }
        accum += seg_len;
    }

    *points.last().unwrap_or(&Pos2::ZERO)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn autoroute_is_orthogonal() {
        let from = Pos2::new(0.0, 0.0);
        let to = Pos2::new(10.0, 5.0);
        let route = autoroute(from, to);
        assert!(is_orthogonal(&route));
    }

    #[test]
    fn normalize_removes_duplicates_and_collinear() {
        let points = vec![
            Pos2::new(0.0, 0.0),
            Pos2::new(0.0, 0.0),
            Pos2::new(5.0, 0.0),
            Pos2::new(10.0, 0.0),
            Pos2::new(10.0, 5.0),
        ];
        let normalized = normalize_orthogonal(&points);
        assert_eq!(
            normalized,
            vec![
                Pos2::new(0.0, 0.0),
                Pos2::new(10.0, 0.0),
                Pos2::new(10.0, 5.0)
            ]
        );
    }
}
