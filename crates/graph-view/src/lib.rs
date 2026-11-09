//! In-process egui graph rendering. The caller owns documents, layouts and relationship analysis.

mod camera;
mod input;
mod paint;

use egui::{Align2, Color32, FontId, PointerButton, Sense, Vec2};
use mathseea_model::{GraphError, Highlight, ImportGraph, Layout, NodeId, Selection};

use camera::Camera;
use input::{DocumentIdentity, validate_document, validate_frame};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ViewOptions {
    pub show_edges: bool,
    pub show_topic_labels: bool,
    pub show_node_labels: bool,
}

impl Default for ViewOptions {
    fn default() -> Self {
        Self {
            show_edges: true,
            show_topic_labels: true,
            show_node_labels: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ViewEvent {
    SelectNode(NodeId),
    SelectTopic(String),
    ClearSelection,
}

/// Retain one view across frames. Documents must remain immutable until reset_camera is called.
#[derive(Debug)]
pub struct GraphView {
    camera: Camera,
    document: Option<DocumentIdentity>,
    document_error: Option<GraphError>,
    fit_requested: bool,
    pending_focus: Option<NodeId>,
    gesture_suppressed: bool,
}

impl Default for GraphView {
    fn default() -> Self {
        Self {
            camera: Camera::default(),
            document: None,
            document_error: None,
            fit_requested: true,
            pending_focus: None,
            gesture_suppressed: false,
        }
    }
}

impl GraphView {
    /// Fit on the next usable frame and invalidate validation after document replacement/mutation.
    pub fn reset_camera(&mut self) {
        self.document = None;
        self.document_error = None;
        self.fit_requested = true;
        self.pending_focus = None;
        self.gesture_suppressed = false;
    }

    /// Center the node once on the next usable frame; unknown IDs produce a visible diagnostic.
    pub fn focus_node(&mut self, id: NodeId) {
        self.pending_focus = Some(id);
    }

    /// Paint a clipped canvas and return selection intents without mutating the input document.
    /// Invalid inputs are shown as GraphError diagnostics, since this fixed API returns events.
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
        painter.rect_filled(viewport, 0.0, Color32::BLACK);

        if !self
            .document
            .as_ref()
            .is_some_and(|identity| identity.matches(graph, layout))
        {
            self.document = Some(DocumentIdentity::new(graph, layout));
            self.document_error = validate_document(graph, layout).err();
            self.fit_requested = true;
        }
        let error = self
            .document_error
            .clone()
            .or_else(|| validate_frame(graph, layout, highlight, selection).err());
        if let Some(error) = error {
            paint_error(&painter, viewport, &error);
            return Vec::new();
        }
        if viewport.width() <= 1.0 || viewport.height() <= 1.0 {
            return Vec::new();
        }
        if self.fit_requested {
            self.camera.fit(layout.bounds, viewport);
            self.fit_requested = false;
        }
        if let Some(id) = self.pending_focus.take() {
            let Some(placement) = layout.nodes.get(id) else {
                paint_error(
                    &painter,
                    viewport,
                    &GraphError::new(format!("定位节点索引越界: {id}")),
                );
                return Vec::new();
            };
            self.camera.focus(*placement);
        }
        let (pressed, any_down) =
            ui.input(|input| (input.pointer.any_pressed(), input.pointer.any_down()));
        if pressed {
            self.gesture_suppressed = false;
        }
        if response.dragged_by(PointerButton::Primary) || response.dragged_by(PointerButton::Middle)
        {
            self.camera.pan(response.drag_delta());
            self.gesture_suppressed = true;
        }
        let mut zoomed = false;
        if response.hovered()
            && let Some(pointer) = ui.input(|input| input.pointer.hover_pos())
        {
            let factor = ui.input_mut(|input| {
                let factor = (input.smooth_scroll_delta.y * 0.004).clamp(-1.0, 1.0).exp()
                    * input.zoom_delta();
                input.smooth_scroll_delta = Vec2::ZERO;
                factor
            });
            if factor.is_finite() && factor > 0.0 && (factor - 1.0).abs() > f32::EPSILON {
                self.camera.zoom_at(factor, pointer, viewport);
                zoomed = true;
                if any_down || response.clicked() {
                    self.gesture_suppressed = true;
                }
            }
        }
        let click = if response.clicked()
            && !response.drag_stopped()
            && !self.gesture_suppressed
            && !zoomed
        {
            response
                .interact_pointer_pos()
                .filter(|pointer| painter.clip_rect().contains(*pointer))
        } else {
            None
        };
        let event = paint::Scene {
            graph,
            layout,
            highlight,
            selection,
            options,
            camera: &self.camera,
            viewport,
        }
        .paint(&painter, click);
        if !any_down {
            self.gesture_suppressed = false;
        }
        event.into_iter().collect()
    }
}

fn paint_error(painter: &egui::Painter, viewport: egui::Rect, error: &GraphError) {
    painter.text(
        viewport.left_top() + Vec2::splat(12.0),
        Align2::LEFT_TOP,
        format!("Graph view: {error}"),
        FontId::proportional(14.0),
        Color32::LIGHT_RED,
    );
}

#[cfg(test)]
mod tests {
    use std::hint::black_box;
    use std::time::Instant;

    use egui::epaint::{ClippedShape, Shape};
    use egui::{Context, Pos2, RawInput, Rect, Vec2};
    use mathseea_model::{Edge, ModuleNode, NodePlacement, Point};

    use super::*;

    fn frame(
        context: &Context,
        scene: &paint::Scene<'_>,
        click: Option<Pos2>,
        reference: bool,
    ) -> (Option<ViewEvent>, Vec<ClippedShape>) {
        let mut event = None;
        let mut output = context.run_ui(
            RawInput {
                screen_rect: Some(scene.viewport),
                ..Default::default()
            },
            |ui| {
                let painter = ui.painter_at(scene.viewport);
                event = if reference {
                    scene.paint_reference(&painter, click)
                } else {
                    scene.paint(&painter, click)
                };
            },
        );
        let shapes = std::mem::take(&mut output.shapes);
        output.drop_without_applying_deltas();
        (event, shapes)
    }

    fn compare(context: &Context, scene: &paint::Scene<'_>, click: Option<Pos2>) {
        for dpi in [1.0, 2.0] {
            context.set_pixels_per_point(dpi);
            frame(context, scene, None, true);
            frame(context, scene, None, false);
            let (old_event, old_shapes) = frame(context, scene, click, true);
            let (event, shapes) = frame(context, scene, click, false);
            assert_eq!(event, old_event);
            assert_eq!(shapes, old_shapes);
        }
    }

    // Replicate the bundled graph deterministically; setup is outside timed frames.
    fn fixture(text: &str, copies: usize) -> (ImportGraph, Layout) {
        let rows: Vec<_> = text.lines().collect();
        let count: usize = rows[0].parse().unwrap();
        let mut nodes = Vec::new();
        let mut placements = Vec::new();
        let mut edges = Vec::new();
        for copy in 0..copies {
            for row in &rows[1..=count] {
                let f: Vec<_> = row.split_whitespace().collect();
                let mut node = ModuleNode::new(f[0]);
                if copy > 0 {
                    node.name.push_str(&format!(".Copy{copy}"));
                }
                let rgb = u32::from_str_radix(&f[1][1..], 16).unwrap();
                node.color =
                    mathseea_model::Rgb::new((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8);
                nodes.push(node);
                placements.push(NodePlacement {
                    position: Point::new(
                        f[2].parse().unwrap(),
                        f[3].parse::<f32>().unwrap() + copy as f32 * 160.0,
                    ),
                    radius: f[4].parse().unwrap(),
                });
            }
            for row in &rows[count + 1..] {
                let f: Vec<usize> = row.split_whitespace().map(|v| v.parse().unwrap()).collect();
                edges.extend(f[2..].iter().map(|&id| Edge {
                    dependency: f[0] + copy * count,
                    dependent: id + copy * count,
                }));
            }
        }
        let graph = ImportGraph::new(nodes, edges).unwrap();
        let layout = Layout::from_nodes(&graph, placements).unwrap();
        (graph, layout)
    }

    const SMALL: &str = "3\nMathlib.A.LongVisibleModuleName #ff0000 -50 0 2\nMathlib.B.Second #00ff00 20 20 2\nMathlib.A.Third #ff0000 70 -20 2\n0 1 1\n1 1 2\n2 0\n";

    #[test]
    fn highlight_and_option_changes_preserve_shapes_and_events() {
        let (graph, mut layout) = fixture(SMALL, 1);
        layout.topics[0].position = layout.nodes[0].position;
        layout.topics.push(layout.topics[0].clone());
        let viewport = Rect::from_min_size(Pos2::ZERO, Vec2::new(320.0, 200.0));
        let mut camera = Camera::default();
        camera.fit(layout.bounds, viewport);
        let mut highlight = Highlight::none(&graph);
        let address = highlight.nodes.as_ptr();
        let context = Context::default();
        for (selection, active, mask) in [
            (Selection::None, false, 0),
            (Selection::None, true, 0),
            (Selection::None, true, 2),
            (Selection::Node(0), true, 1),
            (Selection::Topic("A".into()), true, 0),
            (Selection::None, true, 7),
        ] {
            highlight.active = active;
            for (id, on) in highlight.nodes.iter_mut().enumerate() {
                *on = mask & (1 << id) != 0;
            }
            assert_eq!(address, highlight.nodes.as_ptr());
            for flags in 0..8 {
                let options = ViewOptions {
                    show_edges: flags & 1 != 0,
                    show_topic_labels: flags & 2 != 0,
                    show_node_labels: flags & 4 != 0,
                };
                let scene = paint::Scene {
                    graph: &graph,
                    layout: &layout,
                    highlight: &highlight,
                    selection: &selection,
                    options: &options,
                    camera: &camera,
                    viewport,
                };
                compare(
                    &context,
                    &scene,
                    Some(camera.project(layout.nodes[0].position, viewport)),
                );
            }
        }
    }

    #[test]
    fn pan_zoom_long_labels_and_hit_priority_match_reference() {
        let (graph, mut layout) = fixture(SMALL, 1);
        layout.topics[0].position = layout.nodes[0].position;
        let viewport = Rect::from_min_size(Pos2::ZERO, Vec2::new(320.0, 200.0));
        let highlight = Highlight::all(&graph);
        let selection = Selection::Node(0);
        let options = ViewOptions::default();
        let context = Context::default();
        for (zoom, pan) in [
            (1.0, Vec2::ZERO),
            (8.0, Vec2::new(500.0, 80.0)),
            (16.0, Vec2::ZERO),
            (1.0, Vec2::new(-210.0, 0.0)),
        ] {
            let mut camera = Camera::default();
            camera.fit(layout.bounds, viewport);
            if pan.x < 0.0 {
                camera.focus(layout.nodes[0]);
            }
            camera.zoom_at(zoom, viewport.center(), viewport);
            camera.pan(pan);
            let scene = paint::Scene {
                graph: &graph,
                layout: &layout,
                highlight: &highlight,
                selection: &selection,
                options: &options,
                camera: &camera,
                viewport,
            };
            compare(&context, &scene, Some(viewport.center()));
            let (_, shapes) = frame(&context, &scene, Some(viewport.center()), false);
            if zoom == 1.0 && pan == Vec2::ZERO {
                let node = camera.project(layout.nodes[0].position, viewport);
                assert_eq!(
                    frame(&context, &scene, Some(node), false).0,
                    Some(ViewEvent::SelectNode(0))
                );
                let blank = viewport.right_bottom() - Vec2::splat(1.0);
                assert_eq!(
                    frame(&context, &scene, Some(blank), false).0,
                    Some(ViewEvent::ClearSelection)
                );
            } else if zoom == 16.0 {
                assert!(
                    shapes
                        .iter()
                        .any(|s| matches!(&s.shape, Shape::CubicBezier(b)
                    if !viewport.contains(b.points[0]) && !viewport.contains(b.points[3])))
                );
            } else if pan.x < 0.0 {
                assert!(shapes.iter().any(|s| matches!(&s.shape, Shape::Text(t)
                    if t.galley.job.text == graph.nodes[0].name && t.pos.x < 0.0
                    && t.pos.x + t.galley.size().x > viewport.left())));
            }
        }
    }

    #[test]
    #[ignore = "manual CPU benchmark; committed clean worktree required"]
    fn cpu_frame_benchmark() {
        let case =
            std::env::var("MATHSEEA_BENCH_CASE").unwrap_or_else(|_| "default-highlight".into());
        let (copies, zoom, active) = match case.as_str() {
            "default-overview" => (1, 1.0, false),
            "default-highlight" => (1, 8.0, true),
            "large-highlight" => (10, 8.0, true),
            _ => panic!("unknown case: {case}"),
        };
        let (graph, layout) = fixture(include_str!("../../../data/import_graph.txt"), copies);
        let mut highlight = Highlight::all(&graph);
        highlight.active = active;
        if active {
            highlight.nodes.fill(false);
            highlight.nodes[graph.nodes.len() / 2] = true;
        }
        let selection = Selection::None;
        let options = ViewOptions::default();
        let viewport = Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 720.0));
        let contexts = [Context::default(), Context::default()];
        let mut samples = [Vec::new(), Vec::new()];
        for round in 0..3 {
            for order in 0..2 {
                let path = (round + order) & 1;
                let mut camera = Camera::default();
                camera.fit(layout.bounds, viewport);
                camera.zoom_at(zoom, viewport.center(), viewport);
                for index in 0..230 {
                    camera.pan(Vec2::new(if index & 1 == 0 { 20.0 } else { -20.0 }, 0.0));
                    let scene = paint::Scene {
                        graph: &graph,
                        layout: &layout,
                        highlight: &highlight,
                        selection: &selection,
                        options: &options,
                        camera: &camera,
                        viewport,
                    };
                    let start = Instant::now();
                    let output = frame(&contexts[path], &scene, None, path == 0);
                    let elapsed = start.elapsed().as_nanos();
                    if index >= 30 {
                        samples[path].push(elapsed);
                    }
                    black_box(output);
                }
            }
        }
        for times in &mut samples {
            times.sort_unstable();
        }
        println!(
            "case={case} V={} E={} T={} viewport=1280x720 dpi=1 fonts=egui-default samples=600 reference_median_ns={} optimized_median_ns={} reference_p95_ns={} optimized_p95_ns={}",
            graph.nodes.len(),
            graph.edges.len(),
            layout.topics.len(),
            samples[0][299],
            samples[1][299],
            samples[0][569],
            samples[1][569]
        );
    }
}
