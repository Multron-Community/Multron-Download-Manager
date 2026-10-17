use std::f32::consts::TAU;

pub const FRAMES: usize = 12;

pub fn accent(id: &str) -> [u8; 3] {
    match id {
        "cyan" => [0x4f, 0xc3, 0xd1],
        "green" => [0x2e, 0xb8, 0x5c],
        "orange" => [0xff, 0x8c, 0x1a],
        "purple" => [0x8e, 0x5c, 0xe6],
        "red" => [0xe5, 0x39, 0x35],
        "pink" => [0xe8, 0x4a, 0x9b],
        "teal" => [0x14, 0xb8, 0xa6],
        "gray" => [0x80, 0x80, 0x80],
        _ => [0x2f, 0x8c, 0xf0],
    }
}

pub fn badge(size: u32, frame: usize, color: [u8; 3]) -> Vec<u8> {
    let s = size as f32;
    let c = s / 2.0;
    let outer = s / 2.0 - 0.5;
    let ring = outer * 0.6;
    let thickness = (outer * 0.26).max(1.6);
    let start = frame as f32 / FRAMES as f32 * TAU;
    let span = TAU * 0.72;
    const SUB: usize = 4;
    let mut out = vec![0u8; (size * size * 4) as usize];
    for y in 0..size {
        for x in 0..size {
            let (mut disc, mut arc_r, mut arc_g, mut arc_b, mut arc_a) =
                (0.0f32, 0.0, 0.0, 0.0, 0.0);
            for sy in 0..SUB {
                for sx in 0..SUB {
                    let px = x as f32 + (sx as f32 + 0.5) / SUB as f32 - c;
                    let py = y as f32 + (sy as f32 + 0.5) / SUB as f32 - c;
                    let d = (px * px + py * py).sqrt();
                    if d <= outer {
                        disc += 1.0;
                    }
                    if (d - ring).abs() <= thickness / 2.0 {
                        let angle = py.atan2(px).rem_euclid(TAU);
                        let behind = (start - angle).rem_euclid(TAU);
                        let strength = if behind <= span {
                            1.0 - behind / span * 0.9
                        } else {
                            0.18
                        };
                        let (r, g, b) = if behind <= span {
                            (color[0] as f32, color[1] as f32, color[2] as f32)
                        } else {
                            (255.0, 255.0, 255.0)
                        };
                        arc_r += r * strength;
                        arc_g += g * strength;
                        arc_b += b * strength;
                        arc_a += strength;
                    }
                }
            }
            let n = (SUB * SUB) as f32;
            let disc_a = disc / n * 0.92;
            let a = (arc_a / n).min(1.0);
            let (fr, fg, fb) = if arc_a > 0.0 {
                (arc_r / arc_a, arc_g / arc_a, arc_b / arc_a)
            } else {
                (0.0, 0.0, 0.0)
            };
            let bg = [22.0f32, 22.0, 26.0];
            let out_a = a + disc_a * (1.0 - a);
            let blend = |f: f32, b: f32| {
                if out_a <= 0.0 {
                    0.0
                } else {
                    (f * a + b * disc_a * (1.0 - a)) / out_a
                }
            };
            let i = ((y * size + x) * 4) as usize;
            out[i] = blend(fr, bg[0]).round().clamp(0.0, 255.0) as u8;
            out[i + 1] = blend(fg, bg[1]).round().clamp(0.0, 255.0) as u8;
            out[i + 2] = blend(fb, bg[2]).round().clamp(0.0, 255.0) as u8;
            out[i + 3] = (out_a * 255.0).round().clamp(0.0, 255.0) as u8;
        }
    }
    out
}

pub fn downscale(rgba: &[u8], width: u32, height: u32, to: u32) -> Vec<u8> {
    let mut out = vec![0u8; (to * to * 4) as usize];
    if width == 0 || height == 0 {
        return out;
    }
    for y in 0..to {
        for x in 0..to {
            let x0 = x * width / to;
            let x1 = ((x + 1) * width / to).max(x0 + 1).min(width);
            let y0 = y * height / to;
            let y1 = ((y + 1) * height / to).max(y0 + 1).min(height);
            let mut acc = [0f32; 4];
            let mut count = 0f32;
            for sy in y0..y1 {
                for sx in x0..x1 {
                    let i = ((sy * width + sx) * 4) as usize;
                    let a = rgba[i + 3] as f32 / 255.0;
                    acc[0] += rgba[i] as f32 * a;
                    acc[1] += rgba[i + 1] as f32 * a;
                    acc[2] += rgba[i + 2] as f32 * a;
                    acc[3] += a;
                    count += 1.0;
                }
            }
            let o = ((y * to + x) * 4) as usize;
            if acc[3] > 0.0 {
                out[o] = (acc[0] / acc[3]).round() as u8;
                out[o + 1] = (acc[1] / acc[3]).round() as u8;
                out[o + 2] = (acc[2] / acc[3]).round() as u8;
            }
            out[o + 3] = (acc[3] / count * 255.0).round() as u8;
        }
    }
    out
}

pub fn overlay_bottom_right(base: &[u8], size: u32, badge: &[u8], badge_size: u32) -> Vec<u8> {
    let mut out = base.to_vec();
    let off = size - badge_size;
    for y in 0..badge_size {
        for x in 0..badge_size {
            let b = ((y * badge_size + x) * 4) as usize;
            let o = (((y + off) * size + (x + off)) * 4) as usize;
            let sa = badge[b + 3] as f32 / 255.0;
            if sa <= 0.0 {
                continue;
            }
            let da = out[o + 3] as f32 / 255.0;
            let oa = sa + da * (1.0 - sa);
            for k in 0..3 {
                let v = (badge[b + k] as f32 * sa + out[o + k] as f32 * da * (1.0 - sa)) / oa;
                out[o + k] = v.round().clamp(0.0, 255.0) as u8;
            }
            out[o + 3] = (oa * 255.0).round() as u8;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_rotate_and_stay_inside_the_badge() {
        let a = badge(32, 0, accent("blue"));
        let b = badge(32, 3, accent("blue"));
        assert_eq!(a.len(), 32 * 32 * 4);
        assert_ne!(a, b);
        assert_eq!(a[3], 0);
        assert!(a[((16 * 32 + 16) * 4 + 3) as usize] > 200);
    }

    #[test]
    fn badge_lands_in_the_bottom_right_corner() {
        let base = vec![255u8; 32 * 32 * 4];
        let badge = badge(16, 0, accent("blue"));
        let out = overlay_bottom_right(&base, 32, &badge, 16);
        assert_eq!(&out[..4], &[255, 255, 255, 255]);
        let center = (((24 * 32) + 24) * 4) as usize;
        assert!(out[center] < 120);
    }

    #[test]
    fn downscales_keeping_size() {
        let src = vec![200u8; 64 * 64 * 4];
        let out = downscale(&src, 64, 64, 32);
        assert_eq!(out.len(), 32 * 32 * 4);
        assert_eq!(out[0], 200);
    }
}
