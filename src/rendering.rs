use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::fmt::{Display, Formatter};
use std::mem::offset_of;
use std::path::Path;
use std::ptr::{null, null_mut};
use std::{mem, ptr};

use bytemuck::{bytes_of, cast_slice, Pod, Zeroable};
use gl::types::{GLchar, GLenum, GLint, GLsizei, GLsizeiptr, GLuint};
use glam::{Mat4, Vec2, Vec3};
use image::ColorType;
use sdl2::video::{GLContext, GLProfile, SwapInterval, Window};
use sdl2::{Sdl, VideoSubsystem};

use crate::AnalogClock;

const WINDOW_TITLE: &str = "glClock";
const WINDOW_SIZE: u32 = 800;

pub trait Drawable {
    fn draw(&self);
}

pub struct GPUBuffer {
    id: GLuint,
    buffer_type: GLenum,
}

impl GPUBuffer {
    pub fn new(buffer_type: GLenum) -> Self {
        let mut buf_id: u32 = 0;

        unsafe {
            gl::GenBuffers(1, ptr::addr_of_mut!(buf_id));
        }

        Self {
            id: buf_id,
            buffer_type,
        }
    }

    pub fn set_data(&self, data: &[u8], usage: GLenum) {
        unsafe {
            gl::BindBuffer(self.buffer_type, self.id);
            gl::BufferData(
                self.buffer_type,
                data.len() as GLsizeiptr,
                data.as_ptr() as *const _,
                usage,
            );
        }
    }
}

impl Drop for GPUBuffer {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteBuffers(1, &self.id);
        }
    }
}

struct GPUVertexArray(GLuint);

impl GPUVertexArray {
    fn new() -> Self {
        let mut id: u32 = 0;

        unsafe {
            gl::GenVertexArrays(1, ptr::addr_of_mut!(id));
        }

        Self(id)
    }

    fn bind(&self) {
        unsafe {
            gl::BindVertexArray(self.0);
        }
    }
}

impl Drop for GPUVertexArray {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteVertexArrays(1, &self.0);
        }
    }
}

pub enum TextureUsage {
    Diffuse,
    Normal,
}

/// Managed GPU texture
pub struct GPUTexture {
    id: u32,
    usage: TextureUsage,
}

impl GPUTexture {
    pub fn from_img_file(path: &Path, usage: TextureUsage) -> anyhow::Result<Self> {
        if !path.is_file() {
            return Err(anyhow::Error::msg(
                "Image path does not lead to a regular file",
            ));
        }

        let img = image::io::Reader::open(path)?.decode()?;
        let img_width = img.width();
        let img_height = img.height();
        let img_format = match &img.color() {
            ColorType::Rgb8 => gl::RGB,
            ColorType::Rgba8 => gl::RGBA,
            color_type => {
                return Err(anyhow::Error::msg(format!(
                    "Unsupported color format: {:?}",
                    color_type
                )))
            }
        };

        let mut texture_id: u32 = 0;
        unsafe {
            gl::GenTextures(1, ptr::addr_of_mut!(texture_id));
            gl::BindTexture(gl::TEXTURE_2D, texture_id);

            // Set texture wrapping/filtering properties
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::REPEAT as GLint);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::REPEAT as GLint);
            gl::TexParameteri(
                gl::TEXTURE_2D,
                gl::TEXTURE_MIN_FILTER,
                gl::LINEAR_MIPMAP_LINEAR as GLint,
            );
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::LINEAR as GLint);

            gl::TexImage2D(
                gl::TEXTURE_2D,
                0,
                img_format as GLsizei,
                img_width as GLsizei,
                img_height as GLsizei,
                0,
                img_format,
                gl::UNSIGNED_BYTE,
                img.as_bytes().as_ptr() as *const _,
            );
            gl::GenerateMipmap(gl::TEXTURE_2D);
        }

        Ok(Self {
            id: texture_id,
            usage,
        })
    }

    pub fn bind(&self) {
        unsafe {
            gl::BindTexture(gl::TEXTURE_2D, self.id);
        }
    }
}

impl Drop for GPUTexture {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteTextures(1, &self.id);
        }
    }
}

#[derive(Pod, Zeroable, Copy, Clone)]
#[repr(C)]
pub struct Vertex {
    position: Vec3,
    texture_coordinate: Vec2,
    normal: Vec3,
}

impl Vertex {
    const STRIDE: usize = mem::size_of::<Self>();
    const TEX_COORDINATE_OFFSET: usize = offset_of!(Vertex, texture_coordinate);
    const NORMAL_OFFSET: usize = offset_of!(Vertex, normal);

    fn set_vertex_attributes() {
        unsafe {
            gl::VertexAttribPointer(
                0,
                3,
                gl::FLOAT,
                gl::FALSE,
                Vertex::STRIDE as GLsizei,
                null(),
            );
            gl::EnableVertexAttribArray(0);

            gl::VertexAttribPointer(
                1,
                2,
                gl::FLOAT,
                gl::FALSE,
                Vertex::STRIDE as GLsizei,
                Vertex::TEX_COORDINATE_OFFSET as *const _,
            );
            gl::EnableVertexAttribArray(1);

            gl::VertexAttribPointer(
                2,
                3,
                gl::FLOAT,
                gl::FALSE,
                Vertex::STRIDE as GLsizei,
                Vertex::NORMAL_OFFSET as *const _,
            );
            gl::EnableVertexAttribArray(2);
        }
    }

    pub const fn new(position: [f32; 3], texture_coordinate: [f32; 2], normal: [f32; 3]) -> Self {
        Self {
            position: Vec3::from_array(position),
            texture_coordinate: Vec2::from_array(texture_coordinate),
            normal: Vec3::from_array(normal),
        }
    }
}

pub struct Mesh {
    /// Mesh vertices
    vertices: Vec<Vertex>,
    /// Draw indices
    indices: Vec<u32>,

    /// Mesh Vertex Array Object
    vertex_array_object: GPUVertexArray,
    #[allow(unused)]
    /// Mesh vertices GPU buffer
    vertex_buffer: GPUBuffer,
    #[allow(unused)]
    /// Mesh indices GPU buffer
    index_buffer: GPUBuffer,
}

impl Mesh {
    pub fn new(vertices: &[Vertex], indices: &[u32]) -> Self {
        let vertex_buffer = GPUBuffer::new(gl::ARRAY_BUFFER);
        let index_buffer = GPUBuffer::new(gl::ELEMENT_ARRAY_BUFFER);
        let vertex_array_object = GPUVertexArray::new();

        vertex_array_object.bind();
        vertex_buffer.set_data(cast_slice(vertices), gl::STATIC_DRAW);
        index_buffer.set_data(cast_slice(indices), gl::STATIC_DRAW);
        Vertex::set_vertex_attributes();

        unsafe {
            gl::BindVertexArray(0);
        }

        Self {
            vertices: Vec::from(vertices),
            indices: Vec::from(indices),
            vertex_array_object,
            vertex_buffer,
            index_buffer,
        }
    }

    pub fn draw(&self) {
        self.vertex_array_object.bind();
        unsafe {
            gl::DrawElements(
                gl::TRIANGLES,
                self.indices.len() as GLsizei,
                gl::UNSIGNED_INT,
                null(),
            );
            gl::BindVertexArray(0);
        }
    }

    pub fn draw_instanced(&self, count: i32) {
        self.vertex_array_object.bind();
        unsafe {
            gl::DrawElementsInstancedBaseInstance(
                gl::TRIANGLES,
                self.indices.len() as GLsizei,
                gl::UNSIGNED_INT,
                null(),
                count,
                0,
            );
            gl::BindVertexArray(0);
        }
    }

    pub fn make_rect(
        width: f32,
        height: f32,
        uv_top_left: Option<[f32; 2]>,
        uv_bottom_right: Option<[f32; 2]>,
    ) -> Self {
        let half_width = width * 0.5;
        let half_height = height * 0.5;
        let uv_top_left = uv_top_left.unwrap_or([0.0, 0.0]);
        let uv_bottom_right = uv_bottom_right.unwrap_or([1.0, 1.0]);
        let uv_top_right = [uv_bottom_right[0], uv_top_left[1]];
        let uv_bottom_left = [uv_top_left[0], uv_bottom_right[1]];

        let normal = [0.0, 0.0, 1.0];

        let vertices: [Vertex; 4] = [
            Vertex::new([half_width, half_height, 0.0], uv_top_right, normal), // top-right
            Vertex::new([half_width, -half_height, 0.0], uv_bottom_right, normal), // bottom-right
            Vertex::new([-half_width, -half_height, 0.0], uv_bottom_left, normal), // bottom-left
            Vertex::new([-half_width, half_height, 0.0], uv_top_left, normal), // top-left
        ];

        let indices: [u32; 6] = [
            0, 1, 3, // first triangle
            1, 2, 3, // Second triangle
        ];

        Self::new(&vertices, &indices)
    }
}

pub struct ShaderProgram {
    name: String,
    id: u32,
    uniform_locations: HashMap<String, i32>,
}

impl ShaderProgram {
    unsafe fn compile_shader_stage(source: &str, stage: GLenum) -> anyhow::Result<u32> {
        let shader_id: u32 = gl::CreateShader(stage);
        let src_c_str = CString::new(source.as_bytes())?;

        gl::ShaderSource(shader_id, 1, &src_c_str.as_ptr(), null());
        gl::CompileShader(shader_id);

        // Check if successful
        let mut success = 0;

        gl::GetShaderiv(shader_id, gl::COMPILE_STATUS, ptr::addr_of_mut!(success));
        if success == 0 {
            // Compilation failed
            let mut info_log = vec![0; 512];
            gl::GetShaderInfoLog(
                shader_id,
                info_log.len() as GLsizei,
                null_mut(),
                info_log.as_mut_ptr() as *mut GLchar,
            );

            // Shrink log to make compatible with CString
            if let Some(null_idx) = info_log.iter().position(|byte| *byte == 0) {
                info_log.truncate(null_idx);
            }
            let log_str = CString::new(info_log)?;
            let log_str = log_str.into_string()?;

            return Err(anyhow::Error::msg(log_str));
        }

        Ok(shader_id)
    }

    pub unsafe fn from_sources(
        name: &str,
        vertex_src: &str,
        fragment_src: &str,
    ) -> anyhow::Result<Self> {
        let vertex_shader = ShaderProgram::compile_shader_stage(vertex_src, gl::VERTEX_SHADER)?;
        let fragment_shader =
            match ShaderProgram::compile_shader_stage(fragment_src, gl::FRAGMENT_SHADER) {
                Ok(id) => id,
                Err(err) => {
                    gl::DeleteShader(vertex_shader);
                    return Err(err);
                }
            };

        let program_id = gl::CreateProgram();
        gl::AttachShader(program_id, vertex_shader);
        gl::AttachShader(program_id, fragment_shader);
        gl::LinkProgram(program_id);

        gl::DeleteShader(fragment_shader);
        gl::DeleteShader(vertex_shader);

        // Check link status
        let mut success = 0;

        gl::GetProgramiv(program_id, gl::LINK_STATUS, ptr::addr_of_mut!(success));
        if success == 0 {
            let mut info_log = vec![0; 512];
            gl::GetProgramInfoLog(
                program_id,
                info_log.len() as GLsizei,
                null_mut(),
                info_log.as_mut_ptr() as *mut GLchar,
            );

            if let Some(null_idx) = info_log.iter().position(|byte| *byte == 0) {
                info_log.truncate(null_idx);
            }
            let log_str = CString::new(info_log)?;
            let log_str = log_str.into_string()?;

            return Err(anyhow::Error::msg(log_str));
        }

        Ok(Self {
            name: String::from(name),
            id: program_id,
            uniform_locations: HashMap::default(),
        })
    }

    pub fn activate(&self) {
        unsafe {
            gl::UseProgram(self.id);
        }
    }

    fn get_uniform_location(&mut self, name: &str) -> Option<i32> {
        if let Some(location) = self.uniform_locations.get(name) {
            return Some(*location);
        }

        let c_name =
            CString::new(name).expect("Should be able to turn uniform name into a CString");

        let location = unsafe { gl::GetUniformLocation(self.id, c_name.as_ptr() as *const GLchar) };
        if location < 0 {
            None
        } else {
            self.uniform_locations.insert(name.into(), location);
            Some(location)
        }
    }

    pub fn set_mat4(&mut self, name: &str, value: &Mat4) {
        if let Some(location) = self.get_uniform_location(name) {
            let bytes = bytes_of(value);
            unsafe {
                gl::UniformMatrix4fv(location, 1, gl::FALSE, bytes.as_ptr() as *const _);
            }
        } else {
            eprintln!(
                "Mat4 shader uniform, \"{name}\", not found in \"{}\"",
                self.name
            );
        }
    }

    pub fn set_mat4_array(&mut self, name: &str, values: &[Mat4]) {
        if let Some(location) = self.get_uniform_location(name) {
            let bytes: Vec<u8> = values.iter().flat_map(bytes_of).copied().collect();
            unsafe {
                gl::UniformMatrix4fv(
                    location,
                    values.len() as GLsizei,
                    gl::FALSE,
                    bytes.as_ptr() as *const _,
                );
            }
        } else {
            eprintln!(
                "Mat4 shader uniform array, \"{name}\", not found in \"{}\"",
                self.name
            );
        }
    }

    pub fn set_vec3(&mut self, name: &str, value: &Vec3) {
        if let Some(location) = self.get_uniform_location(name) {
            let bytes = bytes_of(value);
            unsafe {
                gl::Uniform3fv(location, 1, bytes.as_ptr() as *const _);
            }
        } else {
            eprintln!(
                "Vec3 shader uniform, \"{name}\", not found in \"{}\"",
                self.name
            );
        }
    }

    pub fn set_vec2(&mut self, name: &str, value: &Vec2) {
        if let Some(location) = self.get_uniform_location(name) {
            let bytes = bytes_of(value);
            unsafe {
                gl::Uniform2fv(location, 1, bytes.as_ptr() as *const _);
            }
        } else {
            eprintln!(
                "Vec2 shader uniform, \"{name}\", not found in \"{}\"",
                self.name
            );
        }
    }
}

impl Drop for ShaderProgram {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteProgram(self.id);
        }
    }
}

pub struct Renderer {
    #[allow(unused)]
    gl_ctx: GLContext,
    gl_version: (u8, u8),
    gl_renderer: String,
    #[allow(unused)]
    video_subsystem: VideoSubsystem,
    window: Window,
}

impl Display for Renderer {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "OpenGL {}.{}\nRenderer: {}",
            self.gl_version.0, self.gl_version.1, self.gl_renderer
        )
    }
}

impl Renderer {
    pub fn new(sdl_ctx: &Sdl) -> anyhow::Result<Self> {
        let video_subsystem = sdl_ctx.video().map_err(anyhow::Error::msg)?;

        let gl_attr = video_subsystem.gl_attr();

        #[cfg(target_os = "macos")]
        gl_attr.set_context_flags().forward_compatible().set();

        #[cfg(debug_assertions)]
        gl_attr.set_context_flags().debug().set();

        #[cfg(not(debug_assertions))]
        gl_attr.set_context_no_error(true);

        gl_attr.set_context_profile(GLProfile::Core);
        gl_attr.set_context_version(4, 6);

        gl_attr.set_framebuffer_srgb_compatible(true);
        gl_attr.set_double_buffer(true);
        gl_attr.set_multisample_samples(4);

        let window = video_subsystem
            .window(WINDOW_TITLE, WINDOW_SIZE, WINDOW_SIZE)
            .opengl()
            .position_centered()
            .build()?;

        let gl_ctx = window.gl_create_context().map_err(anyhow::Error::msg)?;

        gl::load_with(|name| video_subsystem.gl_get_proc_address(name) as *const _);

        video_subsystem
            .gl_set_swap_interval(SwapInterval::Immediate)
            .map_err(anyhow::Error::msg)?;

        let mut gl_version = (0, 0);
        unsafe {
            let mut version_val: i32 = 0;
            gl::GetIntegerv(gl::MAJOR_VERSION, ptr::addr_of_mut!(version_val));
            gl_version.0 = version_val as u8;
            gl::GetIntegerv(gl::MINOR_VERSION, ptr::addr_of_mut!(version_val));
            gl_version.1 = version_val as u8;
        }

        let gl_renderer: String = unsafe {
            let gl_renderer_str = CStr::from_ptr(gl::GetString(gl::RENDERER) as *const _)
                .to_str()
                .unwrap_or("BAD_CSTR");
            String::from(gl_renderer_str)
        };

        unsafe {
            // gl::Enable(gl::DEPTH_TEST);
            gl::Enable(gl::MULTISAMPLE);
            gl::Viewport(0, 0, WINDOW_SIZE as i32, WINDOW_SIZE as i32);
            gl::ClearColor(0.2, 0.3, 0.3, 1.0);
            gl::Clear(gl::COLOR_BUFFER_BIT);
        }

        Ok(Self {
            gl_ctx,
            video_subsystem,
            window,
            gl_version,
            gl_renderer,
        })
    }

    pub fn set_clear_color_u8(&self, red: u8, green: u8, blue: u8) {
        let r = (red as f32) / (u8::MAX as f32);
        let g = (green as f32) / (u8::MAX as f32);
        let b = (blue as f32) / (u8::MAX as f32);

        unsafe {
            gl::ClearColor(r, g, b, 1.0);
        }
    }

    pub fn draw(&self, clock: &AnalogClock) {
        unsafe {
            gl::ClearColor(0.2, 0.3, 0.3, 1.0);
            gl::Clear(gl::COLOR_BUFFER_BIT);
        }

        clock.draw();

        self.window.gl_swap_window();
    }
}
