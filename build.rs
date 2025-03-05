use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=src/ffi.rs");
    println!("cargo:rerun-if-changed=cbindgen.toml");

    // Only generate bindings for library targets or if requested
    if env::var("CARGO_FEATURE_FFI").is_ok() || cfg!(feature = "generate-bindings") {
        let crate_dir = env::var("CARGO_MANIFEST_DIR").unwrap();

        // Create a new cbindgen Builder
        let cbindgen = cbindgen::Builder::new();

        // Load config from cbindgen.toml if it exists
        let mut config = cbindgen::Config::default();
        let config_path = PathBuf::from(&crate_dir).join("cbindgen.toml");
        if config_path.exists() {
            config = match cbindgen::Config::from_file(config_path) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("Error reading cbindgen.toml: {}", e);
                    config
                }
            };
        }

        // Generate bindings
        let bindings = cbindgen
            .with_config(config)
            .with_crate(crate_dir.clone())
            .with_language(cbindgen::Language::C)
            .generate();

        match bindings {
            Ok(bindings) => {
                // Create include directory
                let include_dir = PathBuf::from(&crate_dir).join("include");
                std::fs::create_dir_all(&include_dir).unwrap();

                // Write the bindings to the file
                let file_path = include_dir.join("openaws_vpn_client.h");
                // write_to_file returns a boolean, not a Result
                if bindings.write_to_file(file_path.clone()) {
                    println!("Generated C bindings at: {}", file_path.display());
                } else {
                    eprintln!("Failed to write bindings to {}", file_path.display());
                }
            }
            Err(e) => eprintln!("Error generating bindings: {}", e),
        }
    }

    // Platform-specific configuration
    if let Ok(target_os) = env::var("CARGO_CFG_TARGET_OS") {
        match target_os.as_str() {
            "android" => {
                println!("cargo:rustc-link-lib=c++_shared");
                println!("cargo:rustc-link-lib=log");
            }
            "macos" | "ios" => {
                println!("cargo:rustc-link-lib=framework=Security");
                println!("cargo:rustc-link-lib=framework=CoreFoundation");
            }
            _ => {}
        }
    }
}
