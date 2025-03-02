use std::env;
use std::path::PathBuf;

fn main() {
    let crate_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let package_name = env::var("CARGO_PKG_NAME").unwrap();
    let output_dir = target_dir().join("include");
    std::fs::create_dir_all(&output_dir).unwrap();

    let output_file = output_dir.join(format!("{}.h", package_name));

    cbindgen::Builder::new()
        .with_crate(crate_dir)
        .generate()
        .expect("Unable to generate bindings")
        .write_to_file(output_file);

    println!("cargo:rerun-if-changed=src/ffi.rs");
    println!("cargo:rerun-if-changed=src/lib.rs");
}

fn target_dir() -> PathBuf {
    if let Ok(target_dir) = env::var("CARGO_TARGET_DIR") {
        PathBuf::from(target_dir)
    } else {
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("target")
    }
}
