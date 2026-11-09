use std::collections::HashSet;

use egui::epaint::CubicBezierShape;
use egui::{Align, Align2, Color32, FontId, Painter, Pos2, Rect, Stroke, Vec2};
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
        self.paint_impl::<false>(painter, click)
    }

    #[cfg(test)]
    pub(super) fn paint_reference(
        &self,
        painter: &Painter,
        click: Option<Pos2>,
    ) -> Option<ViewEvent> {
        self.paint_impl::<true>(painter, click)
    }

    fn paint_impl<const REFERENCE: bool>(
        &self,
        painter: &Painter,
        click: Option<Pos2>,
    ) -> Option<ViewEvent> {
        if self.options.show_edges {
            self.paint_edges(painter);
        }
        let mut node_hit = None;
        let mut nearest_distance = f32::INFINITY;
        let clip = painter.clip_rect();
        let zoom = self.camera.relative_zoom();
        let mut highlighted_topics =
            (!REFERENCE && self.options.show_topic_labels && self.highlight.active)
                .then(HashSet::new);
        for (id, (node, placement)) in self.graph.nodes.iter().zip(&self.layout.nodes).enumerate() {
            if let Some(topics) = &mut highlighted_topics
                && self.highlight.nodes[id]
            {
                topics.insert(node.topic.as_str());
            }
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
                paint_label::<REFERENCE>(
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
                        || if REFERENCE {
                            self.graph.nodes.iter().enumerate().any(|(id, node)| {
                                node.topic == label.topic && self.highlight.nodes[id]
                            })
                        } else {
                            highlighted_topics
                                .as_ref()
                                .is_some_and(|topics| topics.contains(label.topic.as_str()))
                        };
                let position = self.camera.project(label.position, self.viewport);
                let bounds = paint_label::<REFERENCE>(
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

// Only Min alignment fixes an exact lower bound without knowing text size.
// Center/Max anchors may extend into the clip, so leave them to precise layout.
fn label_is_definitely_offscreen(position: Pos2, align: Align2, clip: Rect) -> bool {
    position.is_finite()
        && clip.is_finite()
        && ((align.x() == Align::Min && position.x > clip.right())
            || (align.y() == Align::Min && position.y > clip.bottom()))
}

fn paint_label<const REFERENCE: bool>(
    painter: &Painter,
    position: Pos2,
    align: Align2,
    text: &str,
    font: FontId,
    color: Color32,
) -> Rect {
    if !REFERENCE && label_is_definitely_offscreen(position, align, painter.clip_rect()) {
        return Rect::NOTHING;
    }
    let galley = painter.layout_no_wrap(text.to_owned(), font, color);
    let bounds = align.anchor_size(position, galley.size());
    if painter.clip_rect().intersects(bounds) {
        painter.galley(bounds.min, galley, color);
    }
    bounds
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn early_culling_proves_nonintersection_for_all_alignments() {
        let clip = Rect::from_min_max(Pos2::ZERO, Pos2::new(100.0, 100.0));
        for align in [Align::Min, Align::Center, Align::Max]
            .into_iter()
            .flat_map(|x| [Align::Min, Align::Center, Align::Max].map(|y| Align2([x, y])))
        {
            for x in [-300.0, -1.0, 0.0, 100.0, 100.001, 300.0] {
                for y in [-300.0, -1.0, 0.0, 100.0, 100.001, 300.0] {
                    let position = Pos2::new(x, y);
                    if label_is_definitely_offscreen(position, align, clip) {
                        for size in [Vec2::ZERO, Vec2::splat(1.0), Vec2::new(1000.0, 500.0)] {
                            assert!(!clip.intersects(align.anchor_size(position, size)));
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn culling_keeps_boundary_centered_and_incoming_long_labels() {
        let clip = Rect::from_min_max(Pos2::ZERO, Pos2::new(100.0, 100.0));
        for (position, align) in [
            (Pos2::new(100.0, 50.0), Align2::LEFT_CENTER),
            (Pos2::new(-200.0, 50.0), Align2::LEFT_CENTER),
            (Pos2::new(50.0, 100.0), Align2::LEFT_TOP),
            (Pos2::new(50.0, -1.0), Align2::CENTER_BOTTOM),
            (Pos2::new(110.0, 50.0), Align2::CENTER_CENTER),
            (Pos2::new(110.0, 50.0), Align2::RIGHT_CENTER),
        ] {
            assert!(!label_is_definitely_offscreen(position, align, clip));
        }
        assert!(!label_is_definitely_offscreen(
            Pos2::new(f32::NAN, 0.0),
            Align2::LEFT_TOP,
            clip,
        ));
    }

    #[test]
    fn definitely_offscreen_label_does_not_enter_font_layout_cache() {
        let context = egui::Context::default();
        let clip = Rect::from_min_max(Pos2::ZERO, Pos2::new(100.0, 100.0));
        let output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(clip),
                ..Default::default()
            },
            |ui| {
                let painter = ui.painter_at(clip);
                let before = painter.fonts(|fonts| fonts.num_galleys_in_cache());
                let bounds = paint_label::<false>(
                    &painter,
                    Pos2::new(101.0, 50.0),
                    Align2::LEFT_CENTER,
                    "offscreen_unique_label",
                    FontId::proportional(12.0),
                    Color32::WHITE,
                );
                assert_eq!(bounds, Rect::NOTHING);
                assert_eq!(before, painter.fonts(|fonts| fonts.num_galleys_in_cache()));
                let reference = paint_label::<true>(
                    &painter,
                    Pos2::new(101.0, 50.0),
                    Align2::LEFT_CENTER,
                    "offscreen_unique_label",
                    FontId::proportional(12.0),
                    Color32::WHITE,
                );
                assert!(!clip.intersects(reference));
                assert!(painter.fonts(|fonts| fonts.num_galleys_in_cache()) > before);
            },
        );
        output.drop_without_applying_deltas();
    }
}
