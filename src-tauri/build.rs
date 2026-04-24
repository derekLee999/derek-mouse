fn main() {
    println!("cargo:rerun-if-changed=app.manifest.xml");
    println!("cargo:rerun-if-changed=src/opencv_shim.cpp");

    let windows =
        tauri_build::WindowsAttributes::new().app_manifest(include_str!("app.manifest.xml"));
    let attributes = tauri_build::Attributes::new().windows_attributes(windows);

    tauri_build::try_build(attributes).expect("failed to run Tauri build script");

    // Compile OpenCV C++ shim
    cc::Build::new()
        .cpp(true)
        .file("src/opencv_shim.cpp")
        .include("C:/tools/opencv/opencv/build/include")
        .flag("/EHsc")
        .compile("opencv_shim");

    // Link OpenCV
    println!("cargo:rustc-link-search=native=C:/tools/opencv/opencv/build/x64/vc16/lib");
    println!("cargo:rustc-link-lib=dylib=opencv_world490");

    // Copy OpenCV DLL to target dir so the executable can find it at dev time
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let target_dir = std::path::Path::new(&out_dir)
        .ancestors()
        .nth(3)
        .expect("failed to find target dir");
    // Use the UPX-compressed DLL in the project dir if available
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let dll_src = std::path::Path::new(&manifest_dir).join("opencv_world490.dll");
    let dll_src = if dll_src.exists() {
        dll_src
    } else {
        std::path::PathBuf::from(r"C:\tools\opencv\opencv\build\x64\vc16\bin\opencv_world490.dll")
    };
    let dll_dst = target_dir.join("opencv_world490.dll");
    if let Err(e) = std::fs::copy(&dll_src, &dll_dst) {
        println!("cargo:warning=failed to copy OpenCV DLL from {}: {}", dll_src.display(), e);
    }
}
