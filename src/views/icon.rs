type Rgb = [f32; 3];

const SIZE: usize = 256;

struct Canvas {
    px: Vec<f32>,
}

fn rr_sdf(x: f32, y: f32, x0: f32, y0: f32, x1: f32, y1: f32, r: f32) -> f32 {
    let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    let (hx, hy) = ((x1 - x0) / 2.0 - r, (y1 - y0) / 2.0 - r);
    let qx = (x - cx).abs() - hx;
    let qy = (y - cy).abs() - hy;
    qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - r
}

impl Canvas {
    fn new() -> Self {
        Self {
            px: vec![0.0; SIZE * SIZE * 4],
        }
    }

    // paint `color` over pixels where `shape(x, y)` (unit coords) is inside; `clip` masks it
    fn paint(
        &mut self,
        color: impl Fn(f32, f32) -> Rgb,
        shape: impl Fn(f32, f32) -> f32,
        clip: impl Fn(f32, f32) -> f32,
    ) {
        // artwork occupies the central 80% so it matches the macOS icon grid
        const PAD: f32 = 0.1;
        let scale = 1.0 - 2.0 * PAD;
        let aa = 1.0 / (SIZE as f32 * scale);
        for j in 0..SIZE {
            for i in 0..SIZE {
                let (x, y) = (
                    ((i as f32 + 0.5) / SIZE as f32 - PAD) / scale,
                    ((j as f32 + 0.5) / SIZE as f32 - PAD) / scale,
                );
                let cov = (0.5 - shape(x, y) / aa).clamp(0.0, 1.0)
                    * (0.5 - clip(x, y) / aa).clamp(0.0, 1.0);
                if cov <= 0.0 {
                    continue;
                }
                let c = color(x, y);
                let o = (j * SIZE + i) * 4;
                let a0 = self.px[o + 3];
                let a = cov + a0 * (1.0 - cov);
                for k in 0..3 {
                    self.px[o + k] = (c[k] * cov + self.px[o + k] * a0 * (1.0 - cov)) / a;
                }
                self.px[o + 3] = a;
            }
        }
    }
}

fn lerp(a: Rgb, b: Rgb, t: f32) -> Rgb {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

const NAVY: Rgb = [0.09, 0.17, 0.42];
const NAVY_DARK: Rgb = [0.04, 0.08, 0.24];
const NAVY_LIGHT: Rgb = [0.27, 0.42, 0.78];
const NAVY_OUTLINE: Rgb = [0.02, 0.04, 0.14];

pub fn rgba() -> Vec<u8> {
    let mut c = Canvas::new();
    let none = |_: f32, _: f32| -1.0;
    let bg = |x: f32, y: f32| rr_sdf(x, y, 0.0, 0.0, 1.0, 1.0, 0.225);
    let box_ = |x0: f32, y0: f32, x1: f32, y1: f32| {
        move |x: f32, y: f32| (x0 - x).max(x - x1).max(y0 - y).max(y - y1)
    };

    c.paint(
        |_, y| lerp([0.86, 0.92, 1.0], [0.62, 0.74, 0.96], y),
        bg,
        none,
    );

    // flat pipe like the SMB original: wide short rim over a slightly narrower tall body
    let (rim, body) = (box_(0.2, 0.2, 0.8, 0.4), box_(0.235, 0.4, 0.765, 1.0));
    let shade = |l: f32, r: f32, x: f32| {
        let t = (x - l) / (r - l);
        if t < 0.1 {
            NAVY_LIGHT
        } else if t < 0.2 {
            lerp(NAVY_LIGHT, [0.7, 0.8, 1.0], 0.55)
        } else if t < 0.3 {
            NAVY_LIGHT
        } else if t < 0.72 {
            NAVY
        } else if t < 0.86 {
            lerp(NAVY, NAVY_DARK, 0.5)
        } else {
            NAVY_DARK
        }
    };

    for (shape, l, r, edge) in [
        (&body as &dyn Fn(f32, f32) -> f32, 0.235, 0.765, 0.012),
        (&rim, 0.2, 0.8, 0.014),
    ] {
        c.paint(|_, _| NAVY_OUTLINE, |x, y| shape(x, y) - edge, bg);
        c.paint(|x, _| shade(l, r, x), shape, bg);
    }
    // keep the rim's outline over the body
    c.paint(
        |_, _| NAVY_OUTLINE,
        |x, y| (0.4 - y).max(y - 0.414).max(0.2 - x).max(x - 0.8),
        bg,
    );

    c.px.iter()
        .map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
        .collect()
}

pub fn data() -> eframe::egui::IconData {
    eframe::egui::IconData {
        rgba: rgba(),
        width: SIZE as u32,
        height: SIZE as u32,
    }
}
