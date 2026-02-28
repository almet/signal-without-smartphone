use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR not set"));
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let embed_path = out_dir.join("signal-cli-embed.tar.gz");
    let embedded_rs = out_dir.join("embedded_signal_cli.rs");

    println!("cargo:rerun-if-env-changed=SIGNAL_CLI_ARCHIVE");

    let source = env::var("SIGNAL_CLI_ARCHIVE")
        .ok()
        .map(PathBuf::from)
        .filter(|p| p.exists())
        .or_else(|| {
            let local = manifest_dir.join("signal-cli-embed.tar.gz");
            if local.exists() {
                println!("cargo:rerun-if-changed={}", local.display());
                Some(local)
            } else {
                None
            }
        });

    if let Some(src) = source {
        println!("cargo:rerun-if-changed={}", src.display());
        if let Err(e) = fs::copy(&src, &embed_path) {
            panic!("Failed to copy signal-cli archive from {}: {}", src.display(), e);
        }
        write_embedded_rs(&embedded_rs, Some(&embed_path));
    } else {
        write_embedded_rs(&embedded_rs, None);
    }
}

fn write_embedded_rs(out_file: &Path, embed: Option<&Path>) {
    let mut file = fs::File::create(out_file).expect("Failed to create embedded_signal_cli.rs");
    if let Some(path) = embed {
        let path_str = path.display().to_string().replace('\\', "\\\\");
        writeln!(
            file,
            "pub const EMBEDDED_SIGNAL_CLI: Option<&'static [u8]> = Some(include_bytes!(\"{}\"));",
            path_str
        )
        .expect("Failed to write embedded_signal_cli.rs");
    } else {
        writeln!(
            file,
            "pub const EMBEDDED_SIGNAL_CLI: Option<&'static [u8]> = None;"
        )
        .expect("Failed to write embedded_signal_cli.rs");
    }
}
