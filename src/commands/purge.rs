use std::fs;
use std::path::PathBuf;

use log::{debug, info};

use dfm::*;
use crate::{Args, Command, DfmError};
use super::{resolve_dry_run, msg_dry_run};

pub fn purge_command(settings: &Settings, args: &Args, path_to_config_file: &Option<PathBuf>) -> Result<(), DfmError> {
    let Command::Purge {
        dry_run,
        keep_source,
        keep_config_file,
        force
    } = &args.command else {
        return Err(DfmError::Unsupported(format!("unreachable code reached: command {:?} is not `purge`", args.command)));
    };

    let dry_run = resolve_dry_run(*dry_run, args.dry_run);

    let state_directory_path = match calc_state_directory_path() {
        Ok(path) => Some(path),
        Err(e) => {
            info!("state directory path could not be resolved: {}; skipping state directory", e);
            None
        }
    };
    let source_dir_abs_path = match calc_working_dir_paths(&settings) {
        Ok((_, source)) => Some(source),
        Err(e) => {
            info!("source directory path could not be resolved: {}; skipping source directory", e);
            None
        }
    };
    debug!("purge path to config {:?}, state {:?}, source {:?} keep_source {}, keep_config_file {}, force {}",
        path_to_config_file, state_directory_path, source_dir_abs_path, keep_source, keep_config_file, force);

    if dry_run {
        info!("{}", msg_dry_run());
    }

    // Check for un-pulled source changes before deleting the source directory
    if !*keep_source && !*force {
        if let (Some(source_dir_abs_path), Ok(state_path)) = (&source_dir_abs_path, calc_state_file_path()) {
            if let Ok(state) = read_state(&state_path) {
                let mut modified_paths = vec![];
                for (rel_path, sync_time) in &state.syncs {
                    let source_path = PathBuf::from(source_dir_abs_path).join(rel_path);
                    if let Ok(meta) = source_path.metadata() {
                        if let Ok(mtime) = meta.modified() {
                            if mtime > sync_time.0 {
                                modified_paths.push(rel_path.clone());
                            }
                        }
                    }
                }
                if !modified_paths.is_empty() {
                    return Err(DfmError::Other(format!(
                        "source directory contains files with un-pulled changes: {:?}. Use --force to purge",
                        modified_paths
                    )));
                }
            }
        }
    }

    let mut errors: Vec<String> = vec![];

    if !keep_config_file {
        match path_to_config_file {
            None => info!("config file path could not be resolved; skipping"),
            Some(path_to_config_file) if !path_to_config_file.exists() => info!("config file does not exist"),
            Some(path_to_config_file) => {
                if !dry_run {
                    if let Err(e) = fs::remove_file(path_to_config_file) {
                        errors.push(format!("failed to remove config {:?}: {}", path_to_config_file, e));
                    }
                }
                info!("config removed {:?}", path_to_config_file);

                if let Some(config_dir) = path_to_config_file.parent() {
                    let is_home_dir = get_home_path()
                        .map(|home| config_dir == home.as_path())
                        .unwrap_or(false);
                    if is_home_dir {
                        info!("config directory is the home directory; skipping");
                    } else if config_dir.exists() {
                        if !dry_run {
                            if let Err(e) = fs::remove_dir_all(config_dir) {
                                errors.push(format!("failed to remove config directory {:?}: {}", config_dir, e));
                            }
                        }
                        info!("config directory removed {:?}", config_dir);
                    }
                }
            }
        }
    }

    if !keep_source {
        match &source_dir_abs_path {
            None => info!("source directory path could not be resolved; skipping"),
            Some(source_dir_abs_path) if !source_dir_abs_path.exists() => info!("source does not exist"),
            Some(source_dir_abs_path) => {
                if !dry_run {
                    if let Err(e) = fs::remove_dir_all(source_dir_abs_path) {
                        errors.push(format!("failed to remove source {:?}: {}", source_dir_abs_path, e));
                    }
                }
                info!("source removed {:?}", source_dir_abs_path);
            }
        }
    }

    match &state_directory_path {
        None => info!("state directory path could not be resolved; skipping"),
        Some(state_directory_path) if !state_directory_path.exists() => info!("state directory does not exist"),
        Some(state_directory_path) => {
            if !dry_run {
                if let Err(e) = fs::remove_dir_all(state_directory_path) {
                    errors.push(format!("failed to remove state {:?}: {}", state_directory_path, e));
                }
            }
            info!("state removed {:?}", state_directory_path);
        }
    }

    if !errors.is_empty() {
        return Err(DfmError::Other(format!(
            "purge failed to remove some paths: {}",
            errors.join("; ")
        )));
    }
    Ok(())
}
