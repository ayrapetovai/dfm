use std::fs;
use std::path::PathBuf;

use log::{debug, error, info, trace, warn};

use dfm::*;
use crate::{Args, Command, DfmError};

pub fn forget_command(settings: &Settings, args: &Args, state: &mut StateObject) -> Result<(), DfmError> {
    let Command::Forget {
        paths,
        force,
        dry_run,
        ..
    } = &args.command else {
        return Err(DfmError::Unsupported(format!("unreachable code reached: command {:?} is not `forget`", args.command)));
    };

    let dry_run = if !dry_run { args.dry_run } else { true };

    debug!("forget paths {:?}, force {}, dry-run {}", paths, force, dry_run);

    let (target_dir_abs_path, source_dir_abs_path) = calc_working_dir_paths(&settings)?;

    let paths = match paths {
        Some(p) => p.clone(),
        None => vec![target_dir_abs_path.clone()]
    };

    let ListDirectories {
        found: traversed_paths,
        errors: error_messages,
        ..
    } = list_directory(&paths, None)?;
    debug!("traversing result is {:?}", traversed_paths);

    if !error_messages.is_empty() {
        return Err(DfmError::InvalidData(
            format!("failed to process some subdirectories or files in targets {:?}", error_messages)
        ));
    }

    #[derive(Debug)]
    enum ForgetTask {
        Delete(PathBuf),
    }

    let mut tasks: Vec<ForgetTask> = Vec::new();
    let mut error_messages = vec![];

    debug!("::check state procedure begins");

    for target_path in traversed_paths.iter() {
        debug!("checking {:?}", target_path);

        if target_path.is_symlink() {
            let target_abs_path = PathBuf::from_iter(vec![&target_dir_abs_path, &target_path]);
            let target_abs_path = remove_dots_from_path(&target_abs_path);
            let target_symlink_pointee_path = fs::read_link(&target_abs_path)?;

            debug!("target symlink {:?}\n\tpoints to {:?}", target_abs_path, target_symlink_pointee_path);
            if target_symlink_pointee_path.starts_with(&source_dir_abs_path) {
                info!("target symlink {:?}\n\tpoints into source directory, removing", target_abs_path);
                tasks.push(ForgetTask::Delete(target_abs_path.clone()));
            }

            let source_symlink_file_abs_path = filepath_in_source_dir(
                &settings.dot_prefix, &target_dir_abs_path, &source_dir_abs_path,
                &target_abs_path, Some(&settings.symlink_postfix)
            );
            if source_symlink_file_abs_path.exists() {
                let source_file_content = fs::read_to_string(&source_symlink_file_abs_path)?;
                if source_file_content.trim().eq(target_symlink_pointee_path.to_str().unwrap()) {
                    info!("target symlink {:?}\n\tpoints to {:?}, skipping...", target_abs_path, target_symlink_pointee_path.to_str().unwrap());
                    tasks.push(ForgetTask::Delete(source_symlink_file_abs_path));
                    continue;
                } else {
                    info!("target symlink {:?}\n\tpoints to {:?},\n\tmust point to {:?}", target_abs_path, target_symlink_pointee_path.to_str().unwrap(), source_file_content);
                    if *force {
                        tasks.push(ForgetTask::Delete(source_symlink_file_abs_path));
                    } else {
                        info!("specify --force to delete source {:?}", source_symlink_file_abs_path);
                    }
                    continue; // success
                }
            } else {
                debug!("symlink {:?}\n\tdoes not have source symlink file {:?}, skipping...", target_abs_path, source_symlink_file_abs_path);
            }
        }

        let target_abs_path_res = fs::canonicalize(&target_path);
        if target_abs_path_res.is_err() {
            // we are given a path in source dir
            if target_path.is_symlink() {
                debug!("symlink {:?} is broken: {:?}", target_path, target_abs_path_res);
                continue; // error
            }

            let source_file_abs_path = PathBuf::from_iter(vec![&source_dir_abs_path, &target_path]);
            if source_file_abs_path.exists() {
                info!("source {:?} will be removed", source_file_abs_path);
                tasks.push(ForgetTask::Delete(source_file_abs_path));
                continue; // success
            } else {
                info!("source {:?} does not exist, skipping...", source_file_abs_path);
                continue; // success?
            }
        } else {
            let target_abs_path = target_abs_path_res?; // safe
            if target_abs_path.starts_with(&source_dir_abs_path) {
                let source_abs_path = target_abs_path;
                debug!("target {:?} resides in source directory", source_abs_path);
                if source_abs_path.to_str().unwrap().ends_with(&settings.symlink_postfix) {
                    let source_symlink_file_abs_path = source_abs_path;
                    let source_rel_path = file_path_relative_to(&source_symlink_file_abs_path, &source_dir_abs_path);
                    let source_rel_str = source_rel_path.to_str().unwrap()
                        .replace(&settings.dot_prefix, ".")
                        .replace(&settings.symlink_postfix, "");
                    let target_symlink_abs_path = PathBuf::from_iter(vec![target_dir_abs_path.to_str().unwrap(), &source_rel_str]);
                    if target_symlink_abs_path.exists() {
                        let target_symlink_pointee_path = match fs::read_link(&target_symlink_abs_path) {
                            Ok(p) => p,
                            Err(e) => {
                                error!("failed to read symlink {:?}: {}", target_symlink_abs_path, e);
                                return Err(e.into());
                            }
                        };
                        let source_file_content = fs::read_to_string(&source_symlink_file_abs_path).unwrap();
                        if source_file_content.trim().eq(target_symlink_pointee_path.to_str().unwrap()) {
                            info!("target symlink {:?}\n\tpoints to {:?}, skipping...", target_symlink_abs_path, target_symlink_pointee_path.to_str().unwrap());
                            tasks.push(ForgetTask::Delete(source_symlink_file_abs_path));
                            continue; // success
                        } else {
                            info!("target symlink {:?}\n\tpoints to {:?},\n\tmust point to {:?}", target_symlink_abs_path, target_symlink_pointee_path.to_str().unwrap(), source_file_content);
                            if *force {
                                tasks.push(ForgetTask::Delete(source_symlink_file_abs_path));
                            } else {
                                info!("specify --force to delete source {:?}", source_symlink_file_abs_path);
                            }
                            continue; // success
                        }
                    }
                } else {
                    info!("source {:?} will be removed", source_abs_path);
                    tasks.push(ForgetTask::Delete(source_abs_path));
                    continue; // success
                }
            } else if target_abs_path.starts_with(&target_dir_abs_path) {
                debug!("target {:?} resides in target directory", target_abs_path);
                let source_abs_path = filepath_in_source_dir(&settings.dot_prefix, &target_dir_abs_path, &source_dir_abs_path, &target_abs_path, None);
                if source_abs_path.exists() {
                    let source_file_rel_path = file_path_relative_to(&source_abs_path, &source_dir_abs_path);
                    let source_file_rel_path = remove_dots_from_path(&source_file_rel_path);
                    let sync_time_opt = state.syncs.get(source_file_rel_path.to_str().unwrap());

                    let cmp = compare_files_by_timestamps(&target_abs_path, &source_abs_path, sync_time_opt)?;
                    if CompareByTimestamp::SourceModified == cmp {
                        // source was modified and if we remove it then we will lose the modifications
                        warn!("source {:?}, was modified, run with --force", source_abs_path);
                        error_messages.push("source was modified");
                        tasks.push(ForgetTask::Delete(source_abs_path.clone()));
                        continue; // error
                    }

                    if CompareByTimestamp::BothModified == cmp {
                        // source was modified and if we remove it then we will lose the modifications
                        warn!("source {:?} and target {:?}, both were modified, run with --force", source_abs_path, target_abs_path);
                        error_messages.push("source and target were modified");
                        tasks.push(ForgetTask::Delete(source_abs_path.clone()));
                        continue; // error
                    }
                    if CompareByTimestamp::TargetModified == cmp {
                        // source was modified and if we remove it then we will lose the modifications
                        warn!("target {:?}, was modified, run with --force", target_abs_path);
                        error_messages.push("target was modified");
                        tasks.push(ForgetTask::Delete(source_abs_path.clone()));
                        continue; // error
                    }
                    info!("source {:?} will be removed", source_abs_path);
                    tasks.push(ForgetTask::Delete(source_abs_path));
                    continue; // success
                } else {
                    info!("source {:?} does not exist, skipping...", source_abs_path);
                    continue; // success
                }
            } else {
                warn!("target {:?}\n\tresides outside the target directory {:?}, skipping...", target_abs_path, target_dir_abs_path);
                continue;
            }
        }
    }

    if !error_messages.is_empty() && !force {
        for error_message in error_messages {
            error!("{}", error_message);
        }
        return Err(DfmError::other("forget failed"));
    }

    if tasks.is_empty() {
        info!("nothing to do");
        return Ok(());
    }

    if dry_run {
        info!("dry run specified, no changes will be made");
    }

    debug!("::remove procedure begins, {} tasks", tasks.len());

    for task in tasks.iter() {
        match task {
            ForgetTask::Delete(source_file) => {
                info!("delete {:?}", source_file);
                if dry_run && !*force {
                    continue;
                }

                // fs::remove_file does not follow links, it deletes the specified file
                // even it is a symlink
                if let Err(e) = fs::remove_file(&source_file) {
                    error!("failed to remove file {:?}: {}", source_file, e);
                    return Err(e.into());
                }
                let source_rel_path = file_path_relative_to(&source_file, &source_dir_abs_path);
                let source_rel_path = remove_dots_from_path(&source_rel_path);
                state.syncs.remove(source_rel_path.to_str().unwrap());

                let mut parent_opt = source_file.parent();
                while let Some(dir) = parent_opt {
                    parent_opt = dir.parent();
                    if dir != source_dir_abs_path && dir.starts_with(&source_dir_abs_path) && dir.read_dir()?.next().is_none() {
                        info!("removing empty directory {:?}", dir);
                        if let Err(e) = fs::remove_dir(dir) {
                            error!("failed to remove parent directory {:?}: {}", dir, e);
                            return Err(e.into());
                        }
                    } else {
                        trace!("removing stopped at {:?}", dir);
                        break;
                    }
                }
            },
        }
    }
    Ok(())
}
