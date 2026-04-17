#![allow(unsafe_code)]

use std::io::Result;
use std::path::{Path, PathBuf};

fn main() -> Result<()> {
    let protoc = protoc_bin_vendored::protoc_bin_path()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::NotFound, e.to_string()))?;
    // SAFETY: build.rs runs single-threaded before any other code touches env.
    unsafe {
        std::env::set_var("PROTOC", &protoc);
    }

    let root = PathBuf::from("vendor/meshtastic-protobufs");
    let files = collect_protos(&root)?;
    if files.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!(
                "no .proto files under {} — run `git submodule update --init --recursive`",
                root.display()
            ),
        ));
    }
    let mut cfg = prost_build::Config::new();
    cfg.protoc_arg("--experimental_allow_proto3_optional");
    cfg.compile_protos(&files, &[root.as_path()])?;
    println!("cargo:rerun-if-changed=vendor/meshtastic-protobufs");
    Ok(())
}

fn collect_protos(root: &Path) -> Result<Vec<PathBuf>> {
    let mut stack = vec![root.to_path_buf()];
    let mut out = Vec::new();
    while let Some(dir) = stack.pop() {
        if !dir.exists() {
            return Ok(Vec::new());
        }
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "proto") {
                out.push(path);
            }
        }
    }
    Ok(out)
}
