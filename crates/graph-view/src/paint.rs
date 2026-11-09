use egui::epaint::CubicBezierShape;
use egui::{Align2, Color32, FontId, Painter, Pos2, Rect, Stroke, Vec2};
use mathseea_model::{Highlight, ImportGraph, Layout, Rgb, Selection};

use crate::camera::Camera;
use crate::{ViewEvent, ViewOptions};

pub(crate) struct Scene<'a> {
    pub(crate) graph: &'a ImportGraph,
    pub(crate) layout: &'a Layout,
    pub(crate) highlight: &'a Highlight,
    pub(crate) selection: &'a Selection,
    pub(crate) options: &'a ViewOptions,
    pub(crate) camera: &'a Camera,
    pub(crate) viewport: Rect,
}

impl Scene<'_> {
    pub(crate) fn paint(&self, painter: &Painter, click: Option<Pos2>) -> Option<ViewEvent> {
        if self.options.show_edges {
            self.paint_edges(painter);
        }
        let mut node_hit = None;
        let mut nearest_distance = f32::INFINITY;
        let clip = painter.clip_rect();
        let zoom = self.camera.relative_zoom();
        for (id, (node, placement)) in self.graph.nodes.iter().zip(&self.layout.nodes).enumerate() {
            let position = self.camera.project(placement.position, self.viewport);
            let radius = self.camera.radius(placement.radius);
            let selected = match self.selection {
                Selection::Node(selected) => *selected == id,
                Selection::Topic(topic) => *topic == node.topic,
                Selection::None => false,
            };
            let related = !self.highlight.active || self.highlight.nodes[id] || selected;
            let color = color(node.color, related);
            let bounds = Rect::from_center_size(position, Vec2::splat(radius * 2.0 + 4.0));
            if clip.intersects(bounds) {
                painter.circle_filled(position, radius, color);
                if selected {
                    painter.circle_stroke(position, radius + 2.0, Stroke::new(1.5, Color32::WHITE));
                }
            }
            if let Some(pointer) = click {
                let distance = pointer.distance(position);
                if distance <= radius.max(6.0) && distance < nearest_distance {
                    node_hit = Some(ViewEvent::SelectNode(id));
                    nearest_distance = distance;
                }
            }
            if self.options.show_node_labels && (selected || (zoom >= 2.5 && radius >= 3.0)) {
                let name = if selected || zoom >= 8.0 {
                    node.name.as_str()
                } else {
                    node.name.rsplit('.').next().unwrap_or(&node.name)
                };
                paint_label(
                    painter,
                    position + Vec2::new(radius + 4.0, 0.0),
                    Align2::LEFT_CENTER,
                    name,
                    FontId::proportional(if selected { 14.0 } else { 12.0 }),
                    color,
                );
            }
        }
        let mut topic_hit = None;
        if self.options.show_topic_labels {
            for label in &self.layout.topics {
                let selected =
                    matches!(self.selection, Selection::Topic(topic) if *topic == label.topic);
                let related =
                    !self.highlight.active
                        || selected
                        || self.graph.nodes.iter().enumerate().any(|(id, node)| {
                            node.topic == label.topic && self.highlight.nodes[id]
                        });
                let position = self.camera.project(label.position, self.viewport);
                let bounds = paint_label(
                    painter,
                    position,
                    Align2::CENTER_BOTTOM,
                    &label.topic,
                    FontId::proportional((16.0 + zoom.log2() * 2.0).clamp(12.0, 24.0)),
                    color(label.color, related),
                );
                if click.is_some_and(|pointer| bounds.contains(pointer)) {
                    topic_hit = Some(ViewEvent::SelectTopic(label.topic.clone()));
                }
            }
        }
        // Text has no independent egui widget: a label can never consume a node click.
        node_hit
            .or(topic_hit)
            .or_else(|| click.map(|_| ViewEvent::ClearSelection))
    }

    fn paint_edges(&self, painter: &Painter) {
        for (index, edge) in self.graph.edges.iter().enumerate() {
            let start = self
                .camera
                .project(self.layout.nodes[edge.dependency].position, self.viewport);
            let end = self
                .camera
                .project(self.layout.nodes[edge.dependent].position, self.viewport);
            let middle_x = start.x * 0.5 + end.x * 0.5;
            let points = [
                start,
                Pos2::new(middle_x, start.y),
                Pos2::new(middle_x, end.y),
                end,
            ];
            // A Bézier is contained in its control-point hull, even if both ends are offscreen.
            if !painter
                .clip_rect()
                .intersects(Rect::from_points(&points).expand(2.0))
            {
                continue;
            }
            let related = !self.highlight.active || self.highlight.edges[index];
            let base = self.graph.nodes[edge.dependency].color;
            let edge_color = Color32::from_rgb(base.r, base.g, base.b).gamma_multiply(if related {
                0.45
            } else {
                0.06
            });
            painter.add(CubicBezierShape::from_points_stroke(
                points,
                false,
                Color32::TRANSPARENT,
                Stroke::new(if related { 1.0 } else { 0.6 }, edge_color),
            ));
        }
    }
}

fn color(rgb: Rgb, related: bool) -> Color32 {
    let color = Color32::from_rgb(rgb.r, rgb.g, rgb.b);
    if related {
        color
    } else {
        color.gamma_multiply(0.16)
    }
}

fn paint_label(
    painter: &Painter,
    position: Pos2,
    align: Align2,
    text: &str,
    font: FontId,
    color: Color32,
) -> Rect {
    let galley = painter.layout_no_wrap(text.to_owned(), font, color);
    let bounds = align.anchor_size(position, galley.size());
    if painter.clip_rect().intersects(bounds) {
        painter.galley(bounds.min, galley, color);
    }
    bounds
}
