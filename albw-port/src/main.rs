//! CTRJack, recompiled ahead of time by 3dsrecomp and run by Zakuro. it
//! still reads the game for everything but the code, so it takes the path
//! to a copy of it the way Zakuro does.

use std::ffi::c_void;
use std::ptr::addr_of;

extern "C" {
    static recomp_abi: u32;
    static recomp_generation: u32;
    static recomp_entry_count: u32;
    static recomp_entries: u8;
    static recomp_module_count: u32;
    static recomp_modules: u8;
    static recomp_origins: u8;
}

fn main() {
    // SAFETY: the tables 3dsrecomp wrote beside the code build.rs links in
    let code = unsafe {
        zakuro::Linked::new(
            0x00040000000EC300,
            recomp_abi,
            addr_of!(recomp_entries) as *const c_void,
            recomp_entry_count,
            addr_of!(recomp_modules) as *const c_void,
            recomp_module_count,
        )
        // so that functions a mod or an update changed run in the interpreter
        .with_origins(recomp_generation, addr_of!(recomp_origins) as *const c_void)
    };
    zakuro::run(Some(code));
}
