use craft::behavior::kurbo::{Affine, BezPath, PathEl, Point, Vec2, flatten};
use craft::egui::{self, Color32, Pos2, epaint::Mesh};
use craft::font_catalog::FontOutline;

struct Edge {
    start: Point,
    end: Point,
    winding: i32,
}

impl Edge {
    fn new(start: Point, end: Point) -> Option<Self> {
        if !start.is_finite() || !end.is_finite() || start.y == end.y {
            return None;
        }
        Some(if start.y < end.y {
            Self {
                start,
                end,
                winding: 1,
            }
        } else {
            Self {
                start: end,
                end: start,
                winding: -1,
            }
        })
    }

    fn x(&self, y: f64) -> f64 {
        self.start.x
            + (self.end.x - self.start.x) * (y - self.start.y) / (self.end.y - self.start.y)
    }

    fn pos(&self, y: f64) -> Pos2 {
        egui::pos2(self.x(y) as f32, y as f32)
    }
}

pub(super) fn glyph(font: &FontOutline<'_>, id: u32, scale: f64, feather: f64) -> Option<Mesh> {
    let mut path: BezPath = font.outline(id).ok()?;
    if path.elements().len() > 2048 {
        return None;
    }
    path.apply_affine(Affine::scale_non_uniform(scale, -scale));
    let mut contours: Vec<Vec<Point>> = Vec::new();
    let mut points = 0;
    flatten(path, 0.18, |element| {
        if points < 1536 {
            match element {
                PathEl::MoveTo(point) => {
                    contours.push(vec![point]);
                    points += 1;
                }
                PathEl::LineTo(point) => {
                    if let Some(contour) = contours.last_mut()
                        && contour.last() != Some(&point)
                    {
                        contour.push(point);
                        points += 1;
                    }
                }
                _ => {}
            }
        }
    });
    if points >= 1536 {
        return None;
    }
    for contour in &mut contours {
        if contour.len() > 1 && contour.first() == contour.last() {
            contour.pop();
        }
    }
    let edges: Vec<_> = contours
        .iter()
        .flat_map(|contour| segments(contour))
        .filter_map(|(a, b)| Edge::new(a, b))
        .collect();
    let mut mesh = fill(&edges);
    if !mesh.is_empty() {
        for contour in &contours {
            feather_contour(&mut mesh, contour, &edges, feather);
        }
    }
    Some(mesh)
}

fn segments(points: &[Point]) -> impl Iterator<Item = (Point, Point)> + '_ {
    points
        .iter()
        .copied()
        .zip(points.iter().copied().cycle().skip(1))
}

/// Add a one-pixel coverage ramp on the empty side of a visible contour.
fn feather_contour(mesh: &mut Mesh, contour: &[Point], edges: &[Edge], width: f64) {
    let contains = |point: Point| {
        edges
            .iter()
            .filter(|edge| {
                edge.start.y <= point.y && point.y < edge.end.y && edge.x(point.y) > point.x
            })
            .map(|edge| edge.winding)
            .sum::<i32>()
            != 0
    };
    let normals: Vec<_> = segments(contour)
        .map(|(start, end)| {
            let delta = end - start;
            let length = delta.hypot();
            if length <= 1e-6 {
                return None;
            }
            let normal = Vec2::new(-delta.y / length, delta.x / length);
            let middle = start + delta * 0.5;
            let probe = normal * 1e-4;
            match (contains(middle + probe), contains(middle - probe)) {
                (false, true) => Some(normal),
                (true, false) => Some(-normal),
                _ => None,
            }
        })
        .collect();
    let joined = |normal: Vec2, neighbor: Option<Vec2>| {
        neighbor.map_or(normal, |other| {
            (normal + other) / (1.0 + normal.dot(other)).max(0.5)
        })
    };
    let pos = |point: Point| egui::pos2(point.x as f32, point.y as f32);
    for (i, (start, end)) in segments(contour).enumerate() {
        if let Some(normal) = normals[i] {
            let outer_start =
                start + joined(normal, normals[(i + normals.len() - 1) % normals.len()]) * width;
            let outer_end = end + joined(normal, normals[(i + 1) % normals.len()]) * width;
            let at = mesh.vertices.len() as u32;
            mesh.colored_vertex(pos(start), Color32::WHITE);
            mesh.colored_vertex(pos(end), Color32::WHITE);
            mesh.colored_vertex(pos(outer_end), Color32::TRANSPARENT);
            mesh.colored_vertex(pos(outer_start), Color32::TRANSPARENT);
            mesh.add_triangle(at, at + 1, at + 2);
            mesh.add_triangle(at, at + 2, at + 3);
        }
    }
}

/// Fill the flattened contours according to their nonzero winding. Each span is a
/// trapezoid between two edges, with bands split where contour edges meet or cross.
fn fill(edges: &[Edge]) -> Mesh {
    let mut cuts: Vec<_> = edges
        .iter()
        .flat_map(|edge| [edge.start.y, edge.end.y])
        .collect();
    for (index, a) in edges.iter().enumerate() {
        for b in &edges[index + 1..] {
            let top = a.start.y.max(b.start.y);
            let bottom = a.end.y.min(b.end.y);
            if top >= bottom {
                continue;
            }
            let from = a.x(top) - b.x(top);
            let to = a.x(bottom) - b.x(bottom);
            if from * to < 0.0 {
                cuts.push(top + (bottom - top) * from / (from - to));
            }
            if cuts.len() > 4096 {
                return Mesh::default();
            }
        }
    }
    cuts.sort_unstable_by(f64::total_cmp);
    cuts.dedup_by(|a, b| (*a - *b).abs() < 1e-6);

    let mut mesh = Mesh::default();
    let mut active = Vec::new();
    for band in cuts.windows(2) {
        let (top, bottom) = (band[0], band[1]);
        let middle = (top + bottom) * 0.5;
        active.clear();
        active.extend(
            edges
                .iter()
                .filter(|edge| edge.start.y < middle && middle < edge.end.y),
        );
        active.sort_unstable_by(|a, b| a.x(middle).total_cmp(&b.x(middle)));
        let mut winding = 0;
        let mut left = None;
        for edge in &active {
            let outer = winding == 0;
            winding += edge.winding;
            if outer {
                left = Some(*edge);
            } else if winding == 0
                && let Some(left) = left.take()
            {
                let start = mesh.vertices.len() as u32;
                for point in [
                    left.pos(top),
                    edge.pos(top),
                    edge.pos(bottom),
                    left.pos(bottom),
                ] {
                    mesh.colored_vertex(point, Color32::WHITE);
                }
                mesh.add_triangle(start, start + 1, start + 2);
                mesh.add_triangle(start, start + 2, start + 3);
            }
        }
        if mesh.vertices.len() > 24_000 {
            return Mesh::default();
        }
    }
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nonzero_winding_preserves_letter_counters() {
        let outline = [(0., 0.), (10., 0.), (10., 10.), (0., 10.)];
        let counter = [(3., 3.), (3., 7.), (7., 7.), (7., 3.)];
        let edges = [outline, counter]
            .into_iter()
            .flat_map(|contour| {
                (0..4).filter_map(move |index| {
                    Edge::new(contour[index].into(), contour[(index + 1) % 4].into())
                })
            })
            .collect::<Vec<_>>();
        let mesh = fill(&edges);
        let area = mesh
            .indices
            .as_chunks::<3>()
            .0
            .iter()
            .map(|triangle| {
                let points = triangle.map(|index| mesh.vertices[index as usize].pos);
                let a = points[1] - points[0];
                let b = points[2] - points[0];
                (a.x * b.y - a.y * b.x).abs() / 2.0
            })
            .sum::<f32>();
        assert_eq!(area, 100. - 16.);
        let mut mesh = mesh;
        feather_contour(&mut mesh, &outer_points(outline), &edges, 1.0);
        feather_contour(&mut mesh, &outer_points(counter), &edges, 1.0);
        assert!(
            mesh.vertices
                .iter()
                .any(|vertex| { vertex.color == Color32::TRANSPARENT && vertex.pos.y == -1.0 })
        );
        assert!(mesh.vertices.iter().any(|vertex| {
            vertex.color == Color32::TRANSPARENT && vertex.pos == egui::pos2(4.0, 4.0)
        }));
    }

    fn outer_points(points: [(f64, f64); 4]) -> [Point; 4] {
        points.map(Point::from)
    }
}
