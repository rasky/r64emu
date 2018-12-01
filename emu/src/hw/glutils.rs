extern crate gl;

use self::gl::types::*;
use super::super::gfx::{ColorFormat, GfxBufferLE, Rgb888, Rgba8888};
use std::ffi;

fn return_param<T, F>(f: F) -> T
where
    F: FnOnce(&mut T),
{
    let mut val = unsafe { ::std::mem::uninitialized() };
    f(&mut val);
    val
}

pub trait ColorForTexture: ColorFormat {
    fn gl_enum() -> GLenum;
    fn byte_size() -> i32;
}

impl ColorForTexture for Rgb888 {
    fn gl_enum() -> GLenum {
        gl::RGBA
    }
    fn byte_size() -> i32 {
        return 3;
    }
}

impl ColorForTexture for Rgba8888 {
    fn gl_enum() -> GLenum {
        gl::RGBA
    }
    fn byte_size() -> i32 {
        return 4;
    }
}

struct Texture {
    id: GLuint,
}

impl Texture {
    unsafe fn new() -> Self {
        let id = return_param(|x| gl::GenTextures(1, x as *mut u32));
        Self { id }
    }

    unsafe fn copy_from_buffer<CF: ColorForTexture>(&self, buffer: &GfxBufferLE<CF>) {
        gl::BindTexture(gl::TEXTURE_2D, self.id);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::LINEAR as i32);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::LINEAR as i32);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as i32);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as i32);

        let width = buffer.width() as i32;
        let height = buffer.height() as i32;
        let (pixels, _pitch) = buffer.raw();
        gl::TexImage2D(
            gl::TEXTURE_2D,
            0,
            gl::RGBA as i32,
            width,
            height,
            0,
            CF::gl_enum(),
            gl::UNSIGNED_BYTE,
            pixels.as_ptr() as *mut ffi::c_void,
        );
    }
}

impl Drop for Texture {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteTextures(1, &self.id);
        }
    }
}

struct VertexBuffer {
    id: GLuint,
}

impl VertexBuffer {
    unsafe fn new() -> Self {
        let id = return_param(|x| gl::GenBuffers(1, x as *mut u32));
        Self { id }
    }
}

impl Drop for VertexBuffer {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteBuffers(1, &self.id);
        }
    }
}

struct VertexArray {
    id: GLuint,
}

impl VertexArray {
    unsafe fn new() -> Self {
        let id = return_param(|x| gl::GenVertexArrays(1, x as *mut u32));
        Self { id }
    }
}

impl Drop for VertexArray {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteVertexArrays(1, &self.id);
        }
    }
}

struct Program {
    id: GLuint,
}

impl Program {
    unsafe fn new() -> Self {
        let id = gl::CreateProgram();
        Self { id }
    }
}

impl Drop for Program {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteProgram(self.id);
        }
    }
}

pub struct SurfaceRenderer {
    vao: VertexArray,
    vbo_pos: VertexBuffer,
    vbo_tex: VertexBuffer,
    program: Program,
    tex: Texture,

    // Backend storage for vertex buffers (must be heap allocated)
    pos_data: Vec<GLfloat>,
    tex_data: Vec<GLfloat>,
}

impl SurfaceRenderer {
    pub fn new<F>(load_fn: F) -> Self
    where
        F: FnMut(&'static str) -> *const ::std::os::raw::c_void,
    {
        unsafe {
            gl::load_with(load_fn);
            let vert_source = b"
                #version 150
                in vec2 a_position;
                in vec2 a_texcoord;
                out vec2 v_texcoord;
                void main() {
                    gl_Position = vec4(a_position * 2.0 - 1.0, 0.0, 1.0);
                    v_texcoord = a_texcoord;
                }
            \0";

            let frag_source = b"
                #version 150
                uniform sampler2D u_texture;
                in vec2 v_texcoord;
                out vec4 v_fragcolor;
                void main() {
                    v_fragcolor = texture(u_texture, v_texcoord);
                }
            \0";

            let program = Program::new();
            let vert_shader = gl::CreateShader(gl::VERTEX_SHADER);
            let frag_shader = gl::CreateShader(gl::FRAGMENT_SHADER);
            gl::ShaderSource(
                vert_shader,
                1,
                &(vert_source.as_ptr() as *const GLchar),
                &(vert_source.len() as GLint),
            );
            gl::ShaderSource(
                frag_shader,
                1,
                &(frag_source.as_ptr() as *const GLchar),
                &(frag_source.len() as GLint),
            );
            gl::CompileShader(vert_shader);
            gl::CompileShader(frag_shader);
            gl::AttachShader(program.id, vert_shader);
            gl::AttachShader(program.id, frag_shader);
            gl::LinkProgram(program.id);
            gl::DeleteShader(vert_shader);
            gl::DeleteShader(frag_shader);

            let loc_u_texture =
                gl::GetUniformLocation(program.id, b"u_texture\0".as_ptr() as _) as u32;
            let loc_a_position =
                gl::GetAttribLocation(program.id, b"a_position\0".as_ptr() as _) as u32;
            let loc_a_texcoord =
                gl::GetAttribLocation(program.id, b"a_texcoord\0".as_ptr() as _) as u32;

            gl::UseProgram(program.id);
            gl::Uniform1i(loc_u_texture as i32, 0);

            let vao = VertexArray::new();
            gl::BindVertexArray(vao.id);

            let vbo_pos = VertexBuffer::new();
            let vbo_tex = VertexBuffer::new();
            let pos_data = vec![0.0, 1.0, 1.0, 1.0, 0.0, 0.0, 1.0, 0.0];
            let tex_data = vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 1.0];

            gl::BindBuffer(gl::ARRAY_BUFFER, vbo_pos.id);
            gl::BufferData(
                gl::ARRAY_BUFFER,
                pos_data.len() as isize * ::std::mem::size_of::<GLfloat>() as isize,
                pos_data.as_ptr() as _,
                gl::STATIC_DRAW,
            );
            gl::BindBuffer(gl::ARRAY_BUFFER, vbo_tex.id);
            gl::BufferData(
                gl::ARRAY_BUFFER,
                tex_data.len() as isize * ::std::mem::size_of::<GLfloat>() as isize,
                tex_data.as_ptr() as _,
                gl::STATIC_DRAW,
            );

            gl::BindBuffer(gl::ARRAY_BUFFER, vbo_pos.id);
            gl::VertexAttribPointer(
                loc_a_position,
                2,
                gl::FLOAT,
                gl::FALSE,
                0,
                ::std::ptr::null(),
            );

            gl::BindBuffer(gl::ARRAY_BUFFER, vbo_tex.id);
            gl::VertexAttribPointer(
                loc_a_texcoord,
                2,
                gl::FLOAT,
                gl::FALSE,
                0,
                ::std::ptr::null(),
            );

            gl::EnableVertexAttribArray(loc_a_position);
            gl::EnableVertexAttribArray(loc_a_texcoord);

            let surf = Self {
                tex: Texture::new(),
                vao: vao,
                vbo_pos,
                vbo_tex,
                program: program,
                pos_data,
                tex_data,
            };

            surf
        }
    }

    pub fn render<C: ColorForTexture>(&self, buffer: &GfxBufferLE<C>) {
        unsafe {
            gl::UseProgram(self.program.id);

            self.tex.copy_from_buffer(buffer);
            gl::ActiveTexture(gl::TEXTURE0);
            gl::BindTexture(gl::TEXTURE_2D, self.tex.id);

            gl::BindVertexArray(self.vao.id);
            gl::DrawArrays(gl::TRIANGLE_STRIP, 0, 4);
        }
    }
}
