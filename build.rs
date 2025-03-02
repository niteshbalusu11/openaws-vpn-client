use std::env;
use std::path::PathBuf;

fn main() {
    let crate_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let package_name = env::var("CARGO_PKG_NAME").unwrap();
    let output_file = target_dir()
        .join(format!("{}.h", package_name))
        .display()
        .to_string();

    cbindgen::Builder::new()
        .with_crate(crate_dir)
        .with_config(cbindgen::Config::from_file("cbindgen.toml").unwrap())
        .generate()
        .expect("Unable to generate bindings")
        .write_to_file(&output_file);

    println!("cargo:rerun-if-changed=src/ffi.rs");
    println!("cargo:rerun-if-changed=src/lib.rs");
    println!("cargo:rerun-if-changed=cbindgen.toml");
}

fn target_dir() -> PathBuf {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());

    if let Ok(target_dir) = env::var("CARGO_TARGET_DIR") {
        PathBuf::from(target_dir).join("include")
    } else {
        manifest_dir.join("target").join("include")
    }
}
