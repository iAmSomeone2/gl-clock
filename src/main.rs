use std::cell::RefCell;
use std::f32::consts;
use std::path::PathBuf;
use std::rc::Rc;
use std::str::FromStr;

use chrono::{Local, Timelike};
use glam::{Affine3A, Mat4, Vec3, Vec3A};
use sdl2::event::Event;

use crate::rendering::{Drawable, GPUTexture, Mesh, Renderer, ShaderProgram, TextureUsage, Vertex};

mod rendering;

struct ClockFace {
    face_mesh: Mesh,
    face_shader_program: ShaderProgram,
    face_texture: GPUTexture,
    tick_mesh: Mesh,
    tick_shader_program: ShaderProgram,
}

impl ClockFace {
    const TICK_COUNT: u8 = 60;

    const FACE_SHADER_SRC: (&'static str, &'static str) = (
        include_str!("shaders/clockFace.vert"),
        include_str!("shaders/clockFace.frag"),
    );

    const FACE_TEXTURE_PATH: &'static str = "assets/textures/clockFace.png";

    const TICK_SHADER_SRC: (&'static str, &'static str) = (
        include_str!("shaders/clockTick.vert"),
        include_str!("shaders/clockTick.frag"),
    );

    fn calculate_tick_transform(index: u8, tick_height: f32) -> Mat4 {
        let (scale, y_translation) = if index % 15 == 0 {
            let scale_factor = 3.0;
            let y_translation = 1.0 - (tick_height * 0.5 * scale_factor);

            (Vec3::new(scale_factor, scale_factor, 1.0), y_translation)
        } else if index % 5 == 0 {
            let scale_factor = 1.5;
            let y_translation = 1.0 - (tick_height * 0.5 * scale_factor);

            (Vec3::new(scale_factor, scale_factor, 1.0), y_translation)
        } else {
            let y_translation = 1.0 - (tick_height * 0.5);
            (Vec3::ONE, y_translation)
        };

        let tick_rads = 0.104_719_76 * (index as f32);
        Mat4::from(
            Affine3A::from_rotation_z(tick_rads)
                * Affine3A::from_translation(Vec3::new(0.0, y_translation, -0.05))
                * Affine3A::from_scale(scale),
        )
    }

    fn new() -> anyhow::Result<Self> {
        let size = 2.0;

        let face_mesh = Mesh::make_rect(size, size, None, None);
        let face_shader_program = unsafe {
            ShaderProgram::from_sources(
                "clockFace",
                Self::FACE_SHADER_SRC.0,
                Self::FACE_SHADER_SRC.1,
            )
        }?;

        let tex_path = PathBuf::from_str(Self::FACE_TEXTURE_PATH)?;

        let face_texture = GPUTexture::from_img_file(&tex_path, TextureUsage::Diffuse)?;

        let tick_height = 0.03;
        let tick_mesh = Mesh::make_rect(0.02, tick_height, None, None);
        let mut tick_shader_program = unsafe {
            ShaderProgram::from_sources(
                "clockTick",
                Self::TICK_SHADER_SRC.0,
                Self::TICK_SHADER_SRC.1,
            )
        }?;

        let transformations: Vec<Mat4> = (0..ClockFace::TICK_COUNT)
            .map(|i| ClockFace::calculate_tick_transform(i, tick_height))
            .collect();
        tick_shader_program.activate();
        tick_shader_program.set_mat4_array("transformations", &transformations);

        Ok(Self {
            face_mesh,
            face_shader_program,
            face_texture,
            tick_mesh,
            tick_shader_program,
        })
    }
}

impl Drawable for ClockFace {
    fn draw(&self) {
        // Draw the face mesh
        self.face_shader_program.activate();
        self.face_texture.bind();
        self.face_mesh.draw();

        // Draw the ticks
        self.tick_shader_program.activate();
        self.tick_mesh.draw_instanced(ClockFace::TICK_COUNT as i32);
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
            ShaderProgram::from_sources(
                "clockHand",
                ClockHand::SHADER_SRC.0,
                ClockHand::SHADER_SRC.1,
            )
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

fn main() -> anyhow::Result<()> {
    let sdl_context = sdl2::init().map_err(anyhow::Error::msg)?;
    let renderer = Renderer::new(&sdl_context)?;
    println!("{renderer}");

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
