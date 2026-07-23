use std::io::Error;
use std::path::PathBuf;

use dfm::*;

pub fn paths_command(settings: &Settings, path_to_config_file: &PathBuf, path_to_state_file: &PathBuf) -> Result<(), Error> {
    println!("config {:?}", path_to_config_file);
    println!("state  {:?}", path_to_state_file);

    let (target_dir_abs_apth, ref source_dir_abs_path) = calc_working_dir_paths(&settings)?;
    println!("source {:?}", source_dir_abs_path);
    println!("target {:?}", target_dir_abs_apth);
    Ok(())
}
