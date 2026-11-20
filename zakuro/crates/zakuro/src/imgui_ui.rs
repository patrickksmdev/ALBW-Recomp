//! Rust wrapper around Dear ImGui C++ bridge (`src/imgui_bridge.cpp`).

use std::time::Instant;

use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::window::Window;
use zakuro_gpu::{Overlay, OverlayMesh, OverlayTexture, OverlayVertex};

const FONT_ROBOTO_MEDIUM: &[u8] = include_bytes!("../../../../imgui/misc/fonts/Roboto-Medium.ttf");
const IMGUI_FONT_TEXTURE_ID: u64 = 0x1000_0001;

#[repr(C)]
pub struct ImGuiMenuState {
    pub display_w: f32,
    pub display_h: f32,
    pub dpi_scale: f32,
    pub delta_time: f32,
    pub menu_open: bool,
    pub show_fps: bool,
    pub current_fps: f32,
    pub slot: i32,
    pub slot_has_save: bool,
    pub resolution: i32,
    pub scale: i32,
    pub layout: i32,
    pub top_shape: i32,
    pub fov_scale: f32,
    pub filter: i32,
    pub integer_scale: bool,
    pub hardware_rasterizer: bool,
    pub frame_interpolation: bool,
    pub volume: f32,
    pub mute: bool,
    pub actions: u32,
}

#[repr(C)]
struct ImGuiBridgeVertex {
    x: f32,
    y: f32,
    u: f32,
    v: f32,
    r: u8,
    g: u8,
    b: u8,
    a: u8,
}

#[repr(C)]
struct ImGuiBridgeDrawCmd {
    clip_x1: u32,
    clip_y1: u32,
    clip_x2: u32,
    clip_y2: u32,
    vtx_offset: u32,
    vtx_count: u32,
    idx_offset: u32,
    idx_count: u32,
}

extern "C" {
    fn albw_imgui_init(font_data: *const u8, font_size: u32, dpi_scale: f32);
    fn albw_imgui_mouse_pos(x: f32, y: f32);
    fn albw_imgui_mouse_button(button: i32, down: bool);
    fn albw_imgui_mouse_wheel(dx: f32, dy: f32);
    fn albw_imgui_wants_mouse() -> bool;
    fn albw_imgui_take_font_atlas(out_pixels: *mut *const u8, out_w: *mut i32, out_h: *mut i32) -> bool;
    fn albw_imgui_frame(st: *mut ImGuiMenuState);
    fn albw_imgui_cmd_count() -> u32;
    fn albw_imgui_cmds() -> *const ImGuiBridgeDrawCmd;
    fn albw_imgui_vertices() -> *const ImGuiBridgeVertex;
    fn albw_imgui_indices() -> *const u32;
}

pub struct ImGuiUi {
    last_frame: Instant,
}

impl ImGuiUi {
    pub fn new(window: &Window) -> Self {
        let dpi = window.scale_factor() as f32;
        unsafe {
            albw_imgui_init(FONT_ROBOTO_MEDIUM.as_ptr(), FONT_ROBOTO_MEDIUM.len() as u32, dpi);
        }
        Self { last_frame: Instant::now() }
    }

    pub fn event(&self, window: &Window, event: &WindowEvent) -> bool {
        let scale = window.scale_factor() as f32;
        unsafe {
            match event {
                WindowEvent::CursorMoved { position, .. } => {
                    albw_imgui_mouse_pos(position.x as f32 / scale, position.y as f32 / scale);
                }
                WindowEvent::MouseInput { state, button, .. } => {
                    let btn = match button {
                        MouseButton::Left => 0,
                        MouseButton::Right => 1,
                        MouseButton::Middle => 2,
                        _ => return false,
                    };
                    albw_imgui_mouse_button(btn, *state == ElementState::Pressed);
                }
                WindowEvent::MouseWheel { delta, .. } => {
                    let (dx, dy) = match delta {
                        MouseScrollDelta::LineDelta(x, y) => (*x, *y),
                        MouseScrollDelta::PixelDelta(p) => (p.x as f32 / 16.0, p.y as f32 / 16.0),
                    };
                    albw_imgui_mouse_wheel(dx, dy);
                }
                _ => return false,
            }
            albw_imgui_wants_mouse()
        }
    }

    pub fn wants_pointer(&self) -> bool {
        unsafe { albw_imgui_wants_mouse() }
    }

    pub fn render(&mut self, window: &Window, state: &mut ImGuiMenuState, overlay: &mut Overlay) {
        let now = Instant::now();
        let dt = (now - self.last_frame).as_secs_f32().clamp(0.001, 0.1);
        self.last_frame = now;

        let size = window.inner_size();
        state.display_w = size.width as f32;
        state.display_h = size.height as f32;
        state.dpi_scale = window.scale_factor() as f32;
        state.delta_time = dt;

        unsafe {
            albw_imgui_frame(state as *mut ImGuiMenuState);

            let mut font_ptr: *const u8 = std::ptr::null();
            let mut font_w: i32 = 0;
            let mut font_h: i32 = 0;
            if albw_imgui_take_font_atlas(&mut font_ptr, &mut font_w, &mut font_h) && !font_ptr.is_null() {
                let len = (font_w as usize) * (font_h as usize) * 4;
                let pixels = std::slice::from_raw_parts(font_ptr, len).to_vec();
                overlay.textures.push(OverlayTexture {
                    id: IMGUI_FONT_TEXTURE_ID,
                    offset: None,
                    size: [font_w as u32, font_h as u32],
                    pixels,
                    linear: true,
                });
            }

            let cmd_count = albw_imgui_cmd_count() as usize;
            if cmd_count == 0 {
                return;
            }
            let cmds = std::slice::from_raw_parts(albw_imgui_cmds(), cmd_count);
            let verts_ptr = albw_imgui_vertices();
            let idxs_ptr = albw_imgui_indices();

            for cmd in cmds {
                let v_slice = std::slice::from_raw_parts(verts_ptr.add(cmd.vtx_offset as usize), cmd.vtx_count as usize);
                let i_slice = std::slice::from_raw_parts(idxs_ptr.add(cmd.idx_offset as usize), cmd.idx_count as usize);
                let vertices = v_slice
                    .iter()
                    .map(|v| OverlayVertex {
                        position: [v.x, v.y],
                        uv: [v.u, v.v],
                        color: [v.r, v.g, v.b, v.a],
                    })
                    .collect();
                overlay.meshes.push(OverlayMesh {
                    texture: IMGUI_FONT_TEXTURE_ID,
                    clip: [cmd.clip_x1, cmd.clip_y1, cmd.clip_x2, cmd.clip_y2],
                    vertices,
                    indices: i_slice.to_vec(),
                });
            }
        }
    }
}
