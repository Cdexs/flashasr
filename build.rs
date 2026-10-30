fn main() {
    // Qwen3-ASR 精解引擎适配层（运行时动态加载 llama.cpp 官方预编译库，头文件为同版本 b11207 官方头文件，
    // 见 native/llama_asr/；源自 voice-ime ACC-ENGINE-LLAMACPP-452）。
    // 🔴 必须显式声明 rerun-if-changed：漏声明 ⇒ 改了 shim.cpp 却链接旧 .lib。
    println!("cargo:rerun-if-changed=native/llama_asr/shim.cpp");
    println!("cargo:rerun-if-changed=native/llama_asr/include");
    // 程序图标（winres 编进 exe，托盘也用它）：改了图标要重新编进去。
    println!("cargo:rerun-if-changed=assets/icons/app.ico");
    cc::Build::new()
        .cpp(true)
        .file("native/llama_asr/shim.cpp")
        .include("native/llama_asr/include")
        .flag_if_supported("/std:c++17")
        .flag_if_supported("/EHsc")
        .flag_if_supported("/utf-8")
        .flag_if_supported("-std=c++17")
        .warnings(false)
        .emit_rerun_if_env_changed(false)
        .compile("llama_asr_shim");

    #[cfg(target_os = "windows")]
    {
        let version = std::env::var("CARGO_PKG_VERSION").unwrap_or_default();
        let parts: Vec<&str> = version.split('.').collect();
        let win_ver = format!(
            "{}.{}.{}.0",
            parts.first().unwrap_or(&"0"),
            parts.get(1).unwrap_or(&"0"),
            parts.get(2).unwrap_or(&"0")
        );
        let mut res = winres::WindowsResource::new();
        res.set_icon("assets/icons/app.ico");
        res.set("ProductName", "flashasr");
        res.set("FileDescription", "flashasr 识别服务");
        res.set("FileVersion", &win_ver);
        res.set("ProductVersion", &win_ver);
        res.compile().expect("winres compile failed");
    }

    #[cfg(target_os = "macos")]
    {
        println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path");
        println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path/../Frameworks");
    }
}
