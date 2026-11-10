struct Camera {
    eye_focal: vec4<f32>,
    right_aspect: vec4<f32>,
    up_near: vec4<f32>,
    forward_far: vec4<f32>,
}
@group(0) @binding(0) var<uniform> camera: Camera;
fn project(p: vec3<f32>) -> vec4<f32> {
    let r = p - camera.eye_focal.xyz;
    let z = dot(r, camera.forward_far.xyz);
    let near = camera.up_near.w;
    let far = camera.forward_far.w;
    return vec4(dot(r,camera.right_aspect.xyz)*camera.eye_focal.w/camera.right_aspect.w,
        dot(r,camera.up_near.xyz)*camera.eye_focal.w, far/(far-near)*z-far*near/(far-near), z);
}
struct Vertex {
    @location(0) center_radius: vec4<f32>,
    @location(1) color: vec4<f32>,
}
struct Sphere {
    @builtin(position) position: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) center_radius: vec4<f32>,
    @location(2) color: vec4<f32>,
}
@vertex fn sphere_vertex(v: Vertex, @builtin(vertex_index) index: u32) -> Sphere {
    let corners = array<vec2<f32>,6>(vec2(-1.,-1.),vec2(1.,-1.),vec2(-1.,1.),vec2(-1.,1.),vec2(1.,-1.),vec2(1.,1.));
    let relative = v.center_radius.xyz-camera.eye_focal.xyz;
    let depth = dot(relative,camera.forward_far.xyz);
    let cover = length(relative)/max(depth-v.center_radius.w,camera.up_near.w);
    let corner = corners[index] * v.center_radius.w * max(1.15,cover);
    let world = v.center_radius.xyz + camera.right_aspect.xyz*corner.x + camera.up_near.xyz*corner.y;
    var clip = project(world);
    // Fragment intersection, not the billboard plane, defines sphere depth clipping.
    clip.z = clamp(clip.z,0.,max(clip.w,0.));
    return Sphere(clip,world,v.center_radius,v.color);
}
struct SphereFragment { @location(0) color: vec4<f32>, @builtin(frag_depth) depth: f32, }
@fragment fn sphere_fragment(v: Sphere) -> SphereFragment {
    let ray = normalize(v.world-camera.eye_focal.xyz);
    let offset = camera.eye_focal.xyz-v.center_radius.xyz;
    let b = dot(offset,ray);
    let d = b*b-dot(offset,offset)+v.center_radius.w*v.center_radius.w;
    if d < 0. { discard; }
    let distance = -b-sqrt(d);
    let world = camera.eye_focal.xyz+ray*distance;
    let clip = project(world);
    if distance <= 0. || clip.w <= camera.up_near.w || clip.w >= camera.forward_far.w { discard; }
    let normal = normalize(world-v.center_radius.xyz);
    let light = 0.48+0.52*max(dot(normal,normalize(vec3(-0.3,0.7,0.8))),0.);
    let rim = pow(1.-max(dot(normal,-ray),0.),3.);
    return SphereFragment(vec4(v.color.rgb*(light+rim*0.5),1.),clip.z/clip.w);
}
struct Line { @builtin(position) position: vec4<f32>, @location(0) color: vec4<f32>, }
@vertex fn line_vertex(v: Vertex) -> Line { return Line(project(v.center_radius.xyz),v.color); }
@fragment fn line_fragment(v: Line) -> @location(0) vec4<f32> { return v.color; }

// COMPOSITE
@group(0) @binding(0) var image: texture_2d<f32>;
@group(0) @binding(1) var image_sampler: sampler;
struct Composite { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32>, }
@vertex fn composite_vertex(@builtin(vertex_index) index: u32) -> Composite {
    let p = array<vec2<f32>,3>(vec2(-1.,-1.),vec2(3.,-1.),vec2(-1.,3.))[index];
    return Composite(vec4(p,0.,1.),vec2(p.x*0.5+0.5,0.5-p.y*0.5));
}
@fragment fn composite_fragment(v: Composite) -> @location(0) vec4<f32> {
    return textureSample(image,image_sampler,v.uv);
}
