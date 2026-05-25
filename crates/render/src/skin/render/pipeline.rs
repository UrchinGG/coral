use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use wgpu::util::DeviceExt;

use super::RenderContext;
use crate::skin::{
    Skin,
    model::{BodyPart, CubeFaceUvs, FaceUv, Pose},
};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub uv: [f32; 2],
    pub normal: [f32; 3],
}

impl Vertex {
    const ATTRIBS: [wgpu::VertexAttribute; 3] =
        wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2, 2 => Float32x3];

    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct Uniforms {
    pub mvp: [[f32; 4]; 4],
    pub light_direction: [f32; 3],
    pub ambient: f32,
}

pub struct RenderPipeline {
    pub pipeline: wgpu::RenderPipeline,
    pub bind_group_layout: wgpu::BindGroupLayout,
}

impl RenderPipeline {
    pub fn new(ctx: &RenderContext) -> Self {
        let shader = ctx
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("skin_shader"),
                source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
            });

        let bind_group_layout =
            ctx.device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("skin_bind_group_layout"),
                    entries: &[
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Uniform,
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Texture {
                                multisampled: false,
                                view_dimension: wgpu::TextureViewDimension::D2,
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 2,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                            count: None,
                        },
                    ],
                });

        let pipeline_layout = ctx
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("skin_pipeline_layout"),
                bind_group_layouts: &[&bind_group_layout],
                push_constant_ranges: &[],
            });

        let pipeline = ctx
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("skin_pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    buffers: &[Vertex::layout()],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::Rgba8UnormSrgb,
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    cull_mode: None,
                    front_face: wgpu::FrontFace::Ccw,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth32Float,
                    depth_write_enabled: true,
                    depth_compare: wgpu::CompareFunction::Less,
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            });

        Self {
            pipeline,
            bind_group_layout,
        }
    }
}

pub struct Mesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

impl Mesh {
    pub fn new() -> Self {
        Self {
            vertices: Vec::new(),
            indices: Vec::new(),
        }
    }

    pub fn add_cube(
        &mut self,
        position: Vec3,
        size: Vec3,
        uvs: &CubeFaceUvs,
        transform: Mat4,
        expansion: f32,
    ) {
        let min = position - Vec3::splat(expansion);
        let max = position + size + Vec3::splat(expansion);
        let (x0, x1) = (min.x, max.x);
        let (y0, y1) = (min.y, max.y);
        let (z0, z1) = (min.z, max.z);

        self.add_face(
            transform,
            [
                Vec3::new(x0, y1, z0),
                Vec3::new(x1, y1, z0),
                Vec3::new(x0, y0, z0),
                Vec3::new(x1, y0, z0),
            ],
            &uvs.front,
        );
        self.add_face(
            transform,
            [
                Vec3::new(x1, y1, z1),
                Vec3::new(x0, y1, z1),
                Vec3::new(x1, y0, z1),
                Vec3::new(x0, y0, z1),
            ],
            &uvs.back,
        );
        self.add_face(
            transform,
            [
                Vec3::new(x1, y1, z0),
                Vec3::new(x1, y1, z1),
                Vec3::new(x1, y0, z0),
                Vec3::new(x1, y0, z1),
            ],
            &uvs.left,
        );
        self.add_face(
            transform,
            [
                Vec3::new(x0, y1, z1),
                Vec3::new(x0, y1, z0),
                Vec3::new(x0, y0, z1),
                Vec3::new(x0, y0, z0),
            ],
            &uvs.right,
        );
        self.add_face(
            transform,
            [
                Vec3::new(x1, y1, z1),
                Vec3::new(x0, y1, z1),
                Vec3::new(x1, y1, z0),
                Vec3::new(x0, y1, z0),
            ],
            &uvs.top,
        );
        self.add_face(
            transform,
            [
                Vec3::new(x0, y0, z1),
                Vec3::new(x1, y0, z1),
                Vec3::new(x0, y0, z0),
                Vec3::new(x1, y0, z0),
            ],
            &uvs.bottom,
        );
    }

    fn add_face(&mut self, transform: Mat4, corners: [Vec3; 4], uv: &FaceUv) {
        let base = self.vertices.len() as u32;
        let [p0, p1, p2, p3] = corners.map(|c| transform.transform_point3(c));
        let normal = (p2 - p0).cross(p1 - p0).normalize().to_array();

        self.vertices.extend([
            Vertex {
                position: p0.to_array(),
                uv: uv.top_left.to_array(),
                normal,
            },
            Vertex {
                position: p1.to_array(),
                uv: uv.top_right.to_array(),
                normal,
            },
            Vertex {
                position: p2.to_array(),
                uv: uv.bottom_left.to_array(),
                normal,
            },
            Vertex {
                position: p3.to_array(),
                uv: uv.bottom_right.to_array(),
                normal,
            },
        ]);
        self.indices
            .extend_from_slice(&[base, base + 2, base + 1, base + 1, base + 2, base + 3]);
    }

    pub fn build_from_skin(skin: &Skin, pose: &Pose, parts: &[BodyPart]) -> Self {
        let mut mesh = Self::new();
        let slim = skin.is_slim();

        for &part in parts {
            let rotation = match part {
                BodyPart::Head | BodyPart::HeadLayer => pose.head,
                BodyPart::Body | BodyPart::BodyLayer => pose.body,
                BodyPart::LeftArm | BodyPart::LeftArmLayer => pose.left_arm,
                BodyPart::RightArm | BodyPart::RightArmLayer => pose.right_arm,
                BodyPart::LeftLeg | BodyPart::LeftLegLayer => pose.left_leg,
                BodyPart::RightLeg | BodyPart::RightLegLayer => pose.right_leg,
            };
            let pivot = part.pivot(slim);
            let radians = rotation.to_radians();
            let transform = Mat4::from_translation(pivot)
                * Mat4::from_rotation_x(radians.x)
                * Mat4::from_rotation_y(radians.y)
                * Mat4::from_rotation_z(radians.z)
                * Mat4::from_translation(-pivot);
            mesh.add_cube(
                part.position(slim),
                part.size(slim),
                &part.uvs(slim),
                transform,
                part.layer_expansion(),
            );
        }
        mesh
    }

    pub fn create_buffers(&self, ctx: &RenderContext) -> (wgpu::Buffer, wgpu::Buffer, u32) {
        let vb = ctx
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("vertex_buffer"),
                contents: bytemuck::cast_slice(&self.vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
        let ib = ctx
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("index_buffer"),
                contents: bytemuck::cast_slice(&self.indices),
                usage: wgpu::BufferUsages::INDEX,
            });
        (vb, ib, self.indices.len() as u32)
    }
}
