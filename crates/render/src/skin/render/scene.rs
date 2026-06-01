use image::{Rgba, RgbaImage};
use wgpu::util::DeviceExt;

use super::{
    camera::Camera,
    context::RenderContext,
    pipeline::{Mesh, RenderPipeline, Uniforms},
};
use crate::skin::{
    Result, Skin, SkinError,
    model::{BodyPart, Pose},
    output::{OutputType, RenderOutput},
};

pub struct Renderer {
    ctx: RenderContext,
    pipeline: RenderPipeline,
}

impl Renderer {
    pub fn new() -> Result<Self> {
        let ctx = RenderContext::new()?;
        let pipeline = RenderPipeline::new(&ctx);
        Ok(Self { ctx, pipeline })
    }

    pub fn render(&self, skin: &Skin, pose: &Pose, output: OutputType) -> Result<RenderOutput> {
        match output {
            OutputType::FullBody { width, height } => {
                self.render_3d(skin, pose, width, height, false)
            }
            OutputType::Head { width, height } => self.render_3d(skin, pose, width, height, true),
            OutputType::Face { size } => Ok(self.render_face(skin, size)),
        }
    }

    fn render_3d(
        &self,
        skin: &Skin,
        pose: &Pose,
        width: u32,
        height: u32,
        head_only: bool,
    ) -> Result<RenderOutput> {
        let parts: Vec<BodyPart> = if head_only {
            vec![BodyPart::Head, BodyPart::HeadLayer]
        } else {
            BodyPart::BASE_PARTS
                .iter()
                .chain(BodyPart::LAYER_PARTS.iter())
                .copied()
                .collect()
        };

        let mesh = Mesh::build_from_skin(skin, pose, &parts);
        let (vertex_buffer, index_buffer, index_count) = mesh.create_buffers(&self.ctx);

        let camera = if head_only {
            Camera::for_head()
        } else {
            Camera::for_full_body()
        };
        let mvp = camera.view_projection_matrix(width as f32 / height as f32);
        let light_direction = glam::Vec3::new(0.3, 0.8, -0.5).normalize();

        let uniform_buffer =
            self.ctx
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("uniform_buffer"),
                    contents: bytemuck::cast_slice(&[Uniforms {
                        mvp: mvp.to_cols_array_2d(),
                        light_direction: light_direction.to_array(),
                        ambient: 0.4,
                    }]),
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                });

        let texture = self.upload_skin_texture(skin);
        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = self.ctx.device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let bind_group = self
            .ctx
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("skin_bind_group"),
                layout: &self.pipeline.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: uniform_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&texture_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&sampler),
                    },
                ],
            });

        let extent = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };

        let output_texture = self.ctx.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("output_texture"),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let output_view = output_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let depth_texture = self.ctx.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("depth_texture"),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let depth_view = depth_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .ctx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("render_encoder"),
            });

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("render_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &output_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            pass.set_pipeline(&self.pipeline.pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.set_vertex_buffer(0, vertex_buffer.slice(..));
            pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..index_count, 0, 0..1);
        }

        let bytes_per_row = (width * 4 + 255) & !255;
        let output_buffer = self.ctx.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("output_buffer"),
            size: (bytes_per_row * height) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &output_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &output_buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(bytes_per_row),
                    rows_per_image: Some(height),
                },
            },
            extent,
        );

        self.ctx.queue.submit(std::iter::once(encoder.finish()));

        let buffer_slice = output_buffer.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        self.ctx.device.poll(wgpu::Maintain::Wait);
        rx.recv()
            .map_err(|_| SkinError::Render("GPU readback channel closed".into()))?
            .map_err(|e| SkinError::Render(e.to_string()))?;

        let data = buffer_slice.get_mapped_range();
        let mut image_data = Vec::with_capacity((width * height * 4) as usize);
        for row in 0..height {
            let start = (row * bytes_per_row) as usize;
            image_data.extend_from_slice(&data[start..start + (width * 4) as usize]);
        }
        drop(data);
        output_buffer.unmap();

        RgbaImage::from_raw(width, height, image_data)
            .map(|image| RenderOutput { image })
            .ok_or_else(|| SkinError::Render("failed to create output image".into()))
    }

    fn render_face(&self, skin: &Skin, size: u32) -> RenderOutput {
        let texture = skin.texture();
        let mut face = RgbaImage::new(size, size);
        for y in 0..size {
            for x in 0..size {
                let base = *texture.get_pixel(8 + x * 8 / size, 8 + y * 8 / size);
                let overlay = *texture.get_pixel(40 + x * 8 / size, 8 + y * 8 / size);
                face.put_pixel(x, y, blend_pixels(base, overlay));
            }
        }
        RenderOutput { image: face }
    }

    fn upload_skin_texture(&self, skin: &Skin) -> wgpu::Texture {
        let extent = wgpu::Extent3d {
            width: 64,
            height: 64,
            depth_or_array_layers: 1,
        };
        let texture = self.ctx.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("skin_texture"),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.ctx.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            skin.texture(),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(64 * 4),
                rows_per_image: Some(64),
            },
            extent,
        );
        texture
    }
}

fn blend_pixels(base: Rgba<u8>, overlay: Rgba<u8>) -> Rgba<u8> {
    let alpha = overlay.0[3] as f32 / 255.0;
    match alpha {
        a if a == 0.0 => base,
        a if a == 1.0 => overlay,
        _ => {
            let b = |b: u8, o: u8| ((b as f32 * (1.0 - alpha)) + (o as f32 * alpha)) as u8;
            Rgba([
                b(base.0[0], overlay.0[0]),
                b(base.0[1], overlay.0[1]),
                b(base.0[2], overlay.0[2]),
                255,
            ])
        }
    }
}
