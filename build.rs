use clap::CommandFactory;
use clap_mangen::Man;
use std::io::Write;
use std::path::PathBuf;

// Reuse the same CLI definition the binary uses. `build.rs` is compiled before
// the `dfm` lib, so this module is copied in by path rather than imported.
#[path = "src/cli.rs"]
mod cli;

fn main() {
    println!("cargo:rerun-if-changed=src/cli.rs");

    // Write `dfm.1` next to the built binary: `<target>/<profile>/dfm.1`.
    // `[package.metadata.aur] files` points at `target/release/dfm.1` so `cargo
    // aur` packs it into the release tarball and installs it to
    // `/usr/share/man/man1/dfm.1`. Writing into the target dir keeps the repo
    // root clean. `OUT_DIR` is `<target>/<profile>/build/<pkg>-<hash>/out`.
    let out_dir = PathBuf::from(env("OUT_DIR"));
    let profile = out_dir
        .ancestors()
        .nth(3)
        .expect("no profile dir above OUT_DIR");
    let target_root = out_dir
        .ancestors()
        .nth(4)
        .expect("no target dir above OUT_DIR");
    let out_path = target_root.join(profile).join("dfm.1");
    let mut out = std::io::BufWriter::new(std::fs::File::create(&out_path).unwrap());
    Man::new(cli::Args::command()).render(&mut out).unwrap();
    out.flush().unwrap();
}

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("build env var {name} missing"))
}


