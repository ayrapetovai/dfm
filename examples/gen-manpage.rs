//! Generate `dfm.1` from the CLI definition.
//!
//! Usage: `cargo run --example gen-manpage`
//!
//! Writes `dfm.1` to the crate root (where `[package.metadata.aur] files`
//! expects it), then packs it into the AUR release tarball and installs it to
//! `/usr/share/man/man1/dfm.1` when you run `cargo aur` / `makepkg` (see
//! `DEVELOPMENT.md`, "Create a package from sources").

use clap::CommandFactory;
use clap_mangen::Man;
use dfm::cli::Args;
use std::io::Write;

fn main() -> Result<(), std::io::Error> {
    let out_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("dfm.1");
    let mut out = std::io::BufWriter::new(std::fs::File::create(&out_path)?);
    Man::new(Args::command()).render(&mut out)?;
    out.flush()?;
    println!("wrote {}", out_path.display());
    Ok(())
}


