use std::path::PathBuf;

use dfm::*;
use crate::DfmError;

pub fn paths_command(settings: &Settings, path_to_config_file: &PathBuf, path_to_state_file: &PathBuf) -> Result<(), DfmError> {
    println!("config {:?}", path_to_config_file);
    println!("state  {:?}", path_to_state_file);

    let (target_dir_abs_apth, ref source_dir_abs_path) = calc_working_dir_paths(&settings)?;
    println!("source {:?}", source_dir_abs_path);
    println!("target {:?}", target_dir_abs_apth);

    println!("local ignore  {:?}", calc_local_ignore_file().unwrap());
    println!("source ignore {:?}", calc_source_ignore_file(source_dir_abs_path).unwrap());
    Ok(())
}
