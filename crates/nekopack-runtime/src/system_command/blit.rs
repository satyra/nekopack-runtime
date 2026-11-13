//! Original five/six-argument Draw rectangle kernel (0x694208 / 0x6945F8).
//! Preserve native row alignment and scalar/SIMD rounding; see System_Gui_Native_Contracts.md.
use nekopack_image::RgbaImage;
use super::Result;

pub(crate) fn blit(
    target: &mut RgbaImage, source: &RgbaImage,
    pos: [i32; 2], extent: [i32; 2], opacity: i32,
) -> Result<()> {
    if opacity <= 0 { return Ok(()); }
    let [mut x, mut y] = pos.map(i64::from);
    let [mut width, mut height] = extent.map(i64::from);
    let mut sx = 0;
    let mut sy = 0;
    if x < 0 { width += x; sx = -x; x = 0; }
    if y < 0 { height += y; sy = -y; y = 0; }
    width = width.min(i64::from(target.width()) - x).min(i64::from(source.width()) - sx);
    height = height.min(i64::from(target.height()) - y).min(i64::from(source.height()) - sy);
    if width <= 0 || height <= 0 { return Ok(()); }
    let width = width as usize;
    let target_stride = target.width() as usize;
    let source_stride = source.width() as usize;
    for row in 0..height as usize {
        let dst_pixel = (y as usize + row) * target_stride + x as usize;
        let src_pixel = (sy as usize + row) * source_stride + sx as usize;
        let dst = &mut target.pixels_mut()[dst_pixel * 4..][..width * 4];
        let src = &source.pixels()[src_pixel * 4..][..width * 4];
        row_blit(dst, src, dst_pixel & 1 != 0, opacity);
    }
    Ok(())
}

fn interpolate(dst: &mut [u8], src: &[u8], alpha: i32) {
    for (old, &next) in dst.iter_mut().zip(src) {
        *old = (i32::from(*old) + ((i32::from(next) - i32::from(*old)) * alpha).div_euclid(256)) as u8;
    }
}

fn scalar(dst: &mut [u8], src: &[u8], opacity: i32) {
    if opacity >= 255 {
        if src[3] == 255 { dst.copy_from_slice(src); }
        else { interpolate(dst, src, i32::from(src[3])); }
    } else {
        // Native memory is BGRA; mul uses the complete unsigned 32-bit pixel,
        // then EDX (high 32 bits), rather than just source alpha.
        let bgra = u32::from_le_bytes([src[2], src[1], src[0], src[3]]);
        let alpha = ((u64::from(bgra) * opacity as u64) >> 32) as i32;
        interpolate(dst, src, alpha);
    }
}

fn row_blit(dst: &mut [u8], src: &[u8], unaligned: bool, opacity: i32) {
    let count = dst.len() / 4;
    // Native 0x694774 reserves four tail pixels; 0x694BB4 reserves two.
    // VirtualAlloc aligns the base, so alignment depends on the pixel offset,
    // including odd-width preceding rows, rather than this Vec's address.
    let tail = if opacity >= 255 { 4 } else { 2 };
    let mut at = 0;
    if count > tail {
        if unaligned {
            scalar(&mut dst[..4], &src[..4], opacity);
            at = 1;
        }
        while at + tail < count {
            let start = at * 4;
            if opacity >= 255 && src[start + 3] == 255 && src[start + 7] == 255 {
                dst[start..start + 8].copy_from_slice(&src[start..start + 8]);
            } else {
                for pixel in at..at + 2 {
                    let start = pixel * 4;
                    let alpha = i32::from(src[start + 3]);
                    let alpha = if opacity >= 255 { alpha } else { alpha * opacity / 256 };
                    interpolate(&mut dst[start..start + 4], &src[start..start + 4], alpha);
                }
            }
            at += 2;
        }
    }
    for pixel in at..count {
        let start = pixel * 4;
        scalar(&mut dst[start..start + 4], &src[start..start + 4], opacity);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const BUDGET: usize = 1024 * 1024;
    fn filled(w: u32, h: u32, value: [u8; 4]) -> RgbaImage {
        RgbaImage::filled(w, h, value, BUDGET).unwrap()
    }
    fn pixel(image: &RgbaImage, x: usize, y: usize) -> &[u8] {
        &image.pixels()[(y * image.width() as usize + x) * 4..][..4]
    }

    #[test]
    fn clip_negative_origin_and_source_edges_without_scaling() {
        let src = RgbaImage::from_rgba(3, 2, vec![
            10,0,0,255, 20,0,0,255, 30,0,0,255,
            40,0,0,255, 50,0,0,255, 60,0,0,255,
        ], BUDGET).unwrap();
        let mut dst = filled(4, 3, [0; 4]);
        blit(&mut dst, &src, [-1,-1], [8,8], 255).unwrap();
        assert_eq!(pixel(&dst,0,0), &[50,0,0,255]);
        assert_eq!(pixel(&dst,1,0), &[60,0,0,255]);
        assert_eq!(pixel(&dst,2,0), &[0;4]);
        assert_eq!(pixel(&dst,0,1), &[0;4]);
        let before = dst.pixels().to_vec();
        for (pos, extent, opacity) in [([0,0],[0,2],255),([0,0],[3,-1],255),([4,0],[3,2],255),([0,0],[3,2],0),([i32::MIN,0],[3,2],255)] {
            blit(&mut dst,&src,pos,extent,opacity).unwrap();
            assert_eq!(dst.pixels(),before);
        }
    }

    #[test]
    fn full_opacity_mixed_pairs_differ_from_scalar_and_opaque_pairs() {
        let mut src = filled(6,1,[255,0,0,255]);
        src.pixels_mut()[7] = 0;
        let mut dst = filled(6,1,[0,255,255,255]);
        blit(&mut dst,&src,[0,0],[6,1],255).unwrap();
        assert_eq!(pixel(&dst,0,0), &[254,0,0,255]); // mixed SIMD pair
        assert_eq!(pixel(&dst,1,0), &[0,255,255,255]);
        assert_eq!(pixel(&dst,2,0), &[255,0,0,255]); // scalar tail
        let src = filled(6,1,[255,0,0,255]);
        blit(&mut dst,&src,[0,0],[6,1],1000).unwrap();
        assert_eq!(pixel(&dst,0,0), &[255,0,0,255]); // both opaque copy
    }

    #[test]
    fn partial_opacity_packed_bgra_carry_and_row_alignment() {
        let src = filled(5,2,[255,0,0,2]);
        let mut dst = filled(5,2,[0,255,255,255]);
        blit(&mut dst,&src,[0,0],[5,2],127).unwrap();
        assert_eq!(pixel(&dst,0,0), &[0,255,255,255]); // SIMD factor floor(2*127/256)=0
        assert_eq!(pixel(&dst,4,0), &[0,254,254,254]); // scalar packed multiply factor=1
        assert_eq!(pixel(&dst,0,1), &[0,254,254,254]); // odd pitch scalar prefix
        assert_eq!(pixel(&dst,1,1), &[0,255,255,255]); // following SIMD pair
        assert_eq!(pixel(&dst,3,1), &[0,254,254,254]); // two-pixel tail
        let src = filled(1,1,[0,0,255,2]);
        let mut dst = filled(1,1,[255,255,0,255]);
        blit(&mut dst,&src,[0,0],[1,1],127).unwrap();
        assert_eq!(dst.pixels(), &[255,255,0,255]); // blue byte cannot cause the red-byte carry
    }

    #[test]
    fn interpolation_rounds_negative_differences_down_and_blends_alpha() {
        let src = filled(1,1,[1,255,0,128]);
        let mut dst = filled(1,1,[2,0,255,255]);
        blit(&mut dst,&src,[0,0],[1,1],255).unwrap();
        assert_eq!(dst.pixels(), &[1,127,127,191]);
    }
}
