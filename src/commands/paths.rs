use std::path::PathBuf;

use dfm::*;
use crate::DfmError;

pub fn paths_command(settings: &Settings, path_to_config_file: &PathBuf, path_to_state_file: &PathBuf) -> Result<(), DfmError> {
    let (target_dir_abs_apth, ref source_dir_abs_path) = calc_working_dir_paths(&settings)?;
    println!("Source: {}", source_dir_abs_path.to_str().unwrap());
    println!("Target: {}", target_dir_abs_apth.to_str().unwrap());

    println!("Config: {}", path_to_config_file.to_str().unwrap());
    println!("State : {}", path_to_state_file.to_str().unwrap());

    println!("Local ignore : {}", calc_local_ignore_file().unwrap().to_str().unwrap());
    println!("Source ignore: {}", calc_source_ignore_file(source_dir_abs_path).unwrap().to_str().unwrap());

    Ok(())
}
