fn main() {
    // Tell the linker where to find libcactus.dylib.
    // The dylib lives in <repo>/libs/ alongside the Tauri source.
    let cactus_lib_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../libs");
    let cactus_lib_dir = cactus_lib_dir
        .canonicalize()
        .expect("libs/ directory not found -- place libcactus.dylib in sentinel/libs/");

    println!("cargo:rustc-link-search=native={}", cactus_lib_dir.display());
    println!("cargo:rustc-link-lib=dylib=cactus");

    // At runtime the dylib must be found. On macOS we embed an rpath so the
    // binary can locate it without DYLD_LIBRARY_PATH.
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}", cactus_lib_dir.display());

    tauri_build::build()
}
