//! Compiles Dear ImGui (from ../../../imgui) and our C++ bridge, plus Windows stack size.

fn main() {
    let manifest_dir = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let imgui_dir = manifest_dir.join("../../../imgui");

    println!("cargo:rerun-if-changed=src/imgui_bridge.cpp");
    println!("cargo:rerun-if-changed={}", imgui_dir.join("imgui.h").display());

    cc::Build::new()
        .cpp(true)
        .std("c++17")
        .include(&imgui_dir)
        .define("ImDrawIdx", Some("unsigned int"))
        .file(imgui_dir.join("imgui.cpp"))
        .file(imgui_dir.join("imgui_draw.cpp"))
        .file(imgui_dir.join("imgui_tables.cpp"))
        .file(imgui_dir.join("imgui_widgets.cpp"))
        .file("src/imgui_bridge.cpp")
        .warnings(false)
        .compile("albw_imgui");

    let windows = std::env::var("CARGO_CFG_TARGET_FAMILY").is_ok_and(|family| family == "windows");
    if windows {
        let stack = 8 << 20;
        match std::env::var("CARGO_CFG_TARGET_ENV").as_deref() {
            Ok("msvc") => println!("cargo:rustc-link-arg-bins=/STACK:{stack}"),
            _ => println!("cargo:rustc-link-arg-bins=-Wl,--stack,{stack}"),
        }
    }
}
