//! Direct host-to-guest native bridges (M3), bypassing svc 0x32 IPC marshaling.

use recomp_abi::Context;
use zakuro_common::result::errors;
use zakuro_cpu::Bus;

use crate::kernel::object::KObject;
use crate::recompiled::system_from_ctx;
use crate::services::{fs, gsp};

/// Direct replacement for `gsp::Gpu#0x000C` (TriggerCmdReqQueue).
///
/// # Safety
/// `ctx` must be a valid `recomp_abi::Context` whose `user` points to a live `Machine`.
#[no_mangle]
pub unsafe extern "C" fn albw_gsp_trigger_cmd_queue(ctx: *mut Context) -> u32 {
    let system = unsafe { system_from_ctx(ctx) };
    if !gsp::outdrawn(system) {
        gsp::process_command_queue(system);
    }
    0
}

/// Direct replacement for `FSFile#0x0802` (Read).
///
/// # Safety
/// `ctx` must be a valid `recomp_abi::Context` whose `user` points to a live `Machine`.
#[no_mangle]
pub unsafe extern "C" fn albw_fs_read(
    ctx: *mut Context,
    handle: u32,
    out_bytes_ptr: u32,
    offset: u64,
    size: u32,
    dest: u32,
) -> u32 {
    let system = unsafe { system_from_ctx(ctx) };
    let Some(object) = system.kernel.handles.resolve(handle) else {
        return errors::INVALID_HANDLE.0;
    };
    let Some(KObject::ClientSession(session)) = system.kernel.objects.get(object) else {
        return errors::INVALID_HANDLE.0;
    };
    if session.service != "FSFile" {
        return errors::INVALID_HANDLE.0;
    }
    let file_id = session.subhandle;
    let read = fs::read_file(system, file_id, offset, size, dest);
    if out_bytes_ptr != 0 {
        system.memory.write32(out_bytes_ptr, read);
    }
    0
}

/// Direct replacement for `FSFile#0x0808` (Close).
///
/// # Safety
/// `ctx` must be a valid `recomp_abi::Context` whose `user` points to a live `Machine`.
#[no_mangle]
pub unsafe extern "C" fn albw_fs_close(ctx: *mut Context, handle: u32) -> u32 {
    let system = unsafe { system_from_ctx(ctx) };
    let Some(object) = system.kernel.handles.resolve(handle) else {
        return errors::INVALID_HANDLE.0;
    };
    let Some(KObject::ClientSession(session)) = system.kernel.objects.get(object) else {
        return errors::INVALID_HANDLE.0;
    };
    if session.service != "FSFile" {
        return errors::INVALID_HANDLE.0;
    }
    let file_id = session.subhandle;
    system.services.fs.files.remove(&file_id);
    0
}

/// Returns the current top screen width and 3D FOV scale configured by the PC UI.
///
/// # Safety
/// `ctx` must be a valid `recomp_abi::Context` whose `user` points to a live `Machine`.
#[no_mangle]
pub unsafe extern "C" fn albw_get_view_params(
    ctx: *mut Context,
    out_top_width: *mut f32,
    out_fov_scale: *mut f32,
) {
    let system = unsafe { system_from_ctx(ctx) };
    let (top_width, fov_scale) = system.gpu.view_params();
    if !out_top_width.is_null() {
        unsafe { *out_top_width = top_width; }
    }
    if !out_fov_scale.is_null() {
        unsafe { *out_fov_scale = fov_scale; }
    }
}
