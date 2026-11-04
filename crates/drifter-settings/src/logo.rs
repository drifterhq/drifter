use std::time::Instant;

use glow::HasContext;
use khronos_egl as egl;
use smithay_client_toolkit::subcompositor::SubcompositorState;
use wayland_client::protocol::wl_subsurface::WlSubsurface;
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_client::{Connection, Proxy, QueueHandle};

const VERTEX: &str = "attribute vec2 a;
varying vec2 v_coords;
void main(){v_coords=vec2(a.x*0.5+0.5,0.5-a.y*0.5);gl_Position=vec4(a,0.,1.);}
";

pub struct Logo {
    egl: egl::Instance<egl::Static>,
    display: egl::Display,
    context: egl::Context,
    config: egl::Config,
    pub surface: WlSurface,
    subsurface: WlSubsurface,
    window: Option<(wayland_egl::WlEglSurface, egl::Surface, i32)>,
    gl: glow::Context,
    program: glow::Program,
    clock: (f64, Instant),
    pace: f64,
    waiting: bool,
}

impl Logo {
    pub fn new<State>(
        connection: &Connection,
        subcompositor: &SubcompositorState,
        parent: &WlSurface,
        qh: &QueueHandle<State>,
    ) -> Result<Logo, String>
    where
        State: wayland_client::Dispatch<WlSurface, smithay_client_toolkit::compositor::SurfaceData>
            + wayland_client::Dispatch<WlSubsurface, smithay_client_toolkit::subcompositor::SubsurfaceData>
            + 'static,
    {
        let egl = egl::Instance::new(egl::Static);
        let native = connection.backend().display_ptr() as *mut std::ffi::c_void;
        let display = unsafe { egl.get_display(native) }.ok_or("no egl display")?;
        egl.initialize(display).map_err(|e| format!("egl: {e}"))?;
        egl.bind_api(egl::OPENGL_ES_API).map_err(|e| format!("egl: {e}"))?;
        let wanted = [
            egl::RED_SIZE, 8,
            egl::GREEN_SIZE, 8,
            egl::BLUE_SIZE, 8,
            egl::ALPHA_SIZE, 8,
            egl::SURFACE_TYPE, egl::WINDOW_BIT,
            egl::RENDERABLE_TYPE, egl::OPENGL_ES2_BIT,
            egl::NONE,
        ];
        let config = egl
            .choose_first_config(display, &wanted)
            .map_err(|e| format!("egl: {e}"))?
            .ok_or("no egl config with alpha")?;
        let context = egl
            .create_context(display, config, None, &[egl::CONTEXT_CLIENT_VERSION, 2, egl::NONE])
            .map_err(|e| format!("egl: {e}"))?;
        egl.make_current(display, None, None, Some(context)).map_err(|e| format!("egl: {e}"))?;
        let gl = unsafe {
            glow::Context::from_loader_function(|name| {
                egl.get_proc_address(name).map_or(std::ptr::null(), |f| f as *const std::ffi::c_void)
            })
        };
        let program = unsafe { build(&gl)? };
        let (subsurface, surface) = subcompositor.create_subsurface(parent.clone(), qh);
        subsurface.set_desync();
        Ok(Logo {
            egl,
            display,
            context,
            config,
            surface,
            subsurface,
            window: None,
            gl,
            program,
            clock: (20.0, Instant::now()),
            pace: 0.0,
            waiting: false,
        })
    }

    pub fn place(&mut self, x: i32, y: i32, side: i32, scale: i32) -> Result<(), String> {
        self.subsurface.set_position(x, y);
        self.surface.set_buffer_scale(scale);
        let pixels = (side * scale).max(1);
        if let Some((window, _, had)) = self.window.as_ref() {
            if *had == pixels {
                return Ok(());
            }
            window.resize(pixels, pixels, 0, 0);
            if let Some(entry) = self.window.as_mut() {
                entry.2 = pixels;
            }
            return Ok(());
        }
        let window = wayland_egl::WlEglSurface::new(self.surface.id(), pixels, pixels)
            .map_err(|e| format!("wayland egl: {e}"))?;
        let target = unsafe {
            self.egl
                .create_window_surface(self.display, self.config, window.ptr() as egl::NativeWindowType, None)
        }
        .map_err(|e| format!("egl: {e}"))?;
        self.window = Some((window, target, pixels));
        Ok(())
    }

    pub fn set_pace(&mut self, pace: f64) {
        let now = Instant::now();
        let (then, at) = self.clock;
        self.clock = (then + now.duration_since(at).as_secs_f64() * self.pace, now);
        self.pace = pace;
    }

    pub fn arrived(&mut self) {
        self.waiting = false;
    }

    pub fn moving(&self) -> bool {
        self.pace > 0.0
    }

    pub fn draw<State: 'static>(&mut self, qh: &QueueHandle<State>) -> Result<(), String>
    where
        State: wayland_client::Dispatch<wayland_client::protocol::wl_callback::WlCallback, WlSurface>,
    {
        let Some((_, target, pixels)) = self.window.as_ref() else {
            return Ok(());
        };
        let (target, pixels) = (*target, *pixels);
        if self.waiting && self.moving() {
            return Ok(());
        }
        self.egl
            .make_current(self.display, Some(target), Some(target), Some(self.context))
            .map_err(|e| format!("egl: {e}"))?;
        let _ = self.egl.swap_interval(self.display, 0);
        let now = Instant::now();
        let (then, at) = self.clock;
        let clock = then + now.duration_since(at).as_secs_f64() * self.pace;
        self.clock = (clock, now);
        let sky = drifter_core::shader::sky(drifter_core::shader::hour_now());
        unsafe {
            let gl = &self.gl;
            gl.viewport(0, 0, pixels, pixels);
            gl.clear_color(0.0, 0.0, 0.0, 0.0);
            gl.clear(glow::COLOR_BUFFER_BIT);
            gl.use_program(Some(self.program));
            let at = |name: &str| gl.get_uniform_location(self.program, name);
            gl.uniform_1_f32(at("T").as_ref(), clock as f32);
            gl.uniform_1_f32(at("L").as_ref(), sky.light as f32);
            gl.uniform_1_f32(at("alpha").as_ref(), 1.0);
            gl.uniform_2_f32(at("W").as_ref(), pixels as f32, pixels as f32);
            gl.uniform_2_f32(at("size").as_ref(), pixels as f32, pixels as f32);
            for (name, value) in sky.uniforms() {
                gl.uniform_3_f32(at(name).as_ref(), value[0], value[1], value[2]);
            }
            gl.draw_arrays(glow::TRIANGLES, 0, 3);
        }
        if self.moving() {
            self.surface.frame(qh, self.surface.clone());
            self.waiting = true;
        }
        self.surface.damage_buffer(0, 0, pixels, pixels);
        self.egl.swap_buffers(self.display, target).map_err(|e| format!("egl: {e}"))?;
        Ok(())
    }
}

impl Drop for Logo {
    fn drop(&mut self) {
        let _ = self.egl.make_current(self.display, None, None, Some(self.context));
        unsafe { self.gl.delete_program(self.program) };
        if let Some((window, target, _)) = self.window.take() {
            let _ = self.egl.make_current(self.display, None, None, None);
            let _ = self.egl.destroy_surface(self.display, target);
            drop(window);
        }
        let _ = self.egl.make_current(self.display, None, None, None);
        let _ = self.egl.destroy_context(self.display, self.context);
        self.subsurface.destroy();
        self.surface.destroy();
    }
}

unsafe fn build(gl: &glow::Context) -> Result<glow::Program, String> {
    unsafe {
        let program = gl.create_program()?;
        let fragment = drifter_core::welcome::logo_source();
        let mut shaders = Vec::new();
        for (kind, text) in [(glow::VERTEX_SHADER, VERTEX), (glow::FRAGMENT_SHADER, fragment.as_str())] {
            let shader = gl.create_shader(kind)?;
            gl.shader_source(shader, text);
            gl.compile_shader(shader);
            if !gl.get_shader_compile_status(shader) {
                return Err(format!("the logo shader does not compile: {}", gl.get_shader_info_log(shader)));
            }
            gl.attach_shader(program, shader);
            shaders.push(shader);
        }
        gl.bind_attrib_location(program, 0, "a");
        gl.link_program(program);
        if !gl.get_program_link_status(program) {
            return Err(format!("the logo shader does not link: {}", gl.get_program_info_log(program)));
        }
        for shader in shaders {
            gl.detach_shader(program, shader);
            gl.delete_shader(shader);
        }
        let buffer = gl.create_buffer()?;
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(buffer));
        let corners: [f32; 6] = [-1.0, -1.0, 3.0, -1.0, -1.0, 3.0];
        let bytes = std::slice::from_raw_parts(corners.as_ptr() as *const u8, std::mem::size_of_val(&corners));
        gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytes, glow::STATIC_DRAW);
        gl.enable_vertex_attrib_array(0);
        gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, 0, 0);
        Ok(program)
    }
}
