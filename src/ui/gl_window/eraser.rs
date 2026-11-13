//! Draws the hole meshes with a blend that takes their alpha away from the window, so what's behind shows through.

use std::sync::{Arc, OnceLock};

use egui::epaint::{ClippedPrimitive, Mesh, Primitive};
use egui::{PaintCallback, Rect};
use glow::HasContext as _;

use crate::ui::erase::erases;

const FLOATS_PER_VERTEX: usize = 3;
const VERTEX: &str = "
IN vec2 a_pos;
IN float a_alpha;
uniform vec2 u_screen;
OUT float v_alpha;
void main() {
    v_alpha = a_alpha;
    gl_Position = vec4(2.0 * a_pos.x / u_screen.x - 1.0, 1.0 - 2.0 * a_pos.y / u_screen.y, 0.0, 1.0);
}";
const FRAGMENT: &str = "
IN float v_alpha;
void main() {
    COLOR = vec4(0.0, 0.0, 0.0, v_alpha);
}";

#[derive(Clone, Default)]
pub(super) struct Erasers(Arc<OnceLock<Option<Eraser>>>);

pub(super) fn through(primitives: Vec<ClippedPrimitive>, screen: Rect, erasers: &Erasers) -> Vec<ClippedPrimitive> {
    if !primitives.iter().any(|clipped| is_hole(&clipped.primitive)) {
        return primitives;
    }
    primitives
        .into_iter()
        .map(|clipped| if is_hole(&clipped.primitive) { cut(clipped, screen, erasers) } else { clipped })
        .collect()
}

fn is_hole(primitive: &Primitive) -> bool {
    matches!(primitive, Primitive::Mesh(mesh) if erases(mesh.texture_id))
}

fn cut(clipped: ClippedPrimitive, screen: Rect, erasers: &Erasers) -> ClippedPrimitive {
    let Primitive::Mesh(mesh) = clipped.primitive else { return clipped };
    let erasers = erasers.clone();
    let callback = egui_glow::CallbackFn::new(move |info, painter| {
        let gl = painter.gl();
        if let Some(eraser) = erasers.0.get_or_init(|| Eraser::new(gl)) {
            eraser.draw(gl, info.screen_size_px, info.pixels_per_point, &mesh);
        }
    });
    let primitive = Primitive::Callback(PaintCallback { rect: screen, callback: Arc::new(callback) });
    ClippedPrimitive { clip_rect: clipped.clip_rect, primitive }
}

struct Eraser {
    program: glow::Program,
    array: Option<glow::VertexArray>,
    vertices: glow::Buffer,
    indices: glow::Buffer,
    screen: Option<glow::UniformLocation>,
    position: u32,
    alpha: u32,
}

impl Eraser {
    fn new(gl: &glow::Context) -> Option<Self> {
        let modern = !gl.version().is_embedded && gl.version().major >= 3;
        let source = |text: &str, fragment: bool| {
            let (version, input, output) = match (modern, fragment) {
                (true, false) => ("#version 150", "in", "out"),
                (true, true) => ("#version 150\nout vec4 o_color;", "in", "out"),
                (false, false) => ("#version 100", "attribute", "varying"),
                (false, true) => ("#version 100\nprecision mediump float;", "varying", "varying"),
            };
            let color = if modern { "o_color" } else { "gl_FragColor" };
            format!("{version}\n{}", text.replace("IN", input).replace("OUT", output).replace("COLOR", color))
        };
        unsafe {
            let program = gl.create_program().ok()?;
            for (kind, text) in
                [(glow::VERTEX_SHADER, source(VERTEX, false)), (glow::FRAGMENT_SHADER, source(FRAGMENT, true))]
            {
                let shader = gl.create_shader(kind).ok()?;
                gl.shader_source(shader, &text);
                gl.compile_shader(shader);
                if !gl.get_shader_compile_status(shader) {
                    eprintln!("notchbase: glass shader: {}", gl.get_shader_info_log(shader));
                    return None;
                }
                gl.attach_shader(program, shader);
                gl.delete_shader(shader);
            }
            gl.link_program(program);
            if !gl.get_program_link_status(program) {
                eprintln!("notchbase: glass shader: {}", gl.get_program_info_log(program));
                return None;
            }
            Some(Self {
                program,
                array: gl.create_vertex_array().ok(),
                vertices: gl.create_buffer().ok()?,
                indices: gl.create_buffer().ok()?,
                screen: gl.get_uniform_location(program, "u_screen"),
                position: gl.get_attrib_location(program, "a_pos")?,
                alpha: gl.get_attrib_location(program, "a_alpha")?,
            })
        }
    }

    fn draw(&self, gl: &glow::Context, screen_px: [u32; 2], pixels_per_point: f32, mesh: &Mesh) {
        let mut data = Vec::with_capacity(mesh.vertices.len() * FLOATS_PER_VERTEX);
        for vertex in &mesh.vertices {
            let alpha = f32::from(vertex.color.a()) / 255.0;
            data.extend([vertex.pos.x * pixels_per_point, vertex.pos.y * pixels_per_point, alpha]);
        }
        let stride = (FLOATS_PER_VERTEX * size_of::<f32>()) as i32;
        unsafe {
            gl.use_program(Some(self.program));
            gl.uniform_2_f32(self.screen.as_ref(), screen_px[0] as f32, screen_px[1] as f32);
            gl.bind_vertex_array(self.array);
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vertices));
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytes(&data), glow::STREAM_DRAW);
            gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(self.indices));
            gl.buffer_data_u8_slice(glow::ELEMENT_ARRAY_BUFFER, bytes(&mesh.indices), glow::STREAM_DRAW);
            gl.enable_vertex_attrib_array(self.position);
            gl.vertex_attrib_pointer_f32(self.position, 2, glow::FLOAT, false, stride, 0);
            gl.enable_vertex_attrib_array(self.alpha);
            gl.vertex_attrib_pointer_f32(self.alpha, 1, glow::FLOAT, false, stride, 2 * size_of::<f32>() as i32);
            // Keeps what's there times one minus the hole's alpha, which is what cuts the hole.
            gl.blend_func_separate(glow::ZERO, glow::ONE_MINUS_SRC_ALPHA, glow::ZERO, glow::ONE_MINUS_SRC_ALPHA);
            gl.draw_elements(glow::TRIANGLES, mesh.indices.len() as i32, glow::UNSIGNED_INT, 0);
            // Without a vertex array the attributes are global state, so switch them off again for egui.
            if self.array.is_none() {
                gl.disable_vertex_attrib_array(self.position);
                gl.disable_vertex_attrib_array(self.alpha);
            }
            gl.bind_vertex_array(None);
        }
    }
}

fn bytes<T: Copy>(values: &[T]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(values.as_ptr().cast(), size_of_val(values)) }
}

#[cfg(test)]
mod tests {
    use egui::epaint::Vertex;
    use egui::{Shape, TextureId, pos2};

    use super::*;
    use crate::ui::erase::{TEXTURE, grid, hole, strength};

    fn triangle(texture_id: TextureId) -> ClippedPrimitive {
        let vertex = |x: f32, y: f32| Vertex { pos: pos2(x, y), uv: egui::epaint::WHITE_UV, color: strength(0.5) };
        let mesh = Mesh {
            indices: vec![0, 1, 2],
            vertices: vec![vertex(0.0, 0.0), vertex(4.0, 0.0), vertex(0.0, 4.0)],
            texture_id,
        };
        ClippedPrimitive { clip_rect: Rect::EVERYTHING, primitive: Primitive::Mesh(mesh) }
    }

    #[test]
    fn only_holes_become_callbacks() {
        let screen = Rect::from_min_size(pos2(0.0, 0.0), egui::vec2(10.0, 10.0));
        let plain = through(vec![triangle(TextureId::default())], screen, &Erasers::default());
        assert!(matches!(plain[0].primitive, Primitive::Mesh(_)));
        let cut = through(vec![triangle(TextureId::default()), triangle(TEXTURE)], screen, &Erasers::default());
        assert!(matches!(cut[0].primitive, Primitive::Mesh(_)));
        assert!(matches!(&cut[1].primitive, Primitive::Callback(callback) if callback.rect == screen));
        assert_eq!(strength(0.5).a(), 128);
        let fade = grid(screen, egui::vec2(5.0, 2.5), |at| at.y / 10.0);
        assert_eq!((fade.vertices.len(), fade.indices.len()), (3 * 5, 2 * 4 * 6));
        assert_eq!((fade.vertices[0].color.a(), fade.vertices[14].color.a()), (0, 255));
        assert_eq!(fade.vertices[14].pos, pos2(10.0, 10.0));
        assert!(matches!(hole(Mesh::default()), Shape::Mesh(mesh) if mesh.texture_id == TEXTURE));
    }
}
