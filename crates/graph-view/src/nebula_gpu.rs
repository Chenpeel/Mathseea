use std::borrow::Cow;
use std::sync::{Arc, Mutex};

use eframe::{egui_wgpu, wgpu};
use wgpu::util::DeviceExt;

pub(super) struct GpuScene {
    pub nodes: Vec<u8>,
    pub lines: Vec<u8>,
    pub camera: Vec<u8>,
    pub size: [u32; 2],
}

pub(super) struct NebulaCallback {
    pub resources: Arc<Mutex<Option<Resources>>>,
    pub scene: Arc<GpuScene>,
    pub target: wgpu::TextureFormat,
}

pub(super) struct Resources {
    sphere: wgpu::RenderPipeline,
    lines: wgpu::RenderPipeline,
    composite: wgpu::RenderPipeline,
    uniform: wgpu::Buffer,
    camera_group: wgpu::BindGroup,
    line_camera_group: wgpu::BindGroup,
    color: wgpu::TextureView,
    depth: wgpu::TextureView,
    texture_group: wgpu::BindGroup,
    sampler: wgpu::Sampler,
    size: [u32; 2],
}

fn texture(
    device: &wgpu::Device,
    size: [u32; 2],
    format: wgpu::TextureFormat,
) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("mathseea nebula attachment"),
            size: wgpu::Extent3d {
                width: size[0],
                height: size[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
        .create_view(&Default::default())
}

fn pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    entries: (&str, &str),
    format: wgpu::TextureFormat,
    topology: wgpu::PrimitiveTopology,
    depth: bool,
    buffers: &[Option<wgpu::VertexBufferLayout<'_>>],
) -> wgpu::RenderPipeline {
    let (vertex, fragment) = entries;
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(vertex),
        layout: None,
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some(vertex),
            buffers,
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(fragment),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology,
            ..Default::default()
        },
        depth_stencil: depth.then_some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::LessEqual),
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    })
}

impl Resources {
    fn new(device: &wgpu::Device, target: wgpu::TextureFormat, size: [u32; 2]) -> Self {
        let (scene_source, composite_source) = include_str!("nebula.wgsl")
            .split_once("// COMPOSITE")
            .expect("built-in shader sections");
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("nebula 3d"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(scene_source)),
        });
        let composite_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("nebula composite"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(composite_source)),
        });
        let attributes = wgpu::vertex_attr_array![0=>Float32x4,1=>Float32x4];
        let vertex_layout = |step_mode| {
            Some(wgpu::VertexBufferLayout {
                array_stride: 32,
                step_mode,
                attributes: &attributes,
            })
        };
        let format = wgpu::TextureFormat::Rgba8Unorm;
        let sphere = pipeline(
            device,
            &shader,
            ("sphere_vertex", "sphere_fragment"),
            format,
            wgpu::PrimitiveTopology::TriangleList,
            true,
            &[vertex_layout(wgpu::VertexStepMode::Instance)],
        );
        let lines = pipeline(
            device,
            &shader,
            ("line_vertex", "line_fragment"),
            format,
            wgpu::PrimitiveTopology::LineList,
            true,
            &[vertex_layout(wgpu::VertexStepMode::Vertex)],
        );
        let composite = pipeline(
            device,
            &composite_shader,
            ("composite_vertex", "composite_fragment"),
            target,
            wgpu::PrimitiveTopology::TriangleList,
            false,
            &[],
        );
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("nebula camera"),
            size: 64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let camera_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("nebula camera"),
            layout: &sphere.get_bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            }],
        });
        let line_camera_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("nebula line camera"),
            layout: &lines.get_bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            }],
        });
        let color = texture(device, size, format);
        let depth = texture(device, size, wgpu::TextureFormat::Depth32Float);
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("nebula sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let texture_group = Self::texture_group(device, &composite, &color, &sampler);
        Self {
            sphere,
            lines,
            composite,
            uniform,
            camera_group,
            line_camera_group,
            color,
            depth,
            texture_group,
            sampler,
            size,
        }
    }
    fn texture_group(
        device: &wgpu::Device,
        pipeline: &wgpu::RenderPipeline,
        color: &wgpu::TextureView,
        sampler: &wgpu::Sampler,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("nebula composite"),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(color),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        })
    }
    fn resize(&mut self, device: &wgpu::Device, size: [u32; 2]) {
        if self.size == size {
            return;
        }
        self.color = texture(device, size, wgpu::TextureFormat::Rgba8Unorm);
        self.depth = texture(device, size, wgpu::TextureFormat::Depth32Float);
        self.texture_group =
            Self::texture_group(device, &self.composite, &self.color, &self.sampler);
        self.size = size;
    }
}

impl egui_wgpu::CallbackTrait for NebulaCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen: &egui_wgpu::ScreenDescriptor,
        encoder: &mut wgpu::CommandEncoder,
        _resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let Ok(mut guard) = self.resources.lock() else {
            return Vec::new();
        };
        let gpu = guard.get_or_insert_with(|| Resources::new(device, self.target, self.scene.size));
        gpu.resize(device, self.scene.size);
        queue.write_buffer(&gpu.uniform, 0, &self.scene.camera);
        let buffer = |bytes: &[u8]| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("nebula geometry"),
                contents: if bytes.is_empty() { &[0; 32] } else { bytes },
                usage: wgpu::BufferUsages::VERTEX,
            })
        };
        let nodes = buffer(&self.scene.nodes);
        let lines = buffer(&self.scene.lines);
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("nebula offscreen 3d depth pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &gpu.color,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.025,
                        g: 0.035,
                        b: 0.06,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &gpu.depth,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            ..Default::default()
        });
        pass.set_pipeline(&gpu.sphere);
        pass.set_bind_group(0, &gpu.camera_group, &[]);
        pass.set_vertex_buffer(0, nodes.slice(..));
        pass.draw(0..6, 0..(self.scene.nodes.len() / 32) as u32);
        pass.set_pipeline(&gpu.lines);
        pass.set_bind_group(0, &gpu.line_camera_group, &[]);
        pass.set_vertex_buffer(0, lines.slice(..));
        pass.draw(0..(self.scene.lines.len() / 32) as u32, 0..1);
        Vec::new()
    }
    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        pass: &mut wgpu::RenderPass<'static>,
        _resources: &egui_wgpu::CallbackResources,
    ) {
        if let Ok(guard) = self.resources.lock()
            && let Some(gpu) = guard.as_ref()
        {
            pass.set_pipeline(&gpu.composite);
            pass.set_bind_group(0, &gpu.texture_group, &[]);
            pass.draw(0..3, 0..1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn embedded_shader_sections_parse_and_validate_without_a_gpu() {
        let (scene, composite) = include_str!("nebula.wgsl")
            .split_once("// COMPOSITE")
            .unwrap();
        for source in [scene, composite] {
            let module = wgpu::naga::front::wgsl::parse_str(source).unwrap();
            wgpu::naga::valid::Validator::new(
                wgpu::naga::valid::ValidationFlags::all(),
                wgpu::naga::valid::Capabilities::all(),
            )
            .validate(&module)
            .unwrap();
        }
    }
}
