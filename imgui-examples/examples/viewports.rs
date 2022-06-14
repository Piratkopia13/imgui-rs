use std::{collections::HashMap, mem::size_of};

use glow::HasContext;
use glutin::{event::WindowEvent, event_loop::ControlFlow, PossiblyCurrent};
use imgui::{BackendFlags, ConfigFlags, DrawData, DrawVert, ViewportFlags};

fn main() {
    let event_loop = glutin::event_loop::EventLoop::new();

    let mut imgui = imgui::Context::create();
    imgui
        .io_mut()
        .backend_flags
        .insert(BackendFlags::RENDERER_HAS_VIEWPORTS);
    imgui
        .io_mut()
        .config_flags
        .insert(ConfigFlags::DOCKING_ENABLE);
    imgui
        .io_mut()
        .config_flags
        .insert(ConfigFlags::VIEWPORTS_ENABLE);

    imgui.set_ini_filename(None);

    let (mut main_viewport, context) = Viewport::new(&event_loop);
    main_viewport.init_font_texture(&mut imgui, &context);

    let mut winit_platform = imgui_winit_support::WinitPlatform::init(&mut imgui);
    imgui_winit_support::WinitPlatform::init_viewports(
        &mut imgui,
        main_viewport.window(),
        &event_loop,
    );

    let mut viewports = HashMap::new();

    event_loop.run(move |event, window_target, control_flow| {
        winit_platform.handle_event(imgui.io_mut(), main_viewport.window(), &event);

        let mut storage = ViewportStorage {
            window_target,
            context: &context,
            main_viewport: &main_viewport,
            viewports: &mut viewports,
        };
        winit_platform.handle_viewport_event(
            &mut imgui,
            main_viewport.window(),
            &mut storage,
            &event,
        );

        match event {
            glutin::event::Event::WindowEvent {
                window_id,
                event: WindowEvent::CloseRequested,
            } => {
                if window_id == main_viewport.window().id() {
                    *control_flow = ControlFlow::Exit;
                }
            }
            glutin::event::Event::MainEventsCleared => {
                main_viewport.window().request_redraw();
            }
            glutin::event::Event::RedrawRequested(window_id) => {
                if window_id == main_viewport.window().id() {
                    winit_platform
                        .prepare_frame(imgui.io_mut(), main_viewport.window())
                        .unwrap();

                    render(&mut imgui);

                    imgui.update_platform_windows();

                    let mut storage = ViewportStorage {
                        window_target,
                        context: &context,
                        main_viewport: &main_viewport,
                        viewports: &mut viewports,
                    };
                    winit_platform.update_viewports(&mut imgui, &mut storage);

                    let main_draw_data = imgui.render();
                    render_viewport(&mut main_viewport, main_draw_data, &context);

                    for (id, viewport) in &mut viewports {
                        viewport.init_font_texture(&mut imgui, &context);

                        let draw_data = imgui.viewport_by_id(*id).unwrap().draw_data();
                        render_viewport(viewport, draw_data, &context);
                    }
                }
            }
            _ => {}
        }
    });
}

fn render(imgui: &mut imgui::Context) {
    let ui = imgui.new_frame();

    ui.dockspace_over_main_viewport();

    let mut open = true;
    ui.show_demo_window(&mut open);

    ui.end_frame_early();
}

fn render_viewport(viewport: &mut Viewport, draw_data: &DrawData, context: &glow::Context) {
    let pos = viewport.window().inner_position().unwrap();

    unsafe {
        viewport.make_current();

        context.disable(glow::SCISSOR_TEST);
        context.clear(glow::COLOR_BUFFER_BIT);
        context.enable(glow::SCISSOR_TEST);

        context.bind_vertex_array(Some(viewport.vao));
        context.use_program(Some(viewport.shader));

        let left = draw_data.display_pos[0];
        let right = draw_data.display_pos[0] + draw_data.display_size[0];
        let top = draw_data.display_pos[1];
        let bottom = draw_data.display_pos[1] + draw_data.display_size[1];
        let matrix = [
            (2.0 / (right - left)),
            0.0,
            0.0,
            0.0,
            0.0,
            (2.0 / (top - bottom)),
            0.0,
            0.0,
            0.0,
            0.0,
            -1.0,
            0.0,
            (right + left) / (left - right),
            (top + bottom) / (bottom - top),
            0.0,
            1.0,
        ];

        let loc = context
            .get_uniform_location(viewport.shader, "u_Matrix")
            .unwrap();
        context.uniform_matrix_4_f32_slice(Some(&loc), false, &matrix);

        context.blend_func_separate(
            glow::SRC_ALPHA,
            glow::ONE_MINUS_SRC_ALPHA,
            glow::ONE,
            glow::ZERO,
        );
        context.enable(glow::BLEND);

        context.bind_buffer(glow::ARRAY_BUFFER, Some(viewport.vbo));
        context.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(viewport.ibo));

        context.bind_texture(glow::TEXTURE_2D, viewport.font_tex);

        context.viewport(
            0,
            0,
            viewport.window().inner_size().width as i32,
            viewport.window().inner_size().height as i32,
        );
    }

    for draw_list in draw_data.draw_lists() {
        unsafe {
            context.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                std::slice::from_raw_parts(
                    draw_list.vtx_buffer().as_ptr() as *const u8,
                    draw_list.vtx_buffer().len() * size_of::<DrawVert>(),
                ),
                glow::STREAM_DRAW,
            );
            context.buffer_data_u8_slice(
                glow::ELEMENT_ARRAY_BUFFER,
                std::slice::from_raw_parts(
                    draw_list.idx_buffer().as_ptr() as *const u8,
                    draw_list.idx_buffer().len() * size_of::<u16>(),
                ),
                glow::STREAM_DRAW,
            );
            context.bind_vertex_buffer(0, Some(viewport.vbo), 0, size_of::<DrawVert>() as i32);
        }

        for cmd in draw_list.commands() {
            if let imgui::DrawCmd::Elements { count, cmd_params } = cmd {
                unsafe {
                    let window_height = viewport.window().inner_size().height as i32;

                    let x = cmd_params.clip_rect[0] as i32 - pos.x;
                    let y = cmd_params.clip_rect[1] as i32 - pos.y;
                    let width = (cmd_params.clip_rect[2] - cmd_params.clip_rect[0]) as i32;
                    let height = (cmd_params.clip_rect[3] - cmd_params.clip_rect[1]) as i32;

                    context.scissor(x, window_height - (y + height), width, height);
                    context.enable(glow::SCISSOR_TEST);
                    context.draw_elements_base_vertex(
                        glow::TRIANGLES,
                        count as i32,
                        glow::UNSIGNED_SHORT,
                        (cmd_params.idx_offset * size_of::<u16>()) as i32,
                        cmd_params.vtx_offset as i32,
                    );
                }
            }
        }
    }

    viewport.swap_buffers();
}

struct ViewportStorage<'a, T: 'static> {
    window_target: &'a glutin::event_loop::EventLoopWindowTarget<T>,
    main_viewport: &'a Viewport,
    context: &'a glow::Context,
    viewports: &'a mut HashMap<imgui::Id, Viewport>,
}

impl<'a, T: 'static> imgui_winit_support::WinitPlatformViewportStorage for ViewportStorage<'a, T> {
    fn create_window(&mut self, id: imgui::Id, flags: imgui::ViewportFlags) {
        let viewport =
            Viewport::new_shared(self.window_target, self.main_viewport, self.context, flags);
        self.viewports.insert(id, viewport);
    }

    fn remove_windows(&mut self, filter: impl Fn(imgui::Id) -> bool) {
        self.viewports.retain(|id, _| !filter(*id));
    }

    fn get_window(
        &mut self,
        id: glutin::window::WindowId,
    ) -> Option<(imgui::Id, &glutin::window::Window)> {
        self.viewports
            .iter()
            .find(|(_, viewport)| viewport.window().id() == id)
            .map(|(viewport_id, viewport)| (*viewport_id, viewport.window()))
    }

    fn for_each(&mut self, mut func: impl FnMut(imgui::Id, &glutin::window::Window)) {
        self.viewports.iter().for_each(|(id, vp)| {
            func(*id, vp.window());
        });
    }
}

struct Viewport {
    window: Option<glutin::ContextWrapper<PossiblyCurrent, glutin::window::Window>>,
    vao: glow::VertexArray,
    vbo: glow::Buffer,
    ibo: glow::Buffer,
    shader: glow::Program,
    font_tex: Option<glow::Texture>,
}

impl Viewport {
    fn new<T>(event_loop: &glutin::event_loop::EventLoopWindowTarget<T>) -> (Self, glow::Context) {
        let wb = glutin::window::WindowBuilder::new()
            .with_inner_size(glutin::dpi::LogicalSize::new(800.0, 600.0))
            .with_resizable(true)
            .with_title("Viewports")
            .with_visible(true)
            .with_decorations(true);
        let window = unsafe {
            glutin::ContextBuilder::new()
                .with_double_buffer(Some(true))
                .with_vsync(true)
                .build_windowed(wb, event_loop)
                .unwrap()
                .make_current()
                .unwrap()
        };

        let context = unsafe {
            glow::Context::from_loader_function(|s| window.get_proc_address(s) as *const _)
        };

        let (vao, vbo, ibo, shader) = unsafe {
            let vao = context.create_vertex_array().unwrap();
            let vbo = context.create_buffer().unwrap();
            let ibo = context.create_buffer().unwrap();

            context.bind_vertex_array(Some(vao));
            context.vertex_attrib_binding(0, 0);
            context.vertex_attrib_binding(1, 0);
            context.vertex_attrib_binding(2, 0);
            context.vertex_attrib_format_f32(0, 2, glow::FLOAT, false, 0);
            context.vertex_attrib_format_f32(1, 2, glow::FLOAT, false, 8);
            context.vertex_attrib_format_f32(2, 4, glow::UNSIGNED_BYTE, true, 16);
            context.enable_vertex_attrib_array(0);
            context.enable_vertex_attrib_array(1);
            context.enable_vertex_attrib_array(2);
            context.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(ibo));
            context.bind_vertex_array(None);

            let vertex_shader = context.create_shader(glow::VERTEX_SHADER).unwrap();
            context.shader_source(vertex_shader, VERTEX_SHADER);
            context.compile_shader(vertex_shader);

            let fragment_shader = context.create_shader(glow::FRAGMENT_SHADER).unwrap();
            context.shader_source(fragment_shader, FRAGMENT_SHADER);
            context.compile_shader(fragment_shader);

            let program = context.create_program().unwrap();
            context.attach_shader(program, vertex_shader);
            context.attach_shader(program, fragment_shader);
            context.link_program(program);

            context.delete_shader(vertex_shader);
            context.delete_shader(fragment_shader);

            (vao, vbo, ibo, program)
        };

        (
            Self {
                window: Some(window),
                vao,
                vbo,
                ibo,
                shader,
                font_tex: None,
            },
            context,
        )
    }

    fn new_shared<T>(
        event_loop: &glutin::event_loop::EventLoopWindowTarget<T>,
        main_viewport: &Viewport,
        context: &glow::Context,
        flags: imgui::ViewportFlags,
    ) -> Self {
        let wb = glutin::window::WindowBuilder::new()
            .with_inner_size(glutin::dpi::LogicalSize::new(100.0, 100.0))
            .with_resizable(true)
            .with_title("<unnamed>")
            .with_always_on_top(flags.contains(ViewportFlags::TOP_MOST))
            .with_decorations(!flags.contains(ViewportFlags::NO_DECORATION))
            .with_visible(false);
        let window = unsafe {
            glutin::ContextBuilder::new()
                .with_double_buffer(Some(true))
                .with_vsync(true)
                .with_shared_lists(main_viewport.window.as_ref().unwrap().context())
                .build_windowed(wb, event_loop)
                .unwrap()
                .make_current()
                .unwrap()
        };

        let (vao, vbo, ibo, shader) = unsafe {
            let vao = context.create_vertex_array().unwrap();
            let vbo = context.create_buffer().unwrap();
            let ibo = context.create_buffer().unwrap();

            context.bind_vertex_array(Some(vao));
            context.vertex_attrib_binding(0, 0);
            context.vertex_attrib_binding(1, 0);
            context.vertex_attrib_binding(2, 0);
            context.vertex_attrib_format_f32(0, 2, glow::FLOAT, false, 0);
            context.vertex_attrib_format_f32(1, 2, glow::FLOAT, false, 8);
            context.vertex_attrib_format_f32(2, 4, glow::UNSIGNED_BYTE, true, 16);
            context.enable_vertex_attrib_array(0);
            context.enable_vertex_attrib_array(1);
            context.enable_vertex_attrib_array(2);
            context.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(ibo));
            context.bind_vertex_array(None);

            (vao, vbo, ibo, main_viewport.shader)
        };

        Self {
            window: Some(window),
            vao,
            vbo,
            ibo,
            shader,
            font_tex: None,
        }
    }

    fn init_font_texture(&mut self, imgui: &mut imgui::Context, context: &glow::Context) {
        if self.font_tex.is_some() {
            return;
        }

        unsafe {
            self.make_current();

            let font_tex = context.create_texture().unwrap();

            let data = imgui.fonts().build_rgba32_texture();
            context.bind_texture(glow::TEXTURE_2D, Some(font_tex));
            context.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGBA as i32,
                data.width as i32,
                data.height as i32,
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                Some(data.data),
            );
            context.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MAG_FILTER,
                glow::LINEAR as i32,
            );
            context.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MIN_FILTER,
                glow::LINEAR as i32,
            );
            context.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_S,
                glow::CLAMP_TO_EDGE as i32,
            );
            context.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_T,
                glow::CLAMP_TO_EDGE as i32,
            );

            self.font_tex = Some(font_tex);
        }
    }

    fn window(&self) -> &glutin::window::Window {
        self.window.as_ref().unwrap().window()
    }

    fn make_current(&mut self) {
        let window = self.window.take().unwrap();
        self.window = unsafe { Some(window.make_current().unwrap()) };
    }

    fn swap_buffers(&self) {
        self.window.as_ref().unwrap().swap_buffers().unwrap();
    }
}

const VERTEX_SHADER: &str = "#version 450 core

layout(location = 0) in vec2 in_Position;
layout(location = 1) in vec2 in_UV;
layout(location = 2) in vec4 in_Color;

out vec2 v2f_UV;
out vec4 v2f_Color;

uniform mat4 u_Matrix;

void main() {
    gl_Position = u_Matrix * vec4(in_Position, 0.0, 1.0);
    v2f_UV = in_UV;
    v2f_Color = in_Color;
}

";

const FRAGMENT_SHADER: &str = "#version 450 core

in vec2 v2f_UV;
in vec4 v2f_Color;

layout(location = 0) uniform sampler2D u_FontTexture;

out vec4 out_Color;

void main() {
    vec4 texColor = texture(u_FontTexture, v2f_UV);
    vec4 finalColor = texColor * v2f_Color;

    out_Color = finalColor;
}

";
