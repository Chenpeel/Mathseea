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
