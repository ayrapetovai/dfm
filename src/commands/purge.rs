use std::fs;
use std::path::PathBuf;

use log::{debug, info};

use dfm::*;
use crate::{Args, Command, DfmError};
use super::resolve_dry_run;

pub fn purge_command(settings: &Settings, args: &Args, path_to_config_file: &PathBuf) -> Result<(), DfmError> {
    let Command::Purge {
        dry_run,
        keep_source,
        keep_config_file,
        force
    } = &args.command else {
        return Err(DfmError::Unsupported(format!("unreachable code reached: command {:?} is not `purge`", args.command)));
    };

    let dry_run = resolve_dry_run(*dry_run, args.dry_run);

    let ref state_directory_path = match calc_state_directory_path() {
        Ok(path) => path,
        Err(e) => return Err(e)
    };
    let (_, ref source_dir_abs_path) = calc_working_dir_paths(&settings)?;
    debug!("purge path to config {:?}, state {:?}, source {:?} keep_source {}, keep_config_file {}, force {}",
        path_to_config_file, state_directory_path, source_dir_abs_path, keep_source, keep_config_file, force);

    if dry_run {
        info!("dry run specified, no changes will be made");
    }

    if !keep_config_file {
        if !path_to_config_file.exists() {
            info!("config file does not exist");
        } else {
            if !dry_run {
                fs::remove_file(path_to_config_file)?;
            }
            info!("config removed {:?}", path_to_config_file);
        }
    }

    if !keep_source {
        if !source_dir_abs_path.exists() {
            info!("source does not exits");
        } else {
            if !dry_run {
                fs::remove_dir_all(source_dir_abs_path.clone())?;
            }
            info!("source removed {:?}", source_dir_abs_path);
        }
    }

    if !state_directory_path.exists() {
        info!("state directory does not exist");
    } else {
        if !dry_run {
            fs::remove_dir_all(state_directory_path)?;
        }
        info!("state removed {:?}", state_directory_path);
    }
    Ok(())
}
