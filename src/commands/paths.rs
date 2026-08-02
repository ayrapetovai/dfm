use std::path::PathBuf;

use dfm::*;
use crate::DfmError;
use microxdg::Xdg;

pub fn paths_command(settings: &Settings, xdg: &Xdg, path_to_config_file: &Option<PathBuf>, path_to_state_file: &Option<PathBuf>) -> Result<(), DfmError> {
    let (target_dir_abs_path, ref source_dir_abs_path) = calc_working_dir_paths_unchecked(&settings)?;
    println!("Source: {}", source_dir_abs_path.to_str().unwrap());
    println!("Target: {}", target_dir_abs_path.to_str().unwrap());

    match path_to_config_file {
        Some(p) => println!("Config: {}", p.to_str().unwrap()),
        None => println!("Config: unresolved"),
    }
    match path_to_state_file {
        Some(p) => println!("State : {}", p.to_str().unwrap()),
        None => println!("State : unresolved"),
    }

    println!("Local ignore : {}", calc_local_ignore_file(xdg).unwrap().to_str().unwrap());
    println!("Source ignore: {}", calc_source_ignore_file(source_dir_abs_path).unwrap().to_str().unwrap());

    Ok(())
}
