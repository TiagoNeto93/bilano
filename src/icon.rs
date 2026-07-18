//! Procedural app icon, rendered in code so the binary stays self-contained.
//! A rounded square with a blue→green (chat→game) gradient and a white dial
//! knob + pointer. Rendered at 4× and box-downsampled for clean anti-aliasing.

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Rounded-rect signed coverage: distance outside the rounded rect (0 = inside).
fn round_rect_outside(px: f32, py: f32, w: f32, h: f32, r: f32) -> f32 {
    let dx = (r - px).max(px - (w - r)).max(0.0);
    let dy = (r - py).max(py - (h - r)).max(0.0);
    (dx * dx + dy * dy).sqrt() - r
}

/// Return `size`×`size` RGBA8 (unmultiplied), transparent outside the icon.
pub fn rgba(size: u32) -> Vec<u8> {
    let ss = 4u32;
    let n = size * ss;
    let fnf = n as f32;
    let mut hi = vec![0u8; (n * n * 4) as usize];

    let radius = fnf * 0.24;
    let cx = fnf * 0.5;
    let cy = fnf * 0.52;
    let knob_r = fnf * 0.28;
    let ring_w = fnf * 0.085;

    let blue = (74.0, 137.0, 255.0);
    let green = (52.0, 205.0, 130.0);
    let white = (246.0, 248.0, 252.0);

    for y in 0..n {
        for x in 0..n {
            let fx = x as f32 + 0.5;
            let fy = y as f32 + 0.5;

            // Base gradient (chat -> game), with a slight vertical shade.
            let t = fx / fnf;
            let m = smoothstep(0.40, 0.60, t);
            let shade = 1.0 - 0.16 * (fy / fnf);
            let mut r = lerp(blue.0, green.0, m) * shade;
            let mut g = lerp(blue.1, green.1, m) * shade;
            let mut b = lerp(blue.2, green.2, m) * shade;

            // Dial ring.
            let d = ((fx - cx).powi(2) + (fy - cy).powi(2)).sqrt();
            let ring = 1.0
                - (smoothstep(knob_r - ring_w - 1.5, knob_r - ring_w, d)
                    * (1.0 - smoothstep(knob_r, knob_r + 1.5, d)));
            if ring < 1.0 {
                let k = 1.0 - ring;
                r = lerp(r, white.0, k);
                g = lerp(g, white.1, k);
                b = lerp(b, white.2, k);
            }

            // Pointer notch from center toward the top of the dial.
            let notch_w = ring_w * 0.6;
            if fx > cx - notch_w && fx < cx + notch_w && fy < cy && fy > cy - knob_r + ring_w * 0.4 {
                r = white.0;
                g = white.1;
                b = white.2;
            }

            // Anti-aliased rounded-rect alpha.
            let outside = round_rect_outside(fx, fy, fnf, fnf, radius);
            let alpha = (1.0 - smoothstep(-1.5, 1.5, outside)).clamp(0.0, 1.0);

            let i = ((y * n + x) * 4) as usize;
            hi[i] = r as u8;
            hi[i + 1] = g as u8;
            hi[i + 2] = b as u8;
            hi[i + 3] = (alpha * 255.0) as u8;
        }
    }

    // Box downsample ss×ss -> size×size.
    let mut out = vec![0u8; (size * size * 4) as usize];
    let cnt = (ss * ss) as u32;
    for y in 0..size {
        for x in 0..size {
            let (mut r, mut g, mut b, mut a) = (0u32, 0u32, 0u32, 0u32);
            for dy in 0..ss {
                for dx in 0..ss {
                    let sx = x * ss + dx;
                    let sy = y * ss + dy;
                    let i = ((sy * n + sx) * 4) as usize;
                    r += hi[i] as u32;
                    g += hi[i + 1] as u32;
                    b += hi[i + 2] as u32;
                    a += hi[i + 3] as u32;
                }
            }
            let o = ((y * size + x) * 4) as usize;
            out[o] = (r / cnt) as u8;
            out[o + 1] = (g / cnt) as u8;
            out[o + 2] = (b / cnt) as u8;
            out[o + 3] = (a / cnt) as u8;
        }
    }
    out
}
