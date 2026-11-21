mod camera;
mod daylight;
mod gpu;
mod pipelines;
mod scene;
mod texture;

use std::collections::HashMap;

use bytemuck::{Pod, Zeroable};
use camera::Frustum;
use glam::{IVec3, Mat4, Vec3, Vec4};
use pipelines::{Pipelines, pipeline_layouts};
use replay_assets::GAMMA;
use replay_world::{Geometry, SectionMesh};
use texture::{SKIN_LEVELS, TILE_SIZE};
use wgpu::util::DeviceExt;

pub use camera::Camera;
pub use daylight::Daylight;
pub use gpu::{Gpu, Tier};
pub use scene::{Billboard, Cuboid, MeshVertex, PLAIN, SHEET, Scene, WHITE, quad};
pub use texture::SKIN_SIZE;
pub use wgpu::SurfaceTarget;

const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const SECTION_SIZE: i32 = 16;
const CUBOID_VERTICES: u32 = 36;
const QUAD_VERTICES: u32 = QUAD_INDICES.len() as u32;
const QUAD_INDICES: [u32; 6] = [0, 1, 2, 0, 2, 3];
const MSAA_SAMPLES: u32 = 4;
const SHEET_LEVELS: u32 = 4;
const BACKGROUND: [u8; 3] = [31, 31, 34];

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("cannot create surface: {0}")]
    Surface(#[from] wgpu::CreateSurfaceError),
    #[error("no graphics adapter: {0}")]
    Adapter(#[from] wgpu::RequestAdapterError),
    #[error("cannot open graphics device: {0}")]
    Device(#[from] wgpu::RequestDeviceError),
    #[error("surface is not supported by the adapter")]
    UnsupportedSurface,
    #[error("textures exceed this GPU's limits or are malformed")]
    Textures,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Globals {
    view_proj: Mat4,
    camera_block: IVec3,
    fog_start: f32,
    camera_fract: Vec3,
    fog_end: f32,
    glint: Vec4,
    time: f32,
    padding: [f32; 3],
    background: Vec4,
    focus: Vec4,
    lightmap: [Vec4; 256],
}

struct GpuGeometry {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    count: u32,
}

impl GpuGeometry {
    fn upload(device: &wgpu::Device, geometry: &Geometry) -> Option<Self> {
        let buffer = |contents: &[u8], usage| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None,
                contents,
                usage,
            })
        };
        (!geometry.indices.is_empty()).then(|| Self {
            vertices: buffer(
                bytemuck::cast_slice(&geometry.vertices),
                wgpu::BufferUsages::VERTEX,
            ),
            indices: buffer(
                bytemuck::cast_slice(&geometry.indices),
                wgpu::BufferUsages::INDEX,
            ),
            count: geometry.indices.len() as u32,
        })
    }

    fn draw(&self, pass: &mut wgpu::RenderPass, section: &wgpu::BindGroup) {
        pass.set_bind_group(1, section, &[]);
        pass.set_vertex_buffer(0, self.vertices.slice(..));
        pass.set_index_buffer(self.indices.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..self.count, 0, 0..1);
    }
}

struct Section {
    bind_group: wgpu::BindGroup,
    opaque: Option<GpuGeometry>,
    translucent: Option<GpuGeometry>,
}

pub struct Renderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    gpu: Gpu,
    samples: u32,
    depth: wgpu::TextureView,
    multisampled: Option<wgpu::TextureView>,
    globals: wgpu::Buffer,
    globals_layout: wgpu::BindGroupLayout,
    globals_bind_group: wgpu::BindGroup,
    section_layout: wgpu::BindGroupLayout,
    block_sampler: wgpu::Sampler,
    pixel_sampler: wgpu::Sampler,
    entity_sampler: wgpu::Sampler,
    blocks: wgpu::TextureView,
    sheet: wgpu::TextureView,
    skins: wgpu::Texture,
    skin_view: wgpu::TextureView,
    pipelines: Pipelines,
    quads: wgpu::Buffer,
    quad_capacity: u32,
    sections: HashMap<[i32; 3], Section>,
    fog: [f32; 2],
    glint: Vec4,
    daylight: Daylight,
}

impl Renderer {
    pub async fn new(
        target: impl Into<SurfaceTarget<'static>>,
        width: u32,
        height: u32,
        web_gpu: bool,
    ) -> Result<Self, Error> {
        let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
        if !web_gpu {
            descriptor.backends.remove(wgpu::Backends::BROWSER_WEBGPU);
        }
        let instance = wgpu::Instance::new(descriptor);
        let surface = instance.create_surface(target)?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                ..Default::default()
            })
            .await?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                required_limits: adapter.limits(),
                ..Default::default()
            })
            .await?;

        let mut config = surface
            .get_default_config(&adapter, width.max(1), height.max(1))
            .ok_or(Error::UnsupportedSurface)?;
        let formats = surface.get_capabilities(&adapter).formats;
        config.format = formats
            .into_iter()
            .find(wgpu::TextureFormat::is_srgb)
            .unwrap_or(config.format);
        let target_format = config.format.add_srgb_suffix();
        config.view_formats = vec![target_format];
        surface.configure(&device, &config);
        let gpu = Gpu::new(&adapter.get_info());
        let multisampling = [target_format, DEPTH_FORMAT].into_iter().all(|format| {
            let flags = adapter.get_texture_format_features(format).flags;
            flags.sample_count_supported(MSAA_SAMPLES)
                && (format.is_depth_stencil_format()
                    || flags.contains(wgpu::TextureFormatFeatureFlags::MULTISAMPLE_RESOLVE))
        });
        let samples = if multisampling { MSAA_SAMPLES } else { 1 };

        let [globals_layout, section_layout] = bind_group_layouts(&device);
        let shader = device.create_shader_module(wgpu::include_wgsl!("shader.wgsl"));
        let [globals_only, terrain_layout] =
            pipeline_layouts(&device, &globals_layout, &section_layout);

        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let block_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            anisotropy_clamp: gpu.anisotropy(),
            ..Default::default()
        });
        let pixel_sampler = device.create_sampler(&Default::default());
        let entity_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let white = [u8::MAX; (SKIN_SIZE * SKIN_SIZE * 4) as usize];
        let block_array = texture::array(&device, TILE_SIZE, 1, 1);
        let tile = &white[..(TILE_SIZE * TILE_SIZE * 4) as usize];
        texture::write_layers(&queue, &block_array, 0, 1, &[tile]);
        let blocks = texture::array_view(&block_array);
        let skins = texture::array(&device, SKIN_SIZE, 1, 1);
        texture::write_layers(&queue, &skins, 0, 1, &[&white]);
        let skin_view = texture::array_view(&skins);
        let sheet = texture::sheet(&device, &queue, 1, 1, &white[..4], 1);
        let globals_bind_group = globals_bind_group(
            &device,
            &globals_layout,
            &globals,
            [&blocks, &sheet, &skin_view],
            [&block_sampler, &pixel_sampler, &entity_sampler],
        );

        Ok(Self {
            depth: attachment(&device, &config, DEPTH_FORMAT, samples),
            multisampled: (samples > 1)
                .then(|| attachment(&device, &config, target_format, samples)),
            pipelines: Pipelines::new(
                &device,
                &shader,
                [&globals_only, &terrain_layout],
                target_format,
                samples,
            ),
            quads: quad_indices(&device, 1),
            quad_capacity: 1,
            sections: HashMap::new(),
            fog: [400.0, 900.0],
            glint: Vec4::ZERO,
            daylight: Daylight::default(),
            surface,
            device,
            queue,
            config,
            gpu,
            samples,
            globals,
            globals_layout,
            globals_bind_group,
            section_layout,
            block_sampler,
            pixel_sampler,
            entity_sampler,
            blocks,
            sheet,
            skins,
            skin_view,
        })
    }

    pub fn gpu(&self) -> &Gpu {
        &self.gpu
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.config.width = width.max(1);
        self.config.height = height.max(1);
        self.configure();
    }

    pub fn set_fog(&mut self, start: f32, end: f32) {
        self.fog = [start, end];
    }

    pub fn set_daylight(&mut self, daylight: Daylight) {
        self.daylight = daylight;
    }

    pub fn set_glint(&mut self, rect: [f32; 4]) {
        self.glint = rect.into();
    }

    pub fn set_block_textures(&mut self, blocks: &[u8], items: &[u8]) -> Result<i32, Error> {
        let (block_layers, block_mips) = texture::layered(blocks).ok_or(Error::Textures)?;
        let (item_layers, item_mips) = texture::layered(items).ok_or(Error::Textures)?;
        let layers = block_layers + item_layers;
        if block_mips.len() != item_mips.len()
            || texture::unambiguous_array_layers(layers)
                > self.device.limits().max_texture_array_layers
        {
            return Err(Error::Textures);
        }
        let array = texture::array(&self.device, TILE_SIZE, layers, block_mips.len() as u32);
        texture::write_layers(&self.queue, &array, 0, block_layers, &block_mips);
        texture::write_layers(&self.queue, &array, block_layers, item_layers, &item_mips);
        self.blocks = texture::array_view(&array);
        self.rebind();
        Ok(block_layers as i32)
    }

    pub fn set_sheet(&mut self, width: u32, height: u32, rgba: &[u8]) -> Result<(), Error> {
        let limit = self.device.limits().max_texture_dimension_2d;
        if width == 0
            || height == 0
            || width.max(height) > limit
            || rgba.len() != width as usize * height as usize * 4
        {
            return Err(Error::Textures);
        }
        self.sheet = texture::sheet(&self.device, &self.queue, width, height, rgba, SHEET_LEVELS);
        self.rebind();
        Ok(())
    }

    pub fn set_skin_count(&mut self, count: u32) -> u32 {
        let count = count.clamp(1, self.device.limits().max_texture_array_layers - 1);
        self.skins = texture::array(&self.device, SKIN_SIZE, count, SKIN_LEVELS);
        self.skin_view = texture::array_view(&self.skins);
        self.rebind();
        count
    }

    pub fn set_skin(&mut self, layer: u32, rgba: &[u8]) {
        let fits = layer < self.skins.depth_or_array_layers()
            && rgba.len() == (SKIN_SIZE * SKIN_SIZE * 4) as usize;
        if fits {
            let mips: Vec<_> = texture::mips(rgba, SKIN_SIZE, SKIN_SIZE)
                .take(self.skins.mip_level_count() as usize)
                .map(|(mip, ..)| mip)
                .collect();
            texture::write_layers(&self.queue, &self.skins, layer, 1, &mips);
        }
    }

    pub fn upload_section(&mut self, pos: [i32; 3], mesh: &SectionMesh) {
        let opaque = GpuGeometry::upload(&self.device, &mesh.opaque);
        let translucent = GpuGeometry::upload(&self.device, &mesh.translucent);
        if opaque.is_none() && translucent.is_none() {
            self.sections.remove(&pos);
            return;
        }
        let origin = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None,
                contents: bytemuck::cast_slice(&[pos[0], pos[1], pos[2], 0]),
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.section_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: origin.as_entire_binding(),
            }],
        });
        self.sections.insert(
            pos,
            Section {
                bind_group,
                opaque,
                translucent,
            },
        );
    }

    pub fn clear_sections(&mut self) {
        self.sections.clear();
    }

    pub fn render(&mut self, camera: &Camera, scene: &Scene) {
        let (frame, suboptimal) = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) => (frame, false),
            wgpu::CurrentSurfaceTexture::Suboptimal(frame) => (frame, true),
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                return self.configure();
            }
            _ => return,
        };

        let view_proj = camera.view_proj(self.config.width as f32 / self.config.height as f32);
        let camera_floor = camera.position.floor();
        let camera_block = camera_floor.as_ivec3();
        let camera_fract = camera.position - camera_floor;
        let globals = Globals {
            view_proj,
            camera_block,
            fog_start: self.fog[0],
            camera_fract,
            fog_end: self.fog[1],
            glint: self.glint,
            time: scene.time,
            padding: [0.0; 3],
            background: background().extend(1.0),
            focus: scene.focus.map_or(Vec4::ZERO, |(center, strength)| {
                (center - camera.position).extend(strength)
            }),
            lightmap: self.daylight.lightmap(),
        };
        self.queue
            .write_buffer(&self.globals, 0, bytemuck::bytes_of(&globals));
        self.upload(scene);

        let far_to_near = self.visible_sections(view_proj, camera_block, camera_fract);
        let target = frame.texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(self.config.format.add_srgb_suffix()),
            ..Default::default()
        });
        let mut encoder = self.device.create_command_encoder(&Default::default());
        self.draw_scene(&mut encoder, &target, &far_to_near);
        self.queue.submit([encoder.finish()]);
        self.queue.present(frame);
        if suboptimal {
            self.configure();
        }
    }

    fn visible_sections(
        &self,
        view_proj: Mat4,
        camera_block: IVec3,
        camera_fract: Vec3,
    ) -> Vec<(f32, &Section)> {
        let frustum = Frustum::new(view_proj);
        let mut far_to_near: Vec<(f32, &Section)> = self
            .sections
            .iter()
            .filter_map(|(pos, section)| {
                let min =
                    (IVec3::from(*pos) * SECTION_SIZE - camera_block).as_vec3() - camera_fract;
                let max = min + SECTION_SIZE as f32;
                frustum
                    .intersects(min, max)
                    .then(|| ((min + max).length_squared(), section))
            })
            .collect();
        far_to_near.sort_by(|a, b| b.0.total_cmp(&a.0));
        far_to_near
    }

    fn draw_scene(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        far_to_near: &[(f32, &Section)],
    ) {
        let (view, resolve_target, store) = match &self.multisampled {
            Some(multisampled) => (multisampled, Some(target), wgpu::StoreOp::Discard),
            None => (target, None, wgpu::StoreOp::Store),
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear({
                        let [r, g, b] = background().as_dvec3().to_array();
                        wgpu::Color { r, g, b, a: 1.0 }
                    }),
                    store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.depth,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(0.0),
                    store: wgpu::StoreOp::Discard,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        let pipelines = &self.pipelines;
        pass.set_bind_group(0, &self.globals_bind_group, &[]);

        pass.set_pipeline(&pipelines.opaque);
        for (_, section) in far_to_near.iter().rev() {
            if let Some(geometry) = &section.opaque {
                geometry.draw(&mut pass, &section.bind_group);
            }
        }

        pipelines.cuboids.draw_instances(&mut pass, CUBOID_VERTICES);
        pass.set_index_buffer(self.quads.slice(..), wgpu::IndexFormat::Uint32);
        pipelines.solid.draw_quads(&mut pass);
        pipelines
            .glint_cuboids
            .draw_instances(&mut pass, CUBOID_VERTICES);
        pipelines.glint_meshes.draw_quads(&mut pass);
        pipelines.cracks.draw_quads(&mut pass);

        pass.set_pipeline(&pipelines.translucent);
        for (_, section) in far_to_near {
            if let Some(geometry) = &section.translucent {
                geometry.draw(&mut pass, &section.bind_group);
            }
        }

        pass.set_index_buffer(self.quads.slice(..), wgpu::IndexFormat::Uint32);
        pipelines.xray.draw_quads(&mut pass);
        pipelines.blended.draw_quads(&mut pass);
        pipelines.particles.draw_instances(&mut pass, QUAD_VERTICES);
    }

    fn upload(&mut self, scene: &Scene) {
        let (device, queue) = (&self.device, &self.queue);
        let pipelines = &mut self.pipelines;
        pipelines.cuboids.upload(device, queue, &scene.cuboids);
        pipelines
            .glint_cuboids
            .upload(device, queue, &scene.glint_cuboids);
        pipelines.particles.upload(device, queue, &scene.particles);
        let meshes = [
            (&mut pipelines.solid, &scene.solid),
            (&mut pipelines.glint_meshes, &scene.glint_meshes),
            (&mut pipelines.cracks, &scene.cracks),
            (&mut pipelines.xray, &scene.xray),
            (&mut pipelines.blended, &scene.blended),
        ];
        let mut quads = 0;
        for (batch, vertices) in meshes {
            batch.upload(device, queue, vertices);
            quads = quads.max(batch.count / 4);
        }
        if quads > self.quad_capacity {
            self.quad_capacity = quads.next_power_of_two();
            self.quads = quad_indices(device, self.quad_capacity);
        }
    }

    fn rebind(&mut self) {
        self.globals_bind_group = globals_bind_group(
            &self.device,
            &self.globals_layout,
            &self.globals,
            [&self.blocks, &self.sheet, &self.skin_view],
            [
                &self.block_sampler,
                &self.pixel_sampler,
                &self.entity_sampler,
            ],
        );
    }

    fn configure(&mut self) {
        self.surface.configure(&self.device, &self.config);
        self.depth = attachment(&self.device, &self.config, DEPTH_FORMAT, self.samples);
        let format = self.config.format.add_srgb_suffix();
        self.multisampled = self
            .multisampled
            .is_some()
            .then(|| attachment(&self.device, &self.config, format, self.samples));
    }
}

fn bind_group_layouts(device: &wgpu::Device) -> [wgpu::BindGroupLayout; 2] {
    let uniform = |binding, visibility| wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    };
    let texture = |binding, view_dimension| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension,
            multisampled: false,
        },
        count: None,
    };
    let sampler = |binding| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        count: None,
    };
    let globals = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &[
            uniform(0, wgpu::ShaderStages::VERTEX_FRAGMENT),
            texture(1, wgpu::TextureViewDimension::D2Array),
            sampler(2),
            texture(3, wgpu::TextureViewDimension::D2),
            texture(4, wgpu::TextureViewDimension::D2Array),
            sampler(5),
            sampler(6),
        ],
    });
    let section = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &[uniform(0, wgpu::ShaderStages::VERTEX)],
    });
    [globals, section]
}

fn globals_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    globals: &wgpu::Buffer,
    [blocks, sheet, skins]: [&wgpu::TextureView; 3],
    [block_sampler, pixel_sampler, entity_sampler]: [&wgpu::Sampler; 3],
) -> wgpu::BindGroup {
    let view = |binding, view| wgpu::BindGroupEntry {
        binding,
        resource: wgpu::BindingResource::TextureView(view),
    };
    let sampler = |binding, sampler| wgpu::BindGroupEntry {
        binding,
        resource: wgpu::BindingResource::Sampler(sampler),
    };
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: globals.as_entire_binding(),
            },
            view(1, blocks),
            sampler(2, block_sampler),
            view(3, sheet),
            view(4, skins),
            sampler(5, pixel_sampler),
            sampler(6, entity_sampler),
        ],
    })
}

fn attachment(
    device: &wgpu::Device,
    config: &wgpu::SurfaceConfiguration,
    format: wgpu::TextureFormat,
    samples: u32,
) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: config.width,
                height: config.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: samples,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
        .create_view(&Default::default())
}

fn quad_indices(device: &wgpu::Device, quads: u32) -> wgpu::Buffer {
    let indices: Vec<u32> = (0..quads)
        .flat_map(|quad| QUAD_INDICES.map(|corner| quad * 4 + corner))
        .collect();
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::cast_slice(&indices),
        usage: wgpu::BufferUsages::INDEX,
    })
}

fn background() -> Vec3 {
    Vec3::from(BACKGROUND.map(|channel| (f32::from(channel) / 255.0).powf(GAMMA)))
}
