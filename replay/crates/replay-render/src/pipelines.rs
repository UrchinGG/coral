use bytemuck::Pod;
use replay_world::Vertex;

use crate::{
    DEPTH_FORMAT,
    scene::{Billboard, Cuboid, MeshVertex},
};

const ALPHA: Option<wgpu::BlendState> = Some(wgpu::BlendState::ALPHA_BLENDING);
const ADD: wgpu::BlendComponent = wgpu::BlendComponent {
    src_factor: wgpu::BlendFactor::One,
    dst_factor: wgpu::BlendFactor::One,
    operation: wgpu::BlendOperation::Add,
};
const ADDITIVE: Option<wgpu::BlendState> = Some(wgpu::BlendState {
    color: ADD,
    alpha: ADD,
});
const MULTIPLY: Option<wgpu::BlendState> = Some(wgpu::BlendState {
    color: wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::Dst,
        dst_factor: wgpu::BlendFactor::Src,
        operation: wgpu::BlendOperation::Add,
    },
    alpha: wgpu::BlendComponent::REPLACE,
});

#[derive(Clone, Copy)]
enum Depth {
    Write,
    Test,
    Equal,
    Ignore,
}

pub(crate) struct Batch {
    pipeline: wgpu::RenderPipeline,
    buffer: wgpu::Buffer,
    pub(crate) count: u32,
}

impl Batch {
    fn new(device: &wgpu::Device, pipeline: wgpu::RenderPipeline) -> Batch {
        Batch {
            pipeline,
            buffer: vertex_buffer(device, 4),
            count: 0,
        }
    }

    pub(crate) fn upload<T: Pod>(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        items: &[T],
    ) {
        let bytes: &[u8] = bytemuck::cast_slice(items);
        if bytes.len() as u64 > self.buffer.size() {
            self.buffer = vertex_buffer(device, bytes.len().next_power_of_two() as u64);
        }
        queue.write_buffer(&self.buffer, 0, bytes);
        self.count = items.len() as u32;
    }

    pub(crate) fn draw_instances(&self, pass: &mut wgpu::RenderPass, vertices: u32) {
        if self.count > 0 {
            pass.set_pipeline(&self.pipeline);
            pass.set_vertex_buffer(0, self.buffer.slice(..));
            pass.draw(0..vertices, 0..self.count);
        }
    }

    pub(crate) fn draw_quads(&self, pass: &mut wgpu::RenderPass) {
        if self.count > 0 {
            pass.set_pipeline(&self.pipeline);
            pass.set_vertex_buffer(0, self.buffer.slice(..));
            pass.draw_indexed(0..self.count / 4 * 6, 0, 0..1);
        }
    }
}

struct PipelineBuilder<'a> {
    device: &'a wgpu::Device,
    shader: &'a wgpu::ShaderModule,
    target_format: wgpu::TextureFormat,
    samples: u32,
}

impl PipelineBuilder<'_> {
    fn build(
        &self,
        layout: &wgpu::PipelineLayout,
        [vertex, fragment]: [&str; 2],
        buffers: &[Option<wgpu::VertexBufferLayout>],
        blend: Option<wgpu::BlendState>,
        depth: Depth,
        cull_mode: Option<wgpu::Face>,
    ) -> wgpu::RenderPipeline {
        let cutout = matches!(depth, Depth::Write);
        self.device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(fragment),
                layout: Some(layout),
                vertex: wgpu::VertexState {
                    module: self.shader,
                    entry_point: Some(vertex),
                    compilation_options: Default::default(),
                    buffers,
                },
                primitive: wgpu::PrimitiveState {
                    cull_mode,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(matches!(depth, Depth::Write)),
                    depth_compare: Some(match depth {
                        Depth::Ignore => wgpu::CompareFunction::Always,
                        Depth::Equal => wgpu::CompareFunction::Equal,
                        Depth::Write | Depth::Test => wgpu::CompareFunction::GreaterEqual,
                    }),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: wgpu::MultisampleState {
                    count: self.samples,
                    mask: !0,
                    alpha_to_coverage_enabled: cutout && self.samples > 1,
                },
                fragment: Some(wgpu::FragmentState {
                    module: self.shader,
                    entry_point: Some(fragment),
                    compilation_options: wgpu::PipelineCompilationOptions {
                        constants: &[("ALPHA_TO_COVERAGE", f64::from(u8::from(self.samples > 1)))],
                        ..Default::default()
                    },
                    targets: &[Some(wgpu::ColorTargetState {
                        format: self.target_format,
                        blend,
                        write_mask: if cutout {
                            wgpu::ColorWrites::COLOR
                        } else {
                            wgpu::ColorWrites::ALL
                        },
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
    }
}

pub(crate) struct Pipelines {
    pub(crate) opaque: wgpu::RenderPipeline,
    pub(crate) translucent: wgpu::RenderPipeline,
    pub(crate) cuboids: Batch,
    pub(crate) glint_cuboids: Batch,
    pub(crate) solid: Batch,
    pub(crate) glint_meshes: Batch,
    pub(crate) cracks: Batch,
    pub(crate) xray: Batch,
    pub(crate) blended: Batch,
    pub(crate) particles: Batch,
}

impl Pipelines {
    pub(crate) fn new(
        device: &wgpu::Device,
        shader: &wgpu::ShaderModule,
        [globals_only, terrain_layout]: [&wgpu::PipelineLayout; 2],
        target_format: wgpu::TextureFormat,
        samples: u32,
    ) -> Pipelines {
        let builder = PipelineBuilder {
            device,
            shader,
            target_format,
            samples,
        };
        let terrain_vertices = [Some(wgpu::VertexBufferLayout {
            array_stride: size_of::<Vertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2, 2 => Uint32, 3 => Unorm8x4, 4 => Unorm8x4],
        })];
        let cuboid_instances = [Some(wgpu::VertexBufferLayout {
            array_stride: size_of::<Cuboid>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &wgpu::vertex_attr_array![
                0 => Float32x4, 1 => Float32x4, 2 => Float32x4, 3 => Float32x4,
                4 => Float32x3, 5 => Float32, 6 => Float32x3, 7 => Sint32,
                8 => Float32x2, 9 => Uint32, 10 => Unorm8x4, 11 => Unorm8x4,
            ],
        })];
        let mesh_vertices = [Some(wgpu::VertexBufferLayout {
            array_stride: size_of::<MeshVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2, 2 => Sint32, 3 => Unorm8x4],
        })];
        let billboard_instances = [Some(wgpu::VertexBufferLayout {
            array_stride: size_of::<Billboard>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &wgpu::vertex_attr_array![
                0 => Float32x3, 1 => Float32, 2 => Float32x4, 3 => Sint32, 4 => Unorm8x4, 5 => Uint32,
            ],
        })];
        let mesh = |fragment, blend, depth| {
            let pipeline = builder.build(
                globals_only,
                ["vs_mesh", fragment],
                &mesh_vertices,
                blend,
                depth,
                None,
            );
            Batch::new(device, pipeline)
        };
        let instanced = |vertex, fragment, buffers: &[_], blend, depth| {
            let pipeline = builder.build(
                globals_only,
                [vertex, fragment],
                buffers,
                blend,
                depth,
                None,
            );
            Batch::new(device, pipeline)
        };

        Pipelines {
            opaque: builder.build(
                terrain_layout,
                ["vs_terrain", "fs_opaque"],
                &terrain_vertices,
                None,
                Depth::Write,
                Some(wgpu::Face::Back),
            ),
            translucent: builder.build(
                terrain_layout,
                ["vs_terrain", "fs_translucent"],
                &terrain_vertices,
                ALPHA,
                Depth::Test,
                None,
            ),
            cuboids: instanced(
                "vs_cuboid",
                "fs_cuboid",
                &cuboid_instances,
                None,
                Depth::Write,
            ),
            glint_cuboids: instanced(
                "vs_cuboid",
                "fs_armor_glint",
                &cuboid_instances,
                ADDITIVE,
                Depth::Equal,
            ),
            solid: mesh("fs_mesh", None, Depth::Write),
            glint_meshes: mesh("fs_item_glint", ADDITIVE, Depth::Equal),
            cracks: mesh("fs_crack", MULTIPLY, Depth::Test),
            xray: mesh("fs_blended", ALPHA, Depth::Ignore),
            blended: mesh("fs_blended", ALPHA, Depth::Test),
            particles: instanced(
                "vs_billboard",
                "fs_particle",
                &billboard_instances,
                ALPHA,
                Depth::Test,
            ),
        }
    }
}

pub(crate) fn pipeline_layouts(
    device: &wgpu::Device,
    globals: &wgpu::BindGroupLayout,
    section: &wgpu::BindGroupLayout,
) -> [wgpu::PipelineLayout; 2] {
    let layout = |groups: &[Option<&wgpu::BindGroupLayout>]| {
        device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: groups,
            immediate_size: 0,
        })
    };
    [
        layout(&[Some(globals)]),
        layout(&[Some(globals), Some(section)]),
    ]
}

fn vertex_buffer(device: &wgpu::Device, size: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}
