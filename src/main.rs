use bytemuck::{bytes_of, cast_slice, Pod, Zeroable};
use chrono::{Local, Timelike};
use gl::types::*;
use glam::{Affine3A, Mat4, Vec2, Vec3, Vec3A};
use image::ColorType;
use sdl2::event::Event;
use sdl2::video::{GLContext, GLProfile, SwapInterval, Window};
use sdl2::{Sdl, VideoSubsystem};
use std::cell::RefCell;
use std::collections::HashMap;
use std::f32::consts;
use std::ffi::CString;
use std::mem::offset_of;
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};
use std::rc::Rc;
use std::str::FromStr;
use std::{mem, ptr};

const WINDOW_TITLE: &str = "glClock";
const WINDOW_SIZE: u32 = 800;

trait Drawable {
    fn draw(&self);
}

struct GPUBuffer {
    id: GLuint,
    buffer_type: GLenum,
}

impl GPUBuffer {
    fn new(buffer_type: GLenum) -> Self {
        let mut buf_id: u32 = 0;

        unsafe {
            gl::GenBuffers(1, ptr::addr_of_mut!(buf_id));
        }

        Self {
            id: buf_id,
            buffer_type,
        }
    }

    fn set_data(&self, data: &[u8], usage: GLenum) {
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

enum TextureUsage {
    Diffuse,
    Normal,
}

/// Managed GPU texture
struct GPUTexture {
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
                img_format as GLint,
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
struct Vertex {
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

    const fn new(position: [f32; 3], texture_coordinate: [f32; 2], normal: [f32; 3]) -> Self {
        Self {
            position: Vec3::from_array(position),
            texture_coordinate: Vec2::from_array(texture_coordinate),
            normal: Vec3::from_array(normal),
        }
    }
}

struct Mesh {
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
    fn new(vertices: &[Vertex], indices: &[u32]) -> Self {
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

    fn draw(&self) {
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
}

struct ShaderProgram {
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

    pub unsafe fn from_sources(vertex_src: &str, fragment_src: &str) -> anyhow::Result<Self> {
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
            eprintln!("Mat4 shader uniform, \"{name}\", not found");
        }
    }

    pub fn set_vec3(&mut self, name: &str, value: &Vec3) {
        if let Some(location) = self.get_uniform_location(name) {
            let bytes = bytes_of(value);
            unsafe {
                gl::Uniform3fv(location, 1, bytes.as_ptr() as *const _);
            }
        } else {
            eprintln!("Vec3 shader uniform, \"{name}\", not found");
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

struct ClockFace {
    mesh: Mesh,
    shader_program: ShaderProgram,
    texture: GPUTexture,
}

impl ClockFace {
    const VERTICES: [Vertex; 4] = [
        Vertex::new([1.0, 1.0, 0.0], [1.0, 0.0], [0.0, 0.0, 0.0]), // top-right
        Vertex::new([1.0, -1.0, 0.0], [1.0, 1.0], [0.0, 0.0, 0.0]), // bottom-right
        Vertex::new([-1.0, -1.0, 0.0], [0.0, 1.0], [0.0, 0.0, 0.0]), // bottom-left
        Vertex::new([-1.0, 1.0, 0.0], [0.0, 0.0], [0.0, 0.0, 0.0]), // top-left
    ];

    const INDICES: [u32; 6] = [
        0, 1, 3, // first triangle
        1, 2, 3, // Second triangle
    ];

    const SHADER_SRC: (&'static str, &'static str) = (
        include_str!("shaders/clockFace.vert"),
        include_str!("shaders/clockFace.frag"),
    );

    const TEXTURE_PATH: &'static str = "assets/textures/clockFace.webp";

    fn new() -> anyhow::Result<Self> {
        let mesh = Mesh::new(&Self::VERTICES, &Self::INDICES);
        let shader_program =
            unsafe { ShaderProgram::from_sources(Self::SHADER_SRC.0, Self::SHADER_SRC.1) }?;

        let tex_path = PathBuf::from_str(Self::TEXTURE_PATH)?;

        let texture = GPUTexture::from_img_file(&tex_path, TextureUsage::Diffuse)?;

        Ok(Self {
            mesh,
            shader_program,
            texture,
        })
    }
}

impl Drawable for ClockFace {
    fn draw(&self) {
        self.shader_program.activate();
        self.texture.bind();
        self.mesh.draw();
    }
}

struct ClockHand {
    mesh: Rc<RefCell<Mesh>>,
    shader_program: Rc<RefCell<ShaderProgram>>,

    pub color: Vec3,

    length: f32,
    mount_point: Vec3A,
    target_point: Vec3A,

    origin: Vec3A,
    rotation: f32,

    transform: Mat4,
}

impl ClockHand {
    const VERTICES: [Vertex; 3] = [
        Vertex::new([0.0, 1.0, 0.0], [0.5, 0.0], [0.0, 0.0, 0.0]),
        Vertex::new([1.0, -1.0, 0.0], [1.0, 1.0], [0.0, 0.0, 0.0]),
        Vertex::new([-1.0, -1.0, 0.0], [0.0, 1.0], [0.0, 0.0, 0.0]),
    ];

    const INDICES: [u32; 3] = [0, 1, 2];

    const SHADER_SRC: (&'static str, &'static str) = (
        include_str!("shaders/clockHand.vert"),
        include_str!("shaders/clockHand.frag"),
    );

    pub fn new(
        mesh: Rc<RefCell<Mesh>>,
        shader_program: Rc<RefCell<ShaderProgram>>,
        length: f32,
        depth: f32,
        color: Vec3,
    ) -> Self {
        let mount_point = Vec3A::new(0.0, 0.0, depth);
        let target_point = Vec3A::new(0.0, 1.0, depth);
        let origin = (mount_point + target_point) * 0.5;

        let transform = Mat4::IDENTITY;

        let mut hand = Self {
            mesh,
            shader_program,
            color,
            length,
            mount_point,
            target_point,
            origin,
            rotation: 0.0,
            transform,
        };
        hand.update_transform();

        hand
    }

    fn update_transform(&mut self) {
        let scale = Vec3::new(0.03, self.length, 1.0);

        let translation = Vec3::new(0.0, self.length, self.origin.z);

        let transform = Affine3A::from_rotation_z(self.rotation)
            * Affine3A::from_translation(translation)
            * Affine3A::from_scale(scale);

        self.transform = Mat4::from(transform);
    }

    /// Set the hand's rotation (in degrees)
    pub fn set_rotation(&mut self, rotation: f32) {
        let radians = rotation * (consts::PI / 180.0);
        self.rotation = -radians;

        self.target_point.x = self.rotation.cos() * self.length;
        self.target_point.y = self.rotation.sin() * self.length;

        self.origin = (self.mount_point + self.target_point) * 0.5;
        self.update_transform();
    }
}

impl Drawable for ClockHand {
    fn draw(&self) {
        let mut shader_program = self.shader_program.borrow_mut();
        let mesh = self.mesh.borrow();

        shader_program.activate();

        // Set uniforms
        shader_program.set_mat4("transformation", &self.transform);
        shader_program.set_vec3("color", &self.color);

        mesh.draw();
    }
}

struct AnalogClock {
    face: ClockFace,
    second_hand: ClockHand,
    minute_hand: ClockHand,
    hour_hand: ClockHand,
}

impl AnalogClock {
    const SECOND_HAND_COLOR: Vec3 = Vec3::new(1.0, 0.0, 0.0);
    const MINUTE_HAND_COLOR: Vec3 = Vec3::new(0.0, 1.0, 0.0);
    const HOUR_HAND_COLOR: Vec3 = Vec3::new(0.0, 0.0, 1.0);

    pub fn new() -> anyhow::Result<Self> {
        let face = ClockFace::new()?;

        let clock_hand_mesh = Rc::new(RefCell::new(Mesh::new(
            &ClockHand::VERTICES,
            &ClockHand::INDICES,
        )));
        let clock_hand_shader = unsafe {
            ShaderProgram::from_sources(ClockHand::SHADER_SRC.0, ClockHand::SHADER_SRC.1)
        }?;
        let clock_hand_shader = Rc::new(RefCell::new(clock_hand_shader));

        let second_hand = ClockHand::new(
            clock_hand_mesh.clone(),
            clock_hand_shader.clone(),
            0.48,
            -0.1,
            Self::SECOND_HAND_COLOR,
        );
        let minute_hand = ClockHand::new(
            clock_hand_mesh.clone(),
            clock_hand_shader.clone(),
            0.41,
            -0.2,
            Self::MINUTE_HAND_COLOR,
        );
        let hour_hand = ClockHand::new(
            clock_hand_mesh.clone(),
            clock_hand_shader.clone(),
            0.30,
            -0.3,
            Self::HOUR_HAND_COLOR,
        );

        Ok(Self {
            face,
            second_hand,
            minute_hand,
            hour_hand,
        })
    }

    fn get_seconds_rotation(seconds: f32, milliseconds: f32) -> f32 {
        (seconds * 6.0) + (milliseconds * 0.006)
    }

    fn get_minutes_rotation(minutes: f32, seconds: f32, milliseconds: f32) -> f32 {
        (minutes * 6.0) + (seconds * 0.1) + (milliseconds * 0.0001)
    }

    fn get_hours_rotation(hours: f32, minutes: f32, seconds: f32) -> f32 {
        (hours * 30.0) + (minutes * 0.5) + (seconds * 0.008333)
    }

    pub fn update(&mut self) {
        let current_time = Local::now();

        let hours = (current_time.hour() % 12) as f32;
        let minutes = current_time.minute() as f32;
        let seconds = current_time.second() as f32;
        let milliseconds = current_time.nanosecond() as f32 / 1_000_000.0;

        self.second_hand
            .set_rotation(AnalogClock::get_seconds_rotation(seconds, milliseconds));
        self.minute_hand
            .set_rotation(AnalogClock::get_minutes_rotation(
                minutes,
                seconds,
                milliseconds,
            ));
        self.hour_hand
            .set_rotation(AnalogClock::get_hours_rotation(hours, minutes, seconds));
    }
}

impl Drawable for AnalogClock {
    fn draw(&self) {
        self.face.draw();
        self.second_hand.draw();
        self.minute_hand.draw();
        self.hour_hand.draw();
    }
}

struct Renderer {
    #[allow(unused)]
    gl_ctx: GLContext,
    #[allow(unused)]
    video_subsystem: VideoSubsystem,
    window: Window,
}

impl Renderer {
    fn new(sdl_ctx: &Sdl) -> anyhow::Result<Self> {
        let video_subsystem = sdl_ctx.video().map_err(anyhow::Error::msg)?;

        let gl_attr = video_subsystem.gl_attr();

        #[cfg(target_os = "macos")]
        gl_attr.set_context_flags().forward_compatible().set();

        gl_attr.set_context_profile(GLProfile::Core);
        gl_attr.set_context_version(4, 1);

        #[cfg(debug_assertions)]
        gl_attr.set_context_flags().debug().set();

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
            .gl_set_swap_interval(SwapInterval::VSync)
            .map_err(anyhow::Error::msg)?;

        unsafe {
            gl::Enable(gl::DEPTH_TEST);
            gl::Enable(gl::MULTISAMPLE);
            gl::Viewport(0, 0, WINDOW_SIZE as i32, WINDOW_SIZE as i32);
            gl::ClearColor(0.0, 0.0, 0.0, 1.0);
            gl::Clear(gl::COLOR_BUFFER_BIT | gl::DEPTH_BUFFER_BIT);
        }

        Ok(Self {
            gl_ctx,
            video_subsystem,
            window,
        })
    }

    fn set_clear_color_u8(&self, red: u8, green: u8, blue: u8) {
        let r = (red as f32) / (u8::MAX as f32);
        let g = (green as f32) / (u8::MAX as f32);
        let b = (blue as f32) / (u8::MAX as f32);

        unsafe {
            gl::ClearColor(r, g, b, 1.0);
        }
    }

    fn draw(&self, clock: &AnalogClock) {
        unsafe {
            gl::Clear(gl::COLOR_BUFFER_BIT | gl::DEPTH_BUFFER_BIT);
        }

        clock.draw();

        self.window.gl_swap_window();
    }
}

fn main() -> anyhow::Result<()> {
    let sdl_context = sdl2::init().map_err(anyhow::Error::msg)?;
    let renderer = Renderer::new(&sdl_context)?;

    let mut clock = AnalogClock::new()?;

    let mut event_pump = sdl_context.event_pump().map_err(anyhow::Error::msg)?;
    'running: loop {
        for event in event_pump.poll_iter() {
            if let Event::Quit { .. } = event {
                break 'running;
            }
        }

        clock.update();

        renderer.draw(&clock);
    }

    Ok(())
}
