use std::fs;
use std::path::PathBuf;

use log::{debug, error, info, warn};
use regex::RegexSet;

use dfm::*;
use crate::{Args, Command, DfmError};
use super::{sync_file_copy, resolve_dry_run, require_force};

pub fn pull_command(settings: &Settings, args: &Args, state: &mut StateObject) -> Result<(), DfmError> {
    let Command::Pull {
        paths,
        merge,
        force,
        symlink: target_must_be_symlink,
        dry_run,
    } = &args.command else {
        return Err(DfmError::Unsupported(format!("unreachable code reached: command {:?} is not `pull`", args.command)));
    };

    let dry_run = resolve_dry_run(*dry_run, args.dry_run);

    debug!("pull paths {:?}, merge {}, force {}, dry-run {}", paths, merge, force, dry_run);

    let (target_dir_abs_path, source_dir_abs_path) = calc_working_dir_paths(&settings)?;

    let paths = match paths {
        Some(p) => p.clone(),
        None => vec![source_dir_abs_path.clone()]
    };

    let regex_no_dot_files = RegexSet::new(vec![r#"^(.+/)?[^.][^/]+$"#]).unwrap();
    let ListDirectories{
        found: traversed_paths,
        errors: error_messages,
        ..
    } = list_directory(&paths, Some(&regex_no_dot_files))?;
    debug!("traversing result is {:?}", traversed_paths);

    if !error_messages.is_empty() {
        return Err(DfmError::InvalidData(
            format!("failed to process some subdirectories or files in source {:?}", error_messages)
        ));
    }

    enum PullTask {
        Copy(PathBuf, PathBuf),
        CreateOrUpdateSymlink(PathBuf, String),
    }

    let target_ignore_file_path = calc_local_ignore_file()?;
    let target_ignore_regex = load_ignore_regex(&target_ignore_file_path)?;

    let mut tasks: Vec<PullTask> = vec![];
    let mut error_list = vec![];

    for path in traversed_paths.iter() {
        debug!("checking {:?}", path);

        let target_abs_path = PathBuf::from_iter(vec!(&target_dir_abs_path, &path));
        let target_abs_path = remove_dots_from_path(&target_abs_path);
        debug!("target absolute path {:?}", target_abs_path);

        let target_abs_path = if target_abs_path.starts_with(&source_dir_abs_path) {
            let source_file_abs_path = target_abs_path;
            debug!("provided path of a source {:?}", source_file_abs_path);

            let target_file_rel_to_target_dir = file_path_relative_to(&source_file_abs_path, &source_dir_abs_path);
            let dot_prefix = settings.dot_prefix.clone();
            let target_file_rel_to_target_dir = target_file_rel_to_target_dir.to_str().unwrap().replace(&dot_prefix, ".");
            let target_file_rel_to_target_dir = if source_file_abs_path.to_str().unwrap().ends_with(&settings.symlink_postfix) {
                target_file_rel_to_target_dir.replace(&settings.symlink_postfix, "")
            } else {
                target_file_rel_to_target_dir
            };
            let target_file_abs_path = PathBuf::from_iter(vec![target_dir_abs_path.to_str().unwrap(), &target_file_rel_to_target_dir]);
            let target_file_abs_path = remove_dots_from_path(&target_file_abs_path);
            debug!("inferred target {:?}", target_file_abs_path);

            if let Some(pattern) = check_path_matches_regex(&target_ignore_regex, &target_file_abs_path) {
                info!("target {:?} is ignored by regex /{}/ in file {:?}", target_file_abs_path, pattern, target_ignore_file_path);
                continue;
            }

            if !target_file_abs_path.exists() && source_file_abs_path.exists() {
                if source_file_abs_path.to_str().unwrap().ends_with(&settings.symlink_postfix) {
                    let source_file_content = fs::read_to_string(&source_file_abs_path)?;
                    debug!("source is a symlink file, pointing to {}", source_file_content);
                    tasks.push(PullTask::CreateOrUpdateSymlink(target_file_abs_path, source_file_content));
                    continue; // success
                } else {
                    if *target_must_be_symlink {
                        debug!("symlink creating task");
                        tasks.push(PullTask::CreateOrUpdateSymlink(target_file_abs_path.clone(), source_file_abs_path.to_str().unwrap().to_owned()));
                    } else {
                        debug!("regular file creating task");
                        tasks.push(PullTask::Copy(target_file_abs_path, source_file_abs_path));
                    }
                    continue; // success
                }
            } else if target_file_abs_path.is_symlink() && source_file_abs_path.exists() {
                let target_symlink_pointee = fs::read_link(&target_file_abs_path)?;
                let source_file_content: String = fs::read_to_string(&source_file_abs_path)?.trim().to_string();
                if !source_file_content.eq(target_symlink_pointee.to_str().unwrap()) {
                    info!("target symlink {:?} points to {:?},\n\tmust point to {:?}", target_file_abs_path, target_symlink_pointee, source_file_content);
                    tasks.push(PullTask::CreateOrUpdateSymlink(target_file_abs_path, source_file_content));
                    continue; // success
                }
            }
            // TODO check if the pointee of the symlink also is under management and needs to be pulled.
            target_file_abs_path
        } else {
            target_abs_path
        };

        if let Some(pattern) = check_path_matches_regex(&target_ignore_regex, &target_abs_path) {
            info!("target {:?} is ignored by regex /{}/ in file {:?}", target_abs_path, pattern, target_ignore_file_path);
            continue; // ok
        }

        // TODO handle encrypted files and directories

        if target_abs_path.exists() {
            if target_abs_path.is_symlink() {
                let target_symlink_followed_abs_path = fs::canonicalize(&target_abs_path)?;

                let source_file_abs_path = filepath_in_source_dir(&settings.dot_prefix, &target_dir_abs_path, &source_dir_abs_path, &target_abs_path, None);
                if target_symlink_followed_abs_path == source_file_abs_path {
                    println!("target symlink {:?}\t\npoints to the source file {:?}, skipping...", target_abs_path, source_file_abs_path);
                    error_list.push(format!("target {:?} is a valid symlink", target_abs_path));
                    continue; // success
                }

                let source_symlink_file_abs_path = filepath_in_source_dir(&settings.dot_prefix, &target_dir_abs_path, &source_dir_abs_path, &target_abs_path, Some(&settings.symlink_postfix));
                if source_symlink_file_abs_path.exists() {
                    let target_symlink_pointee_path = fs::read_link(&target_abs_path)?;
                    let source_file_content = fs::read_to_string(&source_symlink_file_abs_path)?;
                    if source_file_content.trim().eq(target_symlink_pointee_path.to_str().unwrap()) {
                        info!("target symlink {:?}\n\tpoints to {:?}, skipping...", target_abs_path, target_symlink_pointee_path.to_str().unwrap());
                        continue; // success
                    } else {
                        info!("target symlink {:?}\n\tpoints to {:?},\n\tmust point to {:?}", target_abs_path, target_symlink_pointee_path.to_str().unwrap(), source_file_content);
                        tasks.push(PullTask::CreateOrUpdateSymlink(target_abs_path.clone(), source_file_content));
                        continue; // success
                    }
                } else {
                    if !target_symlink_followed_abs_path.starts_with(&source_dir_abs_path) {
                        info!("target symlink {:?} does not point to the source directory, skipping...", target_abs_path);
                        // TODO remove the symlink?
                        continue; // success
                    }
                }

                // also the case is handled when the symlink pints inside the source directory but
                // to the wrong file
                tasks.push(PullTask::CreateOrUpdateSymlink(target_abs_path.clone(), source_file_abs_path.to_str().unwrap().to_string()));
                continue;
            }

            // existing target file is not a symlink
            let source_abs_path = filepath_in_source_dir(&settings.dot_prefix, &target_dir_abs_path, &source_dir_abs_path, &target_abs_path, None);
            if !source_abs_path.exists() {
                info!("target {:?} is unmanaged,\n\tno source {:?} found, skipping...", target_abs_path, source_abs_path);
                continue; // TODO is this an error?
            }

            let source_file_rel_path = file_path_relative_to(&source_abs_path, &source_dir_abs_path);
            let source_file_rel_path = remove_dots_from_path(&source_file_rel_path);
            let sync_time_opt = state.syncs.get(source_file_rel_path.to_str().unwrap());

            let cmp = compare_files_by_timestamps(&target_abs_path, &source_abs_path, sync_time_opt)?;

            match cmp {
                CompareByTimestamp::BothModified => {
                    // TODO add merge
                    warn!("both source and target was modified, merge needed");
                    require_force(*force, "target and source have conflicting modifications")?;
                },
                CompareByTimestamp::NonModified => {
                    info!("both source and target were not modified, no action needed, skipping...");
                    continue; // success
                },
                CompareByTimestamp::TargetModified => {
                    warn!("target was modified, pulling source will overwrite those changes");
                    require_force(*force, "target was modified")?;
                },
                CompareByTimestamp::SourceModified => {
                    info!("only the source was modified")
                },
                CompareByTimestamp::NeverSynchronized => {
                    if !force {
                        warn!("target {:?}\n\tand source {:?}\n\twere not synchronized.", target_abs_path, source_abs_path);
                        warn!("Use --force to replace target with source");
                        continue; // TODO error?
                    }
                },
            }
            tasks.push(PullTask::Copy(target_abs_path.clone(), source_abs_path));
        } else {
            // target file does not exist
            debug!("target {:?} does not exist", target_abs_path);

            let source_file_abs_path = filepath_in_source_dir(&settings.dot_prefix, &target_dir_abs_path, &source_dir_abs_path, &target_abs_path, None);
            if source_file_abs_path.exists() {
                info!("source {:?} will be copied\n\tto the target {:?}", source_file_abs_path, target_abs_path);
                if *target_must_be_symlink {
                    tasks.push(PullTask::CreateOrUpdateSymlink(target_abs_path.clone(), source_file_abs_path.to_str().unwrap().to_owned()));
                } else {
                    tasks.push(PullTask::Copy(target_abs_path.clone(), source_file_abs_path));
                }
                continue; // success
            } else {
                let source_symlink_file_abs_path = filepath_in_source_dir(&settings.dot_prefix, &target_dir_abs_path, &source_dir_abs_path, &target_abs_path, Some(&settings.symlink_postfix));
                if source_symlink_file_abs_path.exists() {
                    info!("source symlink file {:?} will be used to create a target symlink", source_symlink_file_abs_path);
                    let source_file_content = fs::read_to_string(&source_symlink_file_abs_path)?;
                    tasks.push(PullTask::CreateOrUpdateSymlink(target_abs_path.clone(), source_file_content));
                    continue; // success
                } else {
                    return Err(DfmError::NotFound(
                        format!("for target {:?} no corresponding source file found", target_abs_path)
                    ));
                }
            }
        }
    }

    if !error_list.is_empty() {
        for error_string in &error_list {
            warn!("error: {:?}", error_string);
        }
        require_force(*force, "improper operation")?;
    }

    if tasks.is_empty() {
        info!("nothing to do");
        return Ok(());
    }

    if dry_run {
        info!("dry run specified, no changes will be made");
    }

    debug!("::copy procedure begins, {} tasks", tasks.len());

    for task in tasks.iter() {
        match task {
            PullTask::Copy(target_file, source_file) => {
                info!("copy source {:?}\n\tto target {:?}", source_file, target_file);
                if dry_run {
                    continue;
                }

                sync_file_copy(source_file, target_file, source_file, state, &source_dir_abs_path)?;
            },
            PullTask::CreateOrUpdateSymlink(target_symlink_file_path, points_to) => {
                info!("create symlink {:?} pointing\n\tto {:?}", target_symlink_file_path, points_to);
                if dry_run {
                    continue;
                }

                if let Err(e) = symlink::remove_symlink_file(target_symlink_file_path) {
                    match e.kind() {
                        std::io::ErrorKind::NotFound => {
                            info!("target symlink {:?} does not exist", target_symlink_file_path);
                            // is ok
                        },
                        _ => {
                            error!("failed to remove symlink {:?}: {}", target_symlink_file_path, e);
                            return Err(e.into());
                        }
                    }
                }
                let points_to = if points_to.starts_with("./") {
                    &points_to[2..]
                } else {
                    points_to.as_str()
                };
                let pointee = PathBuf::from(points_to);

                symlink::symlink_file(pointee, target_symlink_file_path)?;
                debug!("target symlink {:?} updated", target_symlink_file_path)
            }
        }
    }
    Ok(())
}
