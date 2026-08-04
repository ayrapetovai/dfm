use std::path::PathBuf;

use dfm::*;
use crate::DfmError;
use microxdg::Xdg;

/// Typed, per-command arguments for `paths` (built by the dispatcher).
pub struct PathsArgs {}

pub fn paths_command(settings: &Settings, xdg: &Xdg, args: PathsArgs, path_to_config_file: &Option<PathBuf>, path_to_state_file: &Option<PathBuf>) -> Result<(), DfmError> {
    let PathsArgs {} = args;
    let (target_dir_abs_path, ref source_dir_abs_path) = calc_working_dir_paths_unchecked(settings)?;
    println!("Source: {}", source_dir_abs_path.display());
    println!("Target: {}", target_dir_abs_path.display());

    match path_to_config_file {
        Some(p) => println!("Config: {}", p.display()),
        None => println!("Config: unresolved"),
    }
    match path_to_state_file {
        Some(p) => println!("State : {}", p.display()),
        None => println!("State : unresolved"),
    }

    println!("Local ignore : {}", calc_local_ignore_file(xdg).unwrap().display());
    println!("Source ignore: {}", calc_source_ignore_file(source_dir_abs_path).unwrap().display());

    Ok(())
}
