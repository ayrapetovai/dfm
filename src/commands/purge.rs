use std::fs;
use std::path::PathBuf;

use log::{debug, info};

use dfm::*;
use crate::{Args, Command, DfmError};
use super::{resolve_dry_run, msg_dry_run, source_rel_to_target_rel};

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
    let (target_dir_abs_path, source_dir_abs_path) = match calc_working_dir_paths(&settings) {
        Ok((target, source)) => (Some(target), Some(source)),
        Err(e) => {
            info!("working directory paths could not be resolved: {}; skipping source and target directories", e);
            (None, None)
        }
    };
    debug!("purge path to config {:?}, state {:?}, source {:?} keep_source {}, keep_config_file {}, force {}",
        path_to_config_file, state_directory_path, source_dir_abs_path, keep_source, keep_config_file, force);

    if dry_run {
        info!("{}", msg_dry_run());
    }

    // Check for un-pushed / un-pulled changes before deleting the source directory.
    // Managed symlinks are excluded: their data is preserved by the replacement
    // step below, so there is nothing to lose for them.
    if !*keep_source && !*force {
        if let (Some(source_dir_abs_path), Some(target_dir_abs_path), Ok(state_path)) =
            (&source_dir_abs_path, &target_dir_abs_path, calc_state_file_path())
        {
            if let Ok(state) = read_state(&state_path) {
                let mut un_pulled = vec![];
                let mut un_pushed = vec![];
                for (rel_path, sync_time) in &state.syncs {
                    let (target_rel, target_abs) = state_key_to_target(target_dir_abs_path, rel_path, settings);
                    if let Ok(meta) = fs::symlink_metadata(&target_abs) {
                        if meta.file_type().is_symlink() {
                            debug!("purge: managed symlink {:?} is preserved by replacement; skipping safety check", target_abs);
                            continue;
                        }
                    }

                    let source_path = PathBuf::from(source_dir_abs_path).join(rel_path);
                    if let Ok(meta) = source_path.metadata() {
                        if let Ok(mtime) = meta.modified() {
                            if mtime > sync_time.mtime {
                                un_pulled.push(rel_path.clone());
                            }
                        }
                    }

                    if let Ok(meta) = target_abs.metadata() {
                        if let Ok(mtime) = meta.modified() {
                            if mtime > sync_time.mtime {
                                un_pushed.push(target_rel);
                            }
                        }
                    }
                }

                let mut msgs = vec![];
                if !un_pulled.is_empty() {
                    msgs.push(format!("source directory contains files with un-pulled changes: {:?}", un_pulled));
                }
                if !un_pushed.is_empty() {
                    msgs.push(format!("target directory contains files with un-pushed changes: {:?}", un_pushed));
                }
                if !msgs.is_empty() {
                    return Err(DfmError::Other(format!("{}; use --force to purge", msgs.join("; "))));
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

    // Replace managed target symlinks with regular copies of their pointees, so
    // removing the source directory does not leave dangling symlinks or lose
    // the files they point to. Runs before the source directory removal.
    if !*keep_source {
        if let (Some(source_dir_abs_path), Some(target_dir_abs_path), Ok(state_path)) =
            (&source_dir_abs_path, &target_dir_abs_path, calc_state_file_path())
        {
            if let Ok(state) = read_state(&state_path) {
                replace_managed_symlinks(settings, target_dir_abs_path, source_dir_abs_path, &state, dry_run, &mut errors);
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

/// Map a state key (source-relative path) to the target-relative path and the
/// absolute target path, stripping the dot-prefix and the symlink/encrypted
/// postfixes.
fn state_key_to_target(target_dir_abs_path: &PathBuf, source_rel: &str, settings: &Settings) -> (String, PathBuf) {
    let target_rel = source_rel_to_target_rel(
        source_rel,
        &settings.dot_prefix,
        &settings.symlink_postfix,
        &settings.encrypted_postfix,
    );
    let target_abs = PathBuf::from_iter([target_dir_abs_path.to_str().unwrap(), &target_rel]);
    (target_rel, remove_dots_from_path(&target_abs))
}

/// Replace managed target symlinks (created by `add -s` / `pull -s`, or from a
/// `.symlink` pointer file) with regular copies of the files they point to.
fn replace_managed_symlinks(
    settings: &Settings,
    target_dir_abs_path: &PathBuf,
    source_dir_abs_path: &PathBuf,
    state: &StateObject,
    dry_run: bool,
    errors: &mut Vec<String>,
) {
    for (rel_path, _) in &state.syncs {
        let (_, target_abs) = state_key_to_target(target_dir_abs_path, rel_path, settings);
        let meta = match fs::symlink_metadata(&target_abs) {
            Ok(meta) => meta,
            Err(_) => continue,
        };
        if !meta.file_type().is_symlink() {
            continue;
        }

        let pointee = match fs::read_link(&target_abs) {
            Ok(p) => p,
            Err(_) => continue,
        };
        let pointee_abs = if pointee.is_absolute() {
            remove_dots_from_path(&pointee)
        } else {
            let joined = target_abs.parent().unwrap_or(target_dir_abs_path).join(&pointee);
            remove_dots_from_path(&joined)
        };

        if !pointee_abs.starts_with(source_dir_abs_path) {
            info!("target symlink {:?} points outside the source directory; leaving as is", target_abs);
            continue;
        }

        if dry_run {
            info!("would replace target symlink {:?} with its pointee {:?}", target_abs, pointee_abs);
            continue;
        }

        if let Err(e) = fs::remove_file(&target_abs) {
            errors.push(format!("failed to remove target symlink {:?}: {}", target_abs, e));
            continue;
        }

        let copy_result = if pointee_abs.to_str().unwrap_or("").ends_with(&settings.encrypted_postfix) {
            dfm::crypt::read_zip_file(settings, &pointee_abs, &target_abs)
        } else {
            fs::copy(&pointee_abs, &target_abs).map_err(DfmError::from).map(|_| ())
        };
        if let Err(e) = copy_result {
            errors.push(format!("failed to replace target symlink {:?} with {:?}: {}", target_abs, pointee_abs, e));
            continue;
        }

        if let Ok(meta) = fs::metadata(&pointee_abs) {
            let _ = fs::set_permissions(&target_abs, meta.permissions());
        }

        info!("replaced target symlink {:?} with its pointee {:?}", target_abs, pointee_abs);
    }
}
