pub const GAMMA: f32 = 2.2;

pub fn halve(rgba: &[u8], width: usize, height: usize) -> (Vec<u8>, u32, u32) {
    let [half_width, half_height] = [width, height].map(|size| (size / 2).max(1));
    let texel = |x: usize, y: usize| {
        let at = (y.min(height - 1) * width + x.min(width - 1)) * 4;
        &rgba[at..at + 4]
    };
    let halved = (0..half_height)
        .flat_map(|y| (0..half_width).map(move |x| (x * 2, y * 2)))
        .flat_map(|(x, y)| {
            let texels = [
                texel(x, y),
                texel(x + 1, y),
                texel(x, y + 1),
                texel(x + 1, y + 1),
            ];
            let alpha: f32 = texels.iter().map(|texel| f32::from(texel[3])).sum();
            let weight = |texel: &[u8]| {
                if alpha > 0.0 {
                    f32::from(texel[3])
                } else {
                    1.0
                }
            };
            let total: f32 = texels.iter().map(|texel| weight(texel)).sum();
            let channel = |channel: usize| {
                let linear: f32 = texels
                    .iter()
                    .map(|texel| (f32::from(texel[channel]) / 255.0).powf(GAMMA) * weight(texel))
                    .sum();
                ((linear / total).powf(GAMMA.recip()) * 255.0).round() as u8
            };
            [
                channel(0),
                channel(1),
                channel(2),
                (alpha / 4.0).round() as u8,
            ]
        })
        .collect();
    (halved, half_width as u32, half_height as u32)
}
