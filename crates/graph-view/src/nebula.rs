use std::sync::{Arc, Mutex};

use eframe::{egui_wgpu, wgpu};
use egui::{Align2, Color32, FontId, PointerButton, Rect, Sense, Stroke, Vec2};
use mathseea_model::{GraphError, Highlight, ImportGraph, Layout, NodeId, Selection};

use crate::camera3d::{OrbitCamera, Vec3, add, dot, mul, sub};
use crate::input::{DocumentIdentity, validate_document, validate_frame};
use crate::nebula_gpu::{GpuScene, NebulaCallback, Resources};
use crate::{ViewEvent, ViewOptions};

#[derive(Clone, Copy, Debug, PartialEq)]
struct Star {
    position: Vec3,
    radius: f32,
}

/// Derive stable 3D coordinates without mutating the analytical 2D layout or graph IDs.
fn spatial_layout(graph: &ImportGraph, layout: &Layout) -> Result<Vec<Star>, GraphError> {
    let width = (layout.bounds.max.x as f64 - layout.bounds.min.x as f64).max(1e-9);
    let height = (layout.bounds.max.y as f64 - layout.bounds.min.y as f64).max(1e-9);
    let stars: Vec<Star> = graph
        .nodes
        .iter()
        .zip(&layout.nodes)
        .map(|(node, placement)| {
            let hash = node.name.bytes().fold(2166136261_u32, |h, b| {
                (h ^ u32::from(b)).wrapping_mul(16777619)
            });
            Star {
                position: [
                    ((placement.position.x as f64 - layout.bounds.min.x as f64) / width * 3.2 - 1.6)
                        as f32,
                    (1.0 - (placement.position.y as f64 - layout.bounds.min.y as f64) / height
                        * 2.0) as f32,
                    ((hash & 65535) as f32 / 65535.0 - 0.5) * 0.9,
                ],
                radius: ((placement.radius as f64 / width) * 3.2).clamp(0.006, 0.045) as f32,
            }
        })
        .collect();
    if graph
        .edges
        .iter()
        .any(|edge| stars[edge.dependency].position[0] >= stars[edge.dependent].position[0])
    {
        return Err(GraphError::new("3D坐标精度不足以保持严格拓扑X顺序"));
    }
    Ok(stars)
}

fn pick(
    camera: &OrbitCamera,
    viewport: Rect,
    pointer: egui::Pos2,
    stars: &[Star],
) -> Option<NodeId> {
    stars
        .iter()
        .enumerate()
        .filter_map(|(id, star)| {
            camera
                .intersect(pointer, viewport, star.position, star.radius)
                .map(|d| (id, d))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(id, _)| id)
}

fn vertex(bytes: &mut Vec<u8>, position: Vec3, radius: f32, color: [f32; 4]) {
    for value in [position[0], position[1], position[2], radius]
        .into_iter()
        .chain(color)
    {
        bytes.extend(value.to_ne_bytes());
    }
}

/// Keep the arrow inside the free segment between sphere surfaces. A view-aligned
/// segment has no unambiguous screen direction and deliberately has no arrowhead.
fn arrowhead(
    source: Star,
    target: Star,
    camera: &OrbitCamera,
    viewport: Rect,
) -> Option<[Vec3; 3]> {
    let delta = sub(target.position, source.position);
    let length = dot(delta, delta).sqrt();
    let start = source.radius * 1.1;
    let end = length - target.radius * 1.1;
    let free = end - start;
    if !length.is_finite() || free <= 1e-5 || source.radius <= 0.0 || target.radius <= 0.0 {
        return None;
    }
    let direction = mul(delta, length.recip());
    let tip = add(source.position, mul(direction, end));
    let surface_start = add(source.position, mul(direction, start));
    let (from, _) = camera.project(surface_start, viewport)?;
    let (to, _) = camera.project(tip, viewport)?;
    if from.distance(to) < 2.0 {
        return None;
    }
    let sight = sub(mul(add(surface_start, tip), 0.5), camera.basis().0);
    let wing = [
        direction[1] * sight[2] - direction[2] * sight[1],
        direction[2] * sight[0] - direction[0] * sight[2],
        direction[0] * sight[1] - direction[1] * sight[0],
    ];
    let norm = dot(wing, wing).sqrt();
    if !norm.is_finite() || norm <= 1e-6 {
        return None;
    }
    let height = (free * 0.45).min(0.035);
    let base = sub(tip, mul(direction, height));
    let side = mul(wing, height * 0.35 / norm);
    Some([tip, add(base, side), sub(base, side)])
}

/// Native wgpu view. The application owns the same immutable graph and selection in 2D and 3D.
pub struct NebulaView {
    target: wgpu::TextureFormat,
    max_texture: u32,
    resources: Arc<Mutex<Option<Resources>>>,
    identity: Option<DocumentIdentity>,
    document_error: Option<GraphError>,
    stars: Vec<Star>,
    camera: OrbitCamera,
    pending_focus: Option<NodeId>,
    fit_requested: bool,
    suppressed: bool,
}

impl NebulaView {
    pub fn new(state: &egui_wgpu::RenderState) -> Self {
        Self {
            target: state.target_format,
            max_texture: state.device.limits().max_texture_dimension_2d,
            resources: Arc::new(Mutex::new(None)),
            identity: None,
            document_error: None,
            stars: Vec::new(),
            camera: OrbitCamera::default(),
            pending_focus: None,
            fit_requested: true,
            suppressed: false,
        }
    }
    pub fn reset_camera(&mut self) {
        self.camera = OrbitCamera::default();
        self.pending_focus = None;
        self.fit_requested = true;
        self.suppressed = false;
    }
    pub fn reset_document(&mut self) {
        self.identity = None;
        self.document_error = None;
        self.reset_camera();
    }
    pub fn focus_node(&mut self, id: NodeId) {
        self.pending_focus = Some(id);
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        graph: &ImportGraph,
        layout: &Layout,
        highlight: &Highlight,
        selection: &Selection,
        options: &ViewOptions,
    ) -> Vec<ViewEvent> {
        let (viewport, response) =
            ui.allocate_exact_size(ui.available_size().max(Vec2::ZERO), Sense::click_and_drag());
        let painter = ui.painter_at(viewport);
        if !self
            .identity
            .as_ref()
            .is_some_and(|id| id.matches(graph, layout))
        {
            self.document_error = validate_document(graph, layout).err();
            self.stars.clear();
            if self.document_error.is_none() {
                match spatial_layout(graph, layout) {
                    Ok(stars) => self.stars = stars,
                    Err(error) => self.document_error = Some(error),
                }
            }
            self.identity = Some(DocumentIdentity::new(graph, layout));
            let pending = self.pending_focus;
            self.reset_camera();
            self.pending_focus = pending;
        }
        if let Some(error) = self
            .document_error
            .clone()
            .or_else(|| validate_frame(graph, layout, highlight, selection).err())
        {
            painter.text(
                viewport.left_top() + Vec2::splat(16.0),
                Align2::LEFT_TOP,
                error.to_string(),
                FontId::proportional(14.0),
                Color32::LIGHT_RED,
            );
            return Vec::new();
        }
        if viewport.width() < 2.0 || viewport.height() < 2.0 {
            return Vec::new();
        }
        if self.fit_requested {
            self.camera.distance = (4.4 / viewport.aspect_ratio().min(1.0)).min(25.0);
            self.fit_requested = false;
        }
        if let Some(id) = self.pending_focus.take()
            && let Some(star) = self.stars.get(id)
        {
            self.camera.target = star.position;
            self.camera.distance = 0.7;
        }
        let (pressed, down) = ui.input(|i| (i.pointer.any_pressed(), i.pointer.any_down()));
        if pressed {
            self.suppressed = false;
        }
        if response.dragged_by(PointerButton::Primary) {
            if ui.input(|i| i.modifiers.shift) {
                self.camera.pan(response.drag_delta(), viewport);
            } else {
                self.camera.orbit(response.drag_delta());
            }
            self.suppressed = true;
        }
        if response.dragged_by(PointerButton::Middle)
            || response.dragged_by(PointerButton::Secondary)
        {
            self.camera.pan(response.drag_delta(), viewport);
            self.suppressed = true;
        }
        let mut zoomed = false;
        if response.hovered() {
            let factor = ui.input_mut(|i| {
                let factor =
                    (i.smooth_scroll_delta.y * 0.004).clamp(-1.0, 1.0).exp() * i.zoom_delta();
                i.smooth_scroll_delta = Vec2::ZERO;
                factor
            });
            if (factor - 1.0).abs() > f32::EPSILON {
                self.camera.zoom(factor);
                zoomed = true;
                if down {
                    self.suppressed = true;
                }
            }
        }
        let mut nodes = Vec::with_capacity(self.stars.len() * 32);
        for (id, star) in self.stars.iter().enumerate() {
            let color = graph.nodes[id].color;
            let selected = match selection {
                Selection::Node(n) => *n == id,
                Selection::Topic(t) => graph.nodes[id].topic == *t,
                Selection::None => false,
            };
            let intensity = if selected {
                1.2
            } else if !highlight.active || highlight.nodes[id] {
                0.92
            } else {
                0.18
            };
            vertex(
                &mut nodes,
                star.position,
                star.radius,
                [
                    f32::from(color.r) / 255.0 * intensity,
                    f32::from(color.g) / 255.0 * intensity,
                    f32::from(color.b) / 255.0 * intensity,
                    1.0,
                ],
            );
        }
        let mut lines = Vec::new();
        if options.show_edges {
            // Related edges only in selected mode; overview LOD keeps a deterministic sample.
            let stride = graph.edges.len().div_ceil(12000).max(1);
            for (index, edge) in graph.edges.iter().enumerate() {
                if highlight.active {
                    if !highlight.edges[index] {
                        continue;
                    }
                } else if !index.is_multiple_of(stride) {
                    continue;
                }
                let a = self.stars[edge.dependency].position;
                let b = self.stars[edge.dependent].position;
                let color = if highlight.active {
                    [0.40, 0.65, 0.95, 1.0]
                } else {
                    [0.075, 0.12, 0.20, 1.0]
                };
                vertex(&mut lines, a, 0.0, color);
                vertex(&mut lines, b, 0.0, color);
                if highlight.active
                    && let Some([tip, left, right]) = arrowhead(
                        self.stars[edge.dependency],
                        self.stars[edge.dependent],
                        &self.camera,
                        viewport,
                    )
                {
                    for end in [left, right] {
                        vertex(&mut lines, tip, 0.0, color);
                        vertex(&mut lines, end, 0.0, color);
                    }
                }
            }
        }
        let dpi = ui.ctx().pixels_per_point();
        let size = [
            (viewport.width() * dpi)
                .round()
                .clamp(1.0, self.max_texture as f32) as u32,
            (viewport.height() * dpi)
                .round()
                .clamp(1.0, self.max_texture as f32) as u32,
        ];
        painter.add(egui_wgpu::Callback::new_paint_callback(
            viewport,
            NebulaCallback {
                resources: self.resources.clone(),
                scene: Arc::new(GpuScene {
                    nodes,
                    lines,
                    camera: self.camera.uniform(viewport.aspect_ratio()),
                    size,
                }),
                target: self.target,
            },
        ));
        if options.show_node_labels {
            let mut considered = 0;
            for (id, star) in self.stars.iter().enumerate() {
                let selected = matches!(selection,Selection::Node(n) if *n==id);
                if !selected && (!highlight.active || !highlight.nodes[id] || considered >= 20) {
                    continue;
                }
                if !selected {
                    considered += 1;
                }
                if let Some((position, _)) = self.camera.project(star.position, viewport)
                    && viewport.contains(position)
                    && pick(&self.camera, viewport, position, &self.stars) == Some(id)
                {
                    painter.text(
                        position + Vec2::new(8.0, -8.0),
                        Align2::LEFT_BOTTOM,
                        &graph.nodes[id].name,
                        FontId::proportional(12.0),
                        Color32::from_rgb(220, 231, 248),
                    );
                }
            }
        }
        if options.show_topic_labels {
            // Theme labels remain in the navigation rail; the viewport prioritizes readable local labels.
            painter.text(
                viewport.left_top() + Vec2::splat(16.0),
                Align2::LEFT_TOP,
                "3D 星云概览 · 拓扑 X 轴 → 使用者",
                FontId::proportional(12.0),
                Color32::from_rgb(148, 168, 200),
            );
        }
        // Camera-space orientation triad, not a claim about screen-left/right after rotation.
        let origin = viewport.left_bottom() + Vec2::new(38.0, -38.0);
        let (_, right, up, _) = self.camera.basis();
        for (axis, name, color) in [
            ([1.0, 0.0, 0.0], "X", Color32::LIGHT_RED),
            ([0.0, 1.0, 0.0], "Y", Color32::LIGHT_GREEN),
            ([0.0, 0.0, 1.0], "Z", Color32::LIGHT_BLUE),
        ] {
            let end = origin + Vec2::new(dot(axis, right), -dot(axis, up)) * 24.0;
            painter.line_segment([origin, end], Stroke::new(1.5, color));
            painter.text(
                end,
                Align2::CENTER_CENTER,
                name,
                FontId::monospace(10.0),
                color,
            );
        }
        let event = if response.clicked() && !response.drag_stopped() && !self.suppressed && !zoomed
        {
            response
                .interact_pointer_pos()
                .filter(|p| viewport.contains(*p))
                .map(|p| {
                    pick(&self.camera, viewport, p, &self.stars)
                        .map_or(ViewEvent::ClearSelection, ViewEvent::SelectNode)
                })
        } else {
            None
        };
        if !down {
            self.suppressed = false;
        }
        event.into_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mathseea_model::{Edge, ModuleNode, NodePlacement, Point};
    #[test]
    fn derived_coordinates_are_stable_and_preserve_graph_and_topology() {
        let graph = ImportGraph::new(
            vec![ModuleNode::new("Mathlib.A"), ModuleNode::new("Mathlib.B")],
            vec![Edge {
                dependency: 0,
                dependent: 1,
            }],
        )
        .unwrap();
        let layout = Layout::from_nodes(
            &graph,
            vec![
                NodePlacement {
                    position: Point::new(-1.0, 2.0),
                    radius: 0.2,
                },
                NodePlacement {
                    position: Point::new(2.0, 3.0),
                    radius: 0.3,
                },
            ],
        )
        .unwrap();
        let before = layout.clone();
        let a = spatial_layout(&graph, &layout).unwrap();
        assert_eq!(a, spatial_layout(&graph, &layout).unwrap());
        assert!(a[0].position[0] < a[1].position[0]);
        assert_eq!(layout, before);
        assert!(
            a.iter()
                .all(|s| s.position.into_iter().all(f32::is_finite) && s.radius > 0.0)
        );
    }
    #[test]
    fn picking_returns_nearest_visible_id_and_no_hit_for_blank() {
        let camera = OrbitCamera::default();
        let viewport = Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(800.0, 600.0));
        let (_, ray) = camera.ray(viewport.center(), viewport);
        let stars = [
            Star {
                position: [0.0; 3],
                radius: 0.1,
            },
            Star {
                position: mul(ray, 0.5),
                radius: 0.1,
            },
        ];
        assert_eq!(pick(&camera, viewport, viewport.center(), &stars), Some(0));
        assert_eq!(pick(&camera, viewport, viewport.left_top(), &stars), None);
    }

    #[test]
    fn arrowheads_fit_free_segments_and_stay_orthogonal_at_maximum_radius() {
        let mut camera = OrbitCamera::default();
        camera.distance = 0.7;
        let viewport = Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(1280.0, 720.0));
        for (length, radius) in [(0.12, 0.045), (0.025, 0.006), (0.4, 0.045)] {
            for (start, end) in [
                ([0.0; 3], [length, 0.0, 0.0]),
                ([length, 0.0, 0.0], [0.0; 3]),
            ] {
                let source = Star {
                    position: start,
                    radius,
                };
                let target = Star {
                    position: end,
                    radius,
                };
                let [tip, left, right] = arrowhead(source, target, &camera, viewport).unwrap();
                let direction = mul(sub(end, start), length.recip());
                let base = mul(add(left, right), 0.5);
                let along = dot(sub(tip, base), direction);
                assert!(along > 0.0 && along <= 0.035001);
                assert!(dot(sub(base, start), direction) > source.radius);
                assert!(dot(sub(tip, start), direction) < length - target.radius);
                assert!(dot(sub(left, right), direction).abs() < 1e-6);
                assert!(dot(sub(left, right), sub(left, right)) > 0.0);
                assert!(tip.into_iter().chain(left).chain(right).all(f32::is_finite));
            }
        }
        // A camera-right parallel edge still gets a non-degenerate orthogonal wing.
        let target = Star {
            position: mul(camera.basis().1, 0.3),
            radius: 0.045,
        };
        assert!(
            arrowhead(
                Star {
                    position: [0.0; 3],
                    radius: 0.045
                },
                target,
                &camera,
                viewport
            )
            .is_some()
        );
    }

    #[test]
    fn arrowheads_omit_coincident_overlapping_and_view_aligned_directions() {
        let camera = OrbitCamera::default();
        let viewport = Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(1280.0, 720.0));
        let source = Star {
            position: [0.0; 3],
            radius: 0.045,
        };
        for position in [
            [0.0; 3],
            [0.01, 0.0, 0.0],
            [0.09, 0.0, 0.0],
            mul(camera.basis().3, 0.3),
        ] {
            assert!(
                arrowhead(
                    source,
                    Star {
                        position,
                        radius: 0.045
                    },
                    &camera,
                    viewport
                )
                .is_none()
            );
        }
    }

    #[test]
    #[ignore = "manual CPU picking benchmark; committed clean worktree required"]
    fn cpu_picking_benchmark() {
        let count: usize = std::env::var("MATHSEEA_PICK_NODES")
            .unwrap_or_else(|_| "3763".into())
            .parse()
            .unwrap();
        assert!((1..=100000).contains(&count));
        let stars: Vec<_> = (0..count)
            .map(|id| Star {
                position: [
                    (id % 101) as f32 / 100.0 * 3.2 - 1.6,
                    ((id / 101) % 71) as f32 / 70.0 * 2.0 - 1.0,
                    ((id * 37) % 97) as f32 / 96.0 - 0.5,
                ],
                radius: 0.01,
            })
            .collect();
        let camera = OrbitCamera::default();
        let viewport = Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(1280.0, 720.0));
        let mut samples = Vec::with_capacity(600);
        for round in 0..3 {
            for index in 0..230 {
                let pointer =
                    viewport.center() + Vec2::new(((index + round) % 11) as f32 * 3.0, 0.0);
                let start = std::time::Instant::now();
                std::hint::black_box(pick(&camera, viewport, pointer, &stars));
                if index >= 30 {
                    samples.push(start.elapsed().as_nanos());
                }
            }
        }
        samples.sort_unstable();
        println!(
            "nodes={count} viewport=1280x720 samples=600 median_ns={} p95_ns={} scope=CPU-ray-pick-not-GPU",
            samples[299], samples[569]
        );
    }
}
