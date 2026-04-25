use std::{
    env,
    fs,
    path::{Path, PathBuf},
};

const OPENCV_DLL_NAME: &str = "opencv_world490.dll";

fn main() {
    println!("cargo:rerun-if-changed=app.manifest.xml");
    println!("cargo:rerun-if-changed=src/opencv_shim.cpp");
    println!("cargo:rerun-if-env-changed=OPENCV_DIR");
    println!("cargo:rerun-if-env-changed=OPENCV_INCLUDE_DIR");
    println!("cargo:rerun-if-env-changed=OPENCV_LIB_DIR");
    println!("cargo:rerun-if-env-changed=OPENCV_BIN_DIR");

    let windows =
        tauri_build::WindowsAttributes::new().app_manifest(include_str!("app.manifest.xml"));
    let attributes = tauri_build::Attributes::new().windows_attributes(windows);

    tauri_build::try_build(attributes).expect("failed to run Tauri build script");

    let opencv = resolve_opencv_paths().unwrap_or_else(|error| panic!("{error}"));

    cc::Build::new()
        .cpp(true)
        .file("src/opencv_shim.cpp")
        .include(&opencv.include_dir)
        .flag("/EHsc")
        .compile("opencv_shim");

    println!(
        "cargo:rustc-link-search=native={}",
        opencv.lib_dir.display()
    );
    println!("cargo:rustc-link-lib=dylib=opencv_world490");

    copy_opencv_dll(&opencv.bin_dir);
}

#[derive(Debug, Clone)]
struct OpenCvPaths {
    include_dir: PathBuf,
    lib_dir: PathBuf,
    bin_dir: PathBuf,
}

fn resolve_opencv_paths() -> Result<OpenCvPaths, String> {
    if let Some(paths) = resolve_from_env_vars()? {
        return Ok(paths);
    }

    let fallback_root = PathBuf::from(r"C:\tools\opencv\opencv\build");
    if let Some(paths) = resolve_from_root(&fallback_root) {
        return Ok(paths);
    }

    Err(format!(
        "未找到可用的 OpenCV 构建目录。\n\
         推荐做法：设置环境变量 `OPENCV_DIR` 指向 OpenCV build 目录，例如 `C:\\tools\\opencv\\opencv\\build`。\n\
         也可以分别设置 `OPENCV_INCLUDE_DIR`、`OPENCV_LIB_DIR`、`OPENCV_BIN_DIR`。\n\
         当前兜底路径也已尝试：{}",
        fallback_root.display()
    ))
}

fn resolve_from_env_vars() -> Result<Option<OpenCvPaths>, String> {
    let root = env::var_os("OPENCV_DIR").map(PathBuf::from);
    let include = env::var_os("OPENCV_INCLUDE_DIR").map(PathBuf::from);
    let lib = env::var_os("OPENCV_LIB_DIR").map(PathBuf::from);
    let bin = env::var_os("OPENCV_BIN_DIR").map(PathBuf::from);

    if let Some(root) = root {
        if include.is_some() || lib.is_some() || bin.is_some() {
            return Err(
                "请只使用 `OPENCV_DIR`，或同时使用 `OPENCV_INCLUDE_DIR` / `OPENCV_LIB_DIR` / `OPENCV_BIN_DIR`，不要混用。"
                    .to_string(),
            );
        }

        return resolve_from_root(&root)
            .map(Some)
            .ok_or_else(|| {
                format!(
                    "`OPENCV_DIR` 指向的目录无效：{}\n\
                     期望至少存在：`include`、`x64\\vc16\\lib`、`x64\\vc16\\bin`",
                    root.display()
                )
            });
    }

    if include.is_none() && lib.is_none() && bin.is_none() {
        return Ok(None);
    }

    let include = include.ok_or_else(|| {
        "已检测到 OpenCV 环境变量，但缺少 `OPENCV_INCLUDE_DIR`。".to_string()
    })?;
    let lib =
        lib.ok_or_else(|| "已检测到 OpenCV 环境变量，但缺少 `OPENCV_LIB_DIR`。".to_string())?;
    let bin =
        bin.ok_or_else(|| "已检测到 OpenCV 环境变量，但缺少 `OPENCV_BIN_DIR`。".to_string())?;

    validate_paths(OpenCvPaths {
        include_dir: include,
        lib_dir: lib,
        bin_dir: bin,
    })
    .map(Some)
}

fn resolve_from_root(root: &Path) -> Option<OpenCvPaths> {
    let candidates = [
        OpenCvPaths {
            include_dir: root.join("include"),
            lib_dir: root.join("x64").join("vc16").join("lib"),
            bin_dir: root.join("x64").join("vc16").join("bin"),
        },
        OpenCvPaths {
            include_dir: root.join("build").join("include"),
            lib_dir: root.join("build").join("x64").join("vc16").join("lib"),
            bin_dir: root.join("build").join("x64").join("vc16").join("bin"),
        },
    ];

    candidates.into_iter().find_map(|paths| validate_paths(paths).ok())
}

fn validate_paths(paths: OpenCvPaths) -> Result<OpenCvPaths, String> {
    ensure_dir(&paths.include_dir, "OpenCV include 目录")?;
    ensure_dir(&paths.lib_dir, "OpenCV lib 目录")?;
    ensure_dir(&paths.bin_dir, "OpenCV bin 目录")?;

    let dll_path = paths.bin_dir.join(OPENCV_DLL_NAME);
    if !dll_path.is_file() {
        return Err(format!("缺少 OpenCV DLL：{}", dll_path.display()));
    }

    Ok(paths)
}

fn ensure_dir(path: &Path, label: &str) -> Result<(), String> {
    if path.is_dir() {
        Ok(())
    } else {
        Err(format!("{label} 不存在：{}", path.display()))
    }
}

fn copy_opencv_dll(bin_dir: &Path) {
    let out_dir = env::var("OUT_DIR").expect("missing OUT_DIR");
    let target_dir = Path::new(&out_dir)
        .ancestors()
        .nth(3)
        .expect("failed to find target dir");

    let manifest_dir = env::var("CARGO_MANIFEST_DIR").expect("missing CARGO_MANIFEST_DIR");
    let bundled_dll = Path::new(&manifest_dir).join(OPENCV_DLL_NAME);
    let env_dll = bin_dir.join(OPENCV_DLL_NAME);
    let dll_src = if bundled_dll.is_file() {
        bundled_dll
    } else {
        env_dll
    };
    let dll_dst = target_dir.join(OPENCV_DLL_NAME);

    if let Err(error) = fs::copy(&dll_src, &dll_dst) {
        println!(
            "cargo:warning=failed to copy OpenCV DLL from {}: {}",
            dll_src.display(),
            error
        );
    }
}
