fn main() {
    // 改了图标要重新编进 exe（tauri_build 只盯 tauri.conf.json 等，不盯 icons/）。
    println!("cargo:rerun-if-changed=icons");
    tauri_build::build()
}
