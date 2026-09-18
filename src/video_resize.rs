//! Fixed-point bilinear scaling for opaque desktop frames. Coordinates sample pixel
//! centers, clamp at the edges, and avoid the floating-point convolution passes.
use xcap::image::RgbaImage;

pub fn resize(frame: &RgbaImage, width: u32, height: u32) -> RgbaImage {
    assert!(width > 0 && height > 0 && frame.width() > 0 && frame.height() > 0);
    if frame.dimensions() == (width, height) {
        return frame.clone();
    }
    fn axis(source: u32, target: u32) -> Vec<(usize, usize, u32)> {
        (0..target)
            .map(|i| {
                let position = (((2 * u64::from(i) + 1) * u64::from(source) * 128)
                    / u64::from(target))
                .saturating_sub(128);
                let left = (position / 256).min(u64::from(source - 1)) as usize;
                (
                    left,
                    (left + 1).min(source as usize - 1),
                    (position % 256) as u32,
                )
            })
            .collect()
    }
    let xs = axis(frame.width(), width);
    let ys = axis(frame.height(), height);
    let stride = frame.width() as usize * 4;
    let input = frame.as_raw();
    let mut result = RgbaImage::new(width, height);
    for (row, &(y0, y1, fy)) in result
        .as_mut()
        .chunks_exact_mut(width as usize * 4)
        .zip(&ys)
    {
        for (pixel, &(x0, x1, fx)) in row.as_chunks_mut::<4>().0.iter_mut().zip(&xs) {
            for channel in 0..3 {
                let a = u32::from(input[y0 * stride + x0 * 4 + channel]);
                let b = u32::from(input[y0 * stride + x1 * 4 + channel]);
                let c = u32::from(input[y1 * stride + x0 * 4 + channel]);
                let d = u32::from(input[y1 * stride + x1 * 4 + channel]);
                pixel[channel] = (((a * (256 - fx) + b * fx) * (256 - fy)
                    + (c * (256 - fx) + d * fx) * fy
                    + 32768)
                    >> 16) as u8;
            }
            pixel[3] = 255;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use xcap::image::Rgba;
    #[test]
    fn downsample_averages_pixel_centers() {
        let source =
            RgbaImage::from_fn(4, 4, |x, y| Rgba([(x * 40) as u8, (y * 40) as u8, 80, 255]));
        let scaled = resize(&source, 2, 2);
        assert_eq!(scaled.get_pixel(0, 0).0, [20, 20, 80, 255]);
        assert_eq!(scaled.get_pixel(1, 1).0, [100, 100, 80, 255]);
    }
    #[test]
    fn constant_image_and_edge_dimensions_are_preserved() {
        for (w, h) in [(1, 1), (7, 3), (1920, 1080)] {
            let source = RgbaImage::from_pixel(w, h, Rgba([17, 98, 201, 255]));
            for (tw, th) in [(1, 1), (2, 2), (13, 9)] {
                assert!(
                    resize(&source, tw, th)
                        .pixels()
                        .all(|p| p.0 == [17, 98, 201, 255])
                );
            }
            assert_eq!(resize(&source, w, h), source);
        }
    }
}
