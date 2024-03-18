use std::path::{Path, PathBuf};

fn get_output_path() -> PathBuf {
    let manifest_dir_string = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let build_type = std::env::var("PROFILE").unwrap();

    Path::new(&manifest_dir_string)
        .join("target")
        .join(build_type)
}

fn main() {
    // let out_dir = std::env::var("OUT_DIR").unwrap();
    let target_dir = get_output_path();
    let src = Path::join(&std::env::current_dir().unwrap(), "config.yaml");
    let dest = Path::join(Path::new(&target_dir), Path::new("config.yaml"));
    std::fs::copy(src, dest).unwrap();
}
