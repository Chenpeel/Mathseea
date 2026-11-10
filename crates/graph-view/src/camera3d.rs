use egui::{Pos2, Rect, Vec2};

pub(super) type Vec3 = [f32; 3];

pub(super) fn add(a: Vec3, b: Vec3) -> Vec3 {
    std::array::from_fn(|i| a[i] + b[i])
}
pub(super) fn sub(a: Vec3, b: Vec3) -> Vec3 {
    std::array::from_fn(|i| a[i] - b[i])
}
pub(super) fn mul(a: Vec3, s: f32) -> Vec3 {
    a.map(|v| v * s)
}
pub(super) fn dot(a: Vec3, b: Vec3) -> f32 {
    a.into_iter().zip(b).map(|(x, y)| x * y).sum()
}
fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn unit(a: Vec3) -> Vec3 {
    mul(a, dot(a, a).sqrt().max(1e-8).recip())
}

/// Orbit camera using a positive forward depth shared by CPU picking and WGSL.
#[derive(Clone, Debug)]
pub(super) struct OrbitCamera {
    pub target: Vec3,
    pub distance: f32,
    yaw: f32,
    pitch: f32,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        Self {
            target: [0.0; 3],
            distance: 4.4,
            yaw: 0.22,
            pitch: 0.12,
        }
    }
}

impl OrbitCamera {
    pub fn basis(&self) -> (Vec3, Vec3, Vec3, Vec3) {
        let outward = [
            self.yaw.sin() * self.pitch.cos(),
            self.pitch.sin(),
            self.yaw.cos() * self.pitch.cos(),
        ];
        let eye = add(self.target, mul(outward, self.distance));
        let forward = mul(outward, -1.0);
        let right = unit(cross(forward, [0.0, 1.0, 0.0]));
        let up = cross(right, forward);
        (eye, right, up, forward)
    }

    pub fn orbit(&mut self, delta: Vec2) {
        self.yaw -= delta.x * 0.006;
        self.pitch = (self.pitch + delta.y * 0.006).clamp(-1.45, 1.45);
    }

    pub fn pan(&mut self, delta: Vec2, viewport: Rect) {
        let (_, right, up, _) = self.basis();
        let scale = self.distance / (viewport.height().max(1.0) * self.focal());
        self.target = add(
            self.target,
            add(
                mul(right, -delta.x * 2.0 * scale),
                mul(up, delta.y * 2.0 * scale),
            ),
        );
    }

    pub fn zoom(&mut self, factor: f32) {
        if factor.is_finite() && factor > 0.0 {
            self.distance = (self.distance / factor).clamp(0.25, 25.0);
        }
    }

    pub fn focal(&self) -> f32 {
        1.0 / (25.0_f32.to_radians()).tan()
    }
    pub fn near(&self) -> f32 {
        0.02
    }
    pub fn far(&self) -> f32 {
        80.0
    }

    pub fn project(&self, point: Vec3, viewport: Rect) -> Option<(Pos2, f32)> {
        let (eye, right, up, forward) = self.basis();
        let relative = sub(point, eye);
        let z = dot(relative, forward);
        if z <= self.near() || z >= self.far() {
            return None;
        }
        let x = dot(relative, right) * self.focal() / (z * viewport.aspect_ratio());
        let y = dot(relative, up) * self.focal() / z;
        Some((
            viewport.center() + Vec2::new(x * viewport.width() * 0.5, -y * viewport.height() * 0.5),
            z,
        ))
    }

    pub fn ray(&self, pointer: Pos2, viewport: Rect) -> (Vec3, Vec3) {
        let (eye, right, up, forward) = self.basis();
        let ndc = (pointer - viewport.center()) * 2.0 / viewport.size();
        let direction = add(
            forward,
            add(
                mul(right, ndc.x * viewport.aspect_ratio() / self.focal()),
                mul(up, -ndc.y / self.focal()),
            ),
        );
        (eye, unit(direction))
    }

    pub fn intersect(
        &self,
        pointer: Pos2,
        viewport: Rect,
        center: Vec3,
        radius: f32,
    ) -> Option<f32> {
        let (eye, ray) = self.ray(pointer, viewport);
        let offset = sub(eye, center);
        let b = dot(offset, ray);
        let c = dot(offset, offset) - radius * radius;
        let d = b * b - c;
        if d < 0.0 {
            return None;
        }
        let distance = -b - d.sqrt();
        let hit = add(eye, mul(ray, distance));
        let forward_depth = dot(sub(hit, eye), self.basis().3);
        (distance > 0.0 && forward_depth > self.near() && forward_depth < self.far())
            .then_some(distance)
    }

    pub fn uniform(&self, aspect: f32) -> Vec<u8> {
        let (eye, right, up, forward) = self.basis();
        [
            [eye[0], eye[1], eye[2], self.focal()],
            [right[0], right[1], right[2], aspect],
            [up[0], up[1], up[2], self.near()],
            [forward[0], forward[1], forward[2], self.far()],
        ]
        .into_iter()
        .flatten()
        .flat_map(f32::to_ne_bytes)
        .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn projection_ray_and_nearest_sphere_agree() {
        let camera = OrbitCamera::default();
        let viewport = Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 600.0));
        let center = camera.project([0.0; 3], viewport).unwrap().0;
        assert!((center - viewport.center()).length() < 0.001);
        let (eye, ray) = camera.ray(center, viewport);
        assert!(dot(unit(sub([0.0; 3], eye)), ray) > 0.99999);
        let near = camera.intersect(center, viewport, [0.0; 3], 0.1).unwrap();
        let far = camera
            .intersect(center, viewport, add([0.0; 3], mul(ray, 0.5)), 0.1)
            .unwrap();
        assert!(near < far);
    }
    #[test]
    fn camera_controls_are_finite_and_deterministic() {
        let mut a = OrbitCamera::default();
        let mut b = a.clone();
        let viewport = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0));
        for camera in [&mut a, &mut b] {
            camera.orbit(Vec2::new(100.0, 10000.0));
            camera.pan(Vec2::new(20.0, -10.0), viewport);
            camera.zoom(10000.0);
        }
        assert_eq!(a.basis(), b.basis());
        assert_eq!(a.distance, 0.25);
        assert_eq!(a.uniform(1.5).len(), 64);
        assert!(a.basis().0.into_iter().all(f32::is_finite));
    }
    #[test]
    fn off_axis_sphere_hits_clip_by_forward_depth_not_ray_distance() {
        let camera = OrbitCamera::default();
        let viewport = Rect::from_min_size(Pos2::ZERO, Vec2::new(1200.0, 600.0));
        let pointer = viewport.right_top() + Vec2::new(-1.0, 1.0);
        let (eye, ray) = camera.ray(pointer, viewport);
        let cosine = dot(ray, camera.basis().3);
        assert!(cosine < 0.9);
        for (depth, expected) in [
            (camera.near() * 0.9, false),
            (camera.near() * 1.1, true),
            (camera.far() * 0.99, true),
            (camera.far() * 1.01, false),
        ] {
            // Distant spheres use a larger radius to avoid float cancellation.
            let radius = if depth > 1.0 { 0.1 } else { 0.001 };
            let distance = depth / cosine;
            let center = add(eye, mul(ray, distance + radius));
            assert_eq!(
                camera
                    .intersect(pointer, viewport, center, radius)
                    .is_some(),
                expected
            );
        }
    }
}
