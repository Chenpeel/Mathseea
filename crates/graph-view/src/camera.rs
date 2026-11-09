use egui::{Pos2, Rect, Vec2};
use mathseea_model::{Bounds, NodePlacement, Point};

/// f64 intermediates keep fitting finite f32 world coordinates independent of their units.
#[derive(Debug)]
pub(crate) struct Camera {
    center: [f64; 2],
    scale: f64,
    fit_scale: f64,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            center: [0.0; 2],
            scale: 1.0,
            fit_scale: 1.0,
        }
    }
}

impl Camera {
    pub(crate) fn fit(&mut self, bounds: Bounds, viewport: Rect) {
        let min = [f64::from(bounds.min.x), f64::from(bounds.min.y)];
        let max = [f64::from(bounds.max.x), f64::from(bounds.max.y)];
        self.center = [(min[0] + max[0]) * 0.5, (min[1] + max[1]) * 0.5];
        let minimum_extent = f64::from(f32::from_bits(1));
        let width = (max[0] - min[0]).max(minimum_extent);
        let height = (max[1] - min[1]).max(minimum_extent);
        self.fit_scale = (f64::from((viewport.width() - 64.0).max(1.0)) / width)
            .min(f64::from((viewport.height() - 64.0).max(1.0)) / height);
        self.scale = self.fit_scale;
    }

    pub(crate) fn project(&self, point: Point, viewport: Rect) -> Pos2 {
        let center = viewport.center();
        // Bound tessellation coordinates for extreme but valid world extents.
        let x = ((f64::from(point.x) - self.center[0]) * self.scale).clamp(-1.0e6, 1.0e6);
        let y = ((f64::from(point.y) - self.center[1]) * self.scale).clamp(-1.0e6, 1.0e6);
        center + Vec2::new(x as f32, y as f32)
    }

    pub(crate) fn radius(&self, radius: f32) -> f32 {
        (f64::from(radius) * self.scale).clamp(1.5, 1.0e6) as f32
    }

    pub(crate) fn relative_zoom(&self) -> f32 {
        (self.scale / self.fit_scale) as f32
    }

    pub(crate) fn pan(&mut self, delta: Vec2) {
        self.center[0] -= f64::from(delta.x) / self.scale;
        self.center[1] -= f64::from(delta.y) / self.scale;
    }

    pub(crate) fn zoom_at(&mut self, factor: f32, pointer: Pos2, viewport: Rect) {
        let offset = pointer - viewport.center();
        let old_scale = self.scale;
        self.scale =
            (self.scale * f64::from(factor)).clamp(self.fit_scale / 64.0, self.fit_scale * 4096.0);
        self.center[0] += f64::from(offset.x) * (1.0 / old_scale - 1.0 / self.scale);
        self.center[1] += f64::from(offset.y) * (1.0 / old_scale - 1.0 / self.scale);
    }

    pub(crate) fn focus(&mut self, placement: NodePlacement) {
        self.center = [
            f64::from(placement.position.x),
            f64::from(placement.position.y),
        ];
        self.scale = self
            .scale
            .max(16.0 / f64::from(placement.radius))
            .min(self.fit_scale * 4096.0);
    }
}
