//! The dock's black liquid: a signed distance field of continuous-corner boxes melted together with a smooth
//! minimum, filled solid black with the floating island's grey keyline. The corners are the notch's own
//! (`crates/paint/src/shape.rs`): a superellipse starting 1.32 radii from the edge, squarer when there's room. Everything that moves (the
//! body, the swell over magnified icons, droplets, necks, tooltips and menus) is one of these boxes, so they all
//! merge and pinch apart like one liquid.

use glow::HasContext as _;

use crate::gl::compile;

pub const MAX_BLOBS: usize = 96;

/// The body's top and bottom, in window points, which the glass gradient runs between.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Band {
    pub top: f32,
    pub bottom: f32,
    /// Where the pointer is, for the glass's glow and glint.
    pub pointer: Option<egui::Pos2>,
    /// How black the glass is at its top and its bottom.
    pub tint: [f32; 2],
    /// An open menu's or previews panel's top and bottom, in window points: it gets the same glass over its own height.
    pub panel: Option<(f32, f32)>,
}

/// A rounded box, in window points with the origin at the top left.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Blob {
    pub center: egui::Pos2,
    pub half: egui::Vec2,
    pub radius: f32,
    /// How far, in points, it melts into what was drawn before it. 0 is a hard union.
    pub melt: f32,
    /// A slice's slant: how far right of its top edge's middle its bottom edge's is (x), and how much longer its
    /// bottom edge's half is (y), in points. Zero for a box. A slice's `half` and `lean` are of its shape shrunk by
    /// `radius` all round, which then grows back by `radius` with round corners.
    pub lean: egui::Vec2,
}

impl Blob {
    pub fn rect(rect: egui::Rect, radius: f32, melt: f32) -> Self {
        Self { center: rect.center(), half: rect.size() * 0.5, radius, melt, lean: egui::Vec2::ZERO }
    }

    pub fn circle(center: egui::Pos2, radius: f32, melt: f32) -> Self {
        Self { center, half: egui::vec2(radius, radius), radius, melt, lean: egui::Vec2::ZERO }
    }

    /// The four-sided piece between two level edges, `top` and `bottom` high with these left and right ends: its
    /// sides run straight from one edge's ends to the other's, so slices stacked edge to edge make one smooth bent
    /// outline instead of steps. Its corners are rounded by `radius`, or less where it's too narrow for that.
    pub fn slice(top: f32, bottom: f32, top_x: egui::Rangef, bottom_x: egui::Rangef, radius: f32) -> Self {
        let height = (bottom - top).max(0.0);
        let mut radius = radius.clamp(0.0, height * 0.5);
        // Each side moves in square to itself, so further across the slower it climbs.
        let inward = |from: f32, to: f32, r: f32| {
            let slope = if height > 0.0 { (to - from) / height } else { 0.0 };
            (from + slope * r, to - slope * r, r * (1.0 + slope * slope).sqrt())
        };
        let shrunk = |r: f32| {
            let (left_top, left_bottom, left_in) = inward(top_x.min, bottom_x.min, r);
            let (right_top, right_bottom, right_in) = inward(top_x.max, bottom_x.max, r);
            (
                egui::Rangef::new(left_top + left_in, right_top - right_in),
                egui::Rangef::new(left_bottom + left_in, right_bottom - right_in),
            )
        };
        let (mut over, mut under) = shrunk(radius);
        while radius > 0.01 && (over.span() < 0.0 || under.span() < 0.0) {
            radius *= 0.7;
            (over, under) = shrunk(radius);
        }
        if over.span() < 0.0 || under.span() < 0.0 {
            radius = 0.0;
            (over, under) = (top_x, bottom_x);
        }
        let center = egui::pos2((over.center() + under.center()) * 0.5, (top + bottom) * 0.5);
        let half = egui::vec2((over.span() + under.span()) * 0.25, height * 0.5 - radius);
        let lean = egui::vec2(under.center() - over.center(), (under.span() - over.span()) * 0.5);
        // A lean of exactly zero would draw it as a box; a hair of one keeps it a slice.
        let lean = if lean == egui::Vec2::ZERO { egui::vec2(0.0, 1e-4) } else { lean };
        Self { center, half, radius, melt: 0.0, lean }
    }

    pub fn bounds(&self) -> egui::Rect {
        let half = if self.lean == egui::Vec2::ZERO {
            self.half
        } else {
            self.half + egui::vec2((self.lean.x.abs() + self.lean.y.abs()) * 0.5 + self.radius, self.radius)
        };
        egui::Rect::from_center_size(self.center, half * 2.0).expand(self.melt * 0.5)
    }
}

const VERTEX: &str = r#"
uniform vec4 u_bounds; // x0 y0 x1 y1 in pixels, origin bottom left
uniform vec2 u_size;
void main() {
    int c = gl_VertexID;
    vec2 corner = vec2((c == 1 || c == 3) ? u_bounds.z : u_bounds.x, (c >= 2) ? u_bounds.w : u_bounds.y);
    gl_Position = vec4(corner / u_size * 2.0 - 1.0, 0.0, 1.0);
}
"#;

const FRAGMENT: &str = r#"
uniform vec4 u_box[96];   // center xy, half size xy, in pixels, origin bottom left
uniform vec4 u_shape[96]; // corner extent, melt, in pixels; corner exponent
uniform vec2 u_lean[96];  // a slice's lean (Blob::lean), in pixels; zero for a box
uniform int u_count;
uniform float u_ppp;
uniform float u_alpha;
uniform float u_glass_shadow;
uniform vec3 u_pointer;   // pointer in pixels (origin bottom left), z 1 when it's over the window
uniform float u_thickness; // the thickness band's depth, in points
const float CLEAR = 0.94;
float near(float dist, float reach) {
    float away = dist / reach;
    return CLEAR * exp(-away * away);
}
uniform vec2 u_panel; // an open panel's top y and bottom y, in pixels, origin bottom left; equal when there is none
uniform vec4 u_glass; // body top y, bottom y (pixels, origin bottom left), black at the top and at the bottom; z < 0 is solid
uniform vec2 u_size;
// The blurred desktop picture drawn in place of the live blur (wallpaper.rs), covering the window, its first row the
// window's top; u_fake is how much of it shows, and at 1 the glass is opaque.
uniform sampler2D u_backdrop;
uniform float u_fake;

// A box with the notch's continuous (superellipse) corners: `e` is how far the corner reaches, `n` its exponent.
float box(vec2 p, vec2 h, float e, float n) {
    vec2 q = abs(p) - h + e;
    vec2 m = max(q, 0.0);
    float corner = e > 0.0 ? pow(pow(m.x, n) + pow(m.y, n), 1.0 / n) : length(m);
    return corner + min(max(q.x, q.y), 0.0) - e;
}

// One side of a polygon, from a to b, for its exact distance: the nearest squared distance so far, and the sign,
// which flips each time the side crosses the level line through p.
void side(vec2 p, vec2 a, vec2 b, inout float d, inout float s) {
    vec2 e = b - a;
    vec2 w = p - a;
    vec2 q = w - e * clamp(dot(w, e) / max(dot(e, e), 1e-6), 0.0, 1.0);
    d = min(d, dot(q, q));
    bool up = p.y >= a.y;
    bool below = p.y < b.y;
    bool left = e.x * w.y > e.y * w.x;
    if ((up && below && left) || (!up && !below && !left)) s = -s;
}

// A slice (Blob::slice) before its rounding: the top edge at +h.y and the bottom at -h.y, h.x their average half
// length, and `l` how far the bottom's middle sits right of the top's and how much longer its half is.
float slice(vec2 p, vec2 h, vec2 l) {
    vec2 tl = vec2(-0.5 * l.x - (h.x - 0.5 * l.y), h.y);
    vec2 tr = vec2(-0.5 * l.x + (h.x - 0.5 * l.y), h.y);
    vec2 br = vec2(0.5 * l.x + (h.x + 0.5 * l.y), -h.y);
    vec2 bl = vec2(0.5 * l.x - (h.x + 0.5 * l.y), -h.y);
    float d = dot(p - tl, p - tl);
    float s = 1.0;
    side(p, tl, bl, d, s);
    side(p, tr, tl, d, s);
    side(p, br, tr, d, s);
    side(p, bl, br, d, s);
    return s * sqrt(d);
}

float melt(float a, float b, float k) {
    if (k <= 0.0) return min(a, b);
    float h = max(k - abs(a - b), 0.0) / k;
    return min(a, b) - h * h * h * k / 6.0;
}

void main() {
    vec2 p = gl_FragCoord.xy;
    float d = 1e5;
    float shadow_d = 1e5;
    vec2 drop = vec2(0.0, 2.0 * u_ppp);
    for (int i = 0; i < 96; i++) {
        if (i >= u_count) break;
        vec4 b = u_box[i];
        vec4 s = u_shape[i];
        vec2 l = u_lean[i];
        bool sliced = l.x != 0.0 || l.y != 0.0;
        float here = sliced ? slice(p - b.xy, b.zw, l) - s.x : box(p - b.xy, b.zw, s.x, s.z);
        d = melt(d, here, s.y);
        if (u_glass_shadow > 0.0) {
            float under = sliced ? slice(p + drop - b.xy, b.zw, l) - s.x : box(p + drop - b.xy, b.zw, s.x, s.z);
            shadow_d = melt(shadow_d, under, s.y);
        }
    }

    float keyline = 1.6 * u_ppp;
    float core = clamp(0.5 - d, 0.0, 1.0);
    float outer = clamp(0.5 - (d - keyline), 0.0, 1.0);
    if (outer <= 0.0 && u_glass_shadow <= 0.0) discard;

    // The island's rim: a 1.6 point grey 29 keyline just outside the shape (src/notch/keyline.rs).
    vec3 line = vec3(29.0 / 255.0);
    if (u_glass.z < 0.0) {
        frag_color = vec4(line * (outer - core), outer) * u_alpha;
        return;
    }

    // Glass, like the bottom of the open notch (src/notch/frame.rs): black down to FADE_FROM of the body's height,
    // then thinning out toward the bottom over the blurred desktop.
    float y = gl_FragCoord.y;
    float from = mix(u_glass.x, u_glass.y, 0.32);
    float black = mix(u_glass.z, u_glass.w, smoothstep(from, u_glass.y, y));
    // Menus and previews above the dock are the same glass, the same gradient run over their own height. It starts
    // above the hovered icon's rise, so the rise keeps the dock's own.
    if (u_panel.x > u_panel.y) {
        float panel_from = mix(u_panel.x, u_panel.y, 0.32);
        float panel = mix(u_glass.z, u_glass.w, smoothstep(panel_from, u_panel.y, y));
        black = mix(black, panel, smoothstep(u_glass.x + 14.0 * u_ppp, u_glass.x + 22.0 * u_ppp, y));
    }
    float t = clamp((u_glass.x - y) / max(u_glass.x - u_glass.y, 1.0), 0.0, 1.0);

    // Deep, near-black glass over the blur: no frost, no bevel. One fine highlight along the top edge, strongest
    // where the edge faces straight up and gone by the rounded ends, and the island's keyline outside.
    vec2 g = vec2(dFdx(d), dFdy(d));
    vec2 n = length(g) > 1e-4 ? normalize(g) : vec2(0.0, 1.0);
    float depth = -d / u_ppp;
    vec3 tint = vec3(0.0);
    vec3 rgb = tint * black;
    float a = black;
    if (u_fake > 0.0) {
        vec3 seen = TEX(u_backdrop, vec2(p.x / u_size.x, 1.0 - p.y / u_size.y)).rgb;
        rgb = mix(rgb, seen * (1.0 - black), u_fake);
        a = mix(a, 1.0, u_fake);
    }
    // Smoked glass: a faint 1pt highlight just inside the top edge, gone by the rounded ends, and a barely-there
    // lightening all round the inside of the edge, so it reads as a slab of glass rather than a flat tint.
    float inside = step(-0.5, depth);
    float spec = 0.10 * pow(max(n.y, 0.0), 8.0) * clamp(1.0 - abs(depth - 0.9) / 0.6, 0.0, 1.0) * inside;
    float rim = 0.035 * clamp(1.0 - depth / 3.0, 0.0, 1.0) * inside;
    float light = clamp(spec + rim, 0.0, 1.0);
    rgb = vec3(light) + rgb * (1.0 - light);
    a = light + a * (1.0 - light);

    rgb *= core;
    a *= core;
    // The island keyline still sits just outside, faded with the body as on the island (src/notch/keyline.rs).
    rgb += line * black * (outer - core);
    a += black * (outer - core);

    // The glass's own soft shadow (offset 2, blur 10, 0.3 when seen through), outside the shape only.
    float shade = 0.3 * CLEAR * u_glass_shadow * (1.0 - smoothstep(-5.0 * u_ppp, 5.0 * u_ppp, shadow_d)) * (1.0 - outer);
    a += shade * (1.0 - a);
    frag_color = vec4(rgb, a) * u_alpha;
}
"#;

/// The notch's corner (`continuous_corner` in `crates/paint/src/shape.rs`): it reaches 1.32 radii from the edge and
/// gets squarer when there's room. Returns how far it reaches and its superellipse exponent.
fn continuous_corner(radius: f32, available: f32) -> (f32, f32) {
    const REACH: f32 = 1.32;
    let extent = (radius * REACH).clamp(0.0, available);
    let slack = if available > 0.0 { (available - extent) / available } else { 0.0 };
    let t = (slack / 0.3).clamp(0.0, 1.0);
    (extent, 2.0 + 1.1 * t * t * (3.0 - 2.0 * t))
}

pub struct Liquid {
    program: glow::Program,
    vao: glow::VertexArray,
}

impl Liquid {
    pub fn new(gl: &glow::Context, version: egui_glow::ShaderVersion) -> Self {
        let program = compile(gl, version, VERTEX, FRAGMENT);
        let vao = unsafe { gl.create_vertex_array() }.expect("a vertex array");
        Self { program, vao }
    }

    /// Draws `blobs` (window points, top left origin) into a target of `size` pixels: as glass over the blur when
    /// `glass` gives the body's band, else solid black. `fake` is the blurred desktop picture and how much of it
    /// stands in for the live blur.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &self,
        gl: &glow::Context,
        size: [u32; 2],
        ppp: f32,
        blobs: &[Blob],
        alpha: f32,
        glass: Option<Band>,
        fake: Option<(glow::Texture, f32)>,
    ) {
        let blobs = &blobs[..blobs.len().min(MAX_BLOBS)];
        let Some(bounds) = blobs.iter().map(Blob::bounds).reduce(|a, b| a.union(b)) else { return };
        let height = size[1] as f32;
        let to_px = |p: egui::Pos2| egui::pos2(p.x * ppp, height - p.y * ppp);
        let lo = to_px(bounds.left_bottom());
        let hi = to_px(bounds.right_top());
        // Room for the shadow when there's glass.
        let pad = if glass.is_some() { 16.0 * ppp } else { 4.0 * ppp };
        let mut boxes = Vec::with_capacity(blobs.len() * 4);
        let mut shapes = Vec::with_capacity(blobs.len() * 4);
        let mut leans = Vec::with_capacity(blobs.len() * 2);
        for blob in blobs {
            leans.extend_from_slice(&[blob.lean.x * ppp, blob.lean.y * ppp]);
            let c = to_px(blob.center);
            boxes.extend_from_slice(&[c.x, c.y, blob.half.x.max(0.0) * ppp, blob.half.y.max(0.0) * ppp]);
            let (extent, exponent) = if blob.lean == egui::Vec2::ZERO {
                continuous_corner(blob.radius.max(0.0), blob.half.x.min(blob.half.y).max(0.0))
            } else {
                (blob.radius.max(0.0), 2.0)
            };
            shapes.extend_from_slice(&[extent * ppp, blob.melt.max(0.0) * ppp, exponent, 0.0]);
        }
        unsafe {
            gl.use_program(Some(self.program));
            gl.bind_vertex_array(Some(self.vao));
            gl.disable(glow::SCISSOR_TEST);
            gl.disable(glow::CULL_FACE);
            gl.enable(glow::BLEND);
            gl.blend_func_separate(glow::ONE, glow::ONE_MINUS_SRC_ALPHA, glow::ONE_MINUS_DST_ALPHA, glow::ONE);
            let at = |name: &str| gl.get_uniform_location(self.program, name);
            gl.uniform_4_f32(at("u_bounds").as_ref(), lo.x - pad, lo.y - pad, hi.x + pad, hi.y + pad);
            gl.uniform_2_f32(at("u_size").as_ref(), size[0] as f32, size[1] as f32);
            gl.uniform_4_f32_slice(at("u_box").as_ref(), &boxes);
            gl.uniform_4_f32_slice(at("u_shape").as_ref(), &shapes);
            gl.uniform_2_f32_slice(at("u_lean").as_ref(), &leans);
            gl.uniform_1_i32(at("u_count").as_ref(), blobs.len() as i32);
            gl.uniform_1_f32(at("u_ppp").as_ref(), ppp);
            gl.uniform_1_f32(at("u_alpha").as_ref(), alpha);
            gl.uniform_1_f32(at("u_glass_shadow").as_ref(), if glass.is_some() { 1.0 } else { 0.0 });
            let pointer = glass.and_then(|b| b.pointer);
            let (px, py) = pointer.map_or((0.0, 0.0), |p| (p.x * ppp, height - p.y * ppp));
            gl.uniform_3_f32(at("u_pointer").as_ref(), px, py, if pointer.is_some() { 1.0 } else { 0.0 });
            let body_h = glass.map_or(40.0, |b| b.bottom - b.top);
            // glass.rs: (radius * 0.5).clamp(3, 6), with the body's radius.
            gl.uniform_1_f32(at("u_thickness").as_ref(), (body_h * 0.19).clamp(3.0, 6.0));
            let (panel_top, panel_bottom) = glass.and_then(|b| b.panel).unwrap_or((0.0, 0.0));
            gl.uniform_2_f32(at("u_panel").as_ref(), height - panel_top * ppp, height - panel_bottom * ppp);
            match glass {
                Some(band) => gl.uniform_4_f32(
                    at("u_glass").as_ref(),
                    height - band.top * ppp,
                    height - band.bottom * ppp,
                    band.tint[0].clamp(0.0, 1.0),
                    band.tint[1].clamp(0.0, 1.0),
                ),
                None => gl.uniform_4_f32(at("u_glass").as_ref(), 0.0, 0.0, -1.0, -1.0),
            }
            let (texture, amount) = fake.filter(|_| glass.is_some()).map_or((None, 0.0), |(t, a)| (Some(t), a));
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, texture);
            gl.uniform_1_i32(at("u_backdrop").as_ref(), 0);
            gl.uniform_1_f32(at("u_fake").as_ref(), amount.clamp(0.0, 1.0));
            gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
            gl.bind_texture(glow::TEXTURE_2D, None);
            gl.bind_vertex_array(None);
            gl.use_program(None);
        }
    }
}

/// Puts the blurred desktop picture on the GPU, in place of `old`. Linear filtering does the rest of the smoothing.
pub fn upload(
    gl: &glow::Context,
    old: Option<glow::Texture>,
    picture: &crate::wallpaper::Picture,
) -> Option<glow::Texture> {
    let [w, h] = picture.size;
    unsafe {
        if let Some(old) = old {
            gl.delete_texture(old);
        }
        let texture = gl.create_texture().ok()?;
        gl.bind_texture(glow::TEXTURE_2D, Some(texture));
        for (key, value) in [
            (glow::TEXTURE_MIN_FILTER, glow::LINEAR),
            (glow::TEXTURE_MAG_FILTER, glow::LINEAR),
            (glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE),
            (glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE),
        ] {
            gl.tex_parameter_i32(glow::TEXTURE_2D, key, value as i32);
        }
        gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            glow::RGBA8 as i32,
            w as i32,
            h as i32,
            0,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelUnpackData::Slice(Some(&picture.rgba)),
        );
        gl.bind_texture(glow::TEXTURE_2D, None);
        Some(texture)
    }
}

/// The shader's distance field on the CPU, for tests: negative inside the liquid, in points.
/// The signed distance to the liquid's outline at `p`, in points: the same shape the shader draws. Negative inside.
pub fn distance(blobs: &[Blob], p: egui::Pos2) -> f32 {
    let mut d = 1e5_f32;
    for b in blobs {
        let here = if b.lean == egui::Vec2::ZERO { box_distance(b, p) } else { slice_distance(b, p) };
        d = melt(d, here, b.melt);
    }
    d
}

fn melt(a: f32, b: f32, k: f32) -> f32 {
    if k <= 0.0 {
        return a.min(b);
    }
    let h = (k - (a - b).abs()).max(0.0) / k;
    a.min(b) - h * h * h * k / 6.0
}

fn box_distance(b: &Blob, p: egui::Pos2) -> f32 {
    let (e, n) = continuous_corner(b.radius.max(0.0), b.half.x.min(b.half.y).max(0.0));
    let q = egui::vec2((p.x - b.center.x).abs(), (p.y - b.center.y).abs()) - b.half + egui::vec2(e, e);
    let m = egui::vec2(q.x.max(0.0), q.y.max(0.0));
    let corner = if e > 0.0 { (m.x.powf(n) + m.y.powf(n)).powf(1.0 / n) } else { m.length() };
    corner + q.x.max(q.y).min(0.0) - e
}

/// The shader's `slice`, rounded, in window points (y down, so the top edge is at -half.y).
fn slice_distance(b: &Blob, p: egui::Pos2) -> f32 {
    let (h, l) = (b.half, b.lean);
    let corner = |x: f32, y: f32| egui::pos2(x, y);
    let tl = corner(-0.5 * l.x - (h.x - 0.5 * l.y), -h.y);
    let tr = corner(-0.5 * l.x + (h.x - 0.5 * l.y), -h.y);
    let br = corner(0.5 * l.x + (h.x + 0.5 * l.y), h.y);
    let bl = corner(0.5 * l.x - (h.x + 0.5 * l.y), h.y);
    let p = (p - b.center).to_pos2();
    let mut d = (p - tl).length_sq();
    let mut sign = 1.0;
    for (a, z) in [(tl, bl), (tr, tl), (br, tr), (bl, br)] {
        let (e, w) = (z - a, p - a);
        let q = w - e * (w.dot(e) / e.length_sq().max(1e-6)).clamp(0.0, 1.0);
        d = d.min(q.length_sq());
        let (up, below, left) = (p.y >= a.y, p.y < z.y, e.x * w.y > e.y * w.x);
        if (up && below && left) || (!up && !below && !left) {
            sign = -sign;
        }
    }
    sign * d.sqrt() - b.radius
}
