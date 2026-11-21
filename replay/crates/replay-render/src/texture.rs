use std::iter::successors;

use replay_assets::halve;

pub(crate) const TILE_SIZE: u32 = 16;
pub const SKIN_SIZE: u32 = 64;
pub(crate) const SKIN_LEVELS: u32 = SKIN_SIZE.ilog2() + 1;

pub(crate) fn layered(bytes: &[u8]) -> Option<(u32, Vec<&[u8]>)> {
    let (layers, rest) = bytes.split_first_chunk()?;
    let (levels, mut rest) = rest.split_first_chunk()?;
    let [layers, levels] = [layers, levels].map(|value| u32::from_le_bytes(*value));
    if layers == 0 || !(1..=TILE_SIZE.ilog2() + 1).contains(&levels) {
        return None;
    }
    let mips = (0..levels)
        .map(|level| {
            let size = ((TILE_SIZE >> level) as usize).pow(2) * 4 * layers as usize;
            let (mip, tail) = rest.split_at_checked(size)?;
            rest = tail;
            Some(mip)
        })
        .collect::<Option<Vec<_>>>()?;
    Some((layers, mips))
}

pub(crate) fn unambiguous_array_layers(layers: u32) -> u32 {
    let layers = layers.max(2);
    if layers.is_multiple_of(6) {
        layers + 1
    } else {
        layers
    }
}

pub(crate) fn array(device: &wgpu::Device, size: u32, layers: u32, levels: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: unambiguous_array_layers(layers),
        },
        mip_level_count: levels,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

pub(crate) fn write_layers(
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    first: u32,
    layers: u32,
    mips: &[impl AsRef<[u8]>],
) {
    for (level, mip) in mips.iter().enumerate() {
        let size = texture.width() >> level;
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: level as u32,
                origin: wgpu::Origin3d {
                    x: 0,
                    y: 0,
                    z: first,
                },
                aspect: wgpu::TextureAspect::All,
            },
            mip.as_ref(),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(size * 4),
                rows_per_image: Some(size),
            },
            wgpu::Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: layers,
            },
        );
    }
}

pub(crate) fn sheet(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    width: u32,
    height: u32,
    rgba: &[u8],
    levels: u32,
) -> wgpu::TextureView {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: levels.min(width.max(height).ilog2() + 1),
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for (level, (mip, width, height)) in mips(rgba, width, height)
        .take(texture.mip_level_count() as usize)
        .enumerate()
    {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                mip_level: level as u32,
                ..texture.as_image_copy()
            },
            &mip,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
    }
    texture.create_view(&Default::default())
}

pub(crate) fn mips(
    rgba: &[u8],
    width: u32,
    height: u32,
) -> impl Iterator<Item = (Vec<u8>, u32, u32)> {
    successors(
        Some((rgba.to_vec(), width, height)),
        |(rgba, width, height)| Some(halve(rgba, *width as usize, *height as usize)),
    )
}

pub(crate) fn array_view(texture: &wgpu::Texture) -> wgpu::TextureView {
    texture.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    })
}
