use std::fs;
use std::path::PathBuf;

use log::{debug, info, warn};
use regex::RegexSet;
use walkdir::WalkDir;

use dfm::*;
use crate::{Args, Command, DfmError};
use super::{sync_file_copy, resolve_dry_run, require_force,
            update_sync_state, get_sync_time, source_rel_to_target_rel,
            list_directory_or_error, msg_dry_run, msg_nothing_to_do};

#[derive(Debug)]
enum PullTask {
    Copy(PathBuf, PathBuf),
    CreateOrUpdateSymlink(PathBuf, String),
    Decrypt(PathBuf, PathBuf),
}

/// Handle the encrypted-source timestamp comparison for pull.
/// Pushes a `Decrypt` task or returns an error via `require_force`.
fn handle_encrypted_timestamps(
    cmp: CompareByTimestamp,
    source_abs: &PathBuf,
    target_abs: &PathBuf,
    force: bool,
    tasks: &mut Vec<PullTask>,
) -> Result<(), DfmError> {
    match cmp {
        CompareByTimestamp::BothModified => {
            warn!("both target and encrypted source {:?} were modified, merge needed", source_abs);
            require_force(force, "target and encrypted source have conflicting modifications")?;
            tasks.push(PullTask::Decrypt(target_abs.clone(), source_abs.clone()));
        },
        CompareByTimestamp::NonModified => {
            if force {
                info!("force flag set, decrypting despite no modifications");
                tasks.push(PullTask::Decrypt(target_abs.clone(), source_abs.clone()));
            } else {
                info!("neither target nor encrypted source were modified, no action needed, skipping...");
            }
        },
        CompareByTimestamp::TargetModified => {
            warn!("target was modified, pulling encrypted source will overwrite those changes");
            require_force(force, "target was modified")?;
            tasks.push(PullTask::Decrypt(target_abs.clone(), source_abs.clone()));
        },
        CompareByTimestamp::SourceModified => {
            info!("only the encrypted source was modified, decrypting...");
            tasks.push(PullTask::Decrypt(target_abs.clone(), source_abs.clone()));
        },
        CompareByTimestamp::NeverSynchronized => {
            warn!("target {:?}\n\tand encrypted source {:?}\n\twere not synchronized.", target_abs, source_abs);
            require_force(force, "encrypted source was not synchronized")?;
            tasks.push(PullTask::Decrypt(target_abs.clone(), source_abs.clone()));
        },
    }
    Ok(())
}

pub fn pull_command(settings: &Settings, args: &Args, state: &mut StateObject) -> Result<(), DfmError> {
    let Command::Pull {
        paths,
        force,
        symlink: target_must_be_symlink,
        dry_run,
    } = &args.command else {
        return Err(DfmError::Unsupported(format!("unreachable code reached: command {:?} is not `pull`", args.command)));
    };

    let dry_run = resolve_dry_run(*dry_run, args.dry_run);

    debug!("pull paths {:?}, force {}, dry-run {}", paths, force, dry_run);

    let (target_dir_abs_path, source_dir_abs_path) = calc_working_dir_paths(&settings)?;

    let paths = match paths {
        Some(p) => p.clone(),
        None => vec![source_dir_abs_path.clone()]
    };

    let regex_no_dot_files = RegexSet::new(vec![r#"^(.+/)?[^.][^/]+$"#]).unwrap();
    let traversed_paths = list_directory_or_error(&paths, Some(&regex_no_dot_files), "in source")?;
    debug!("traversing result is {:?}", traversed_paths);


    let target_ignore_file_path = calc_local_ignore_file()?;
    let target_ignore_regex = load_ignore_regex(&target_ignore_file_path)?;

    let mut tasks: Vec<PullTask> = vec![];
    let mut error_list = vec![];
    let mut patterns_to_remove: Vec<String> = vec![];

    for path in traversed_paths.iter() {
        debug!("checking {:?}", path);

        let target_abs_path = PathBuf::from_iter(vec!(&target_dir_abs_path, &path));
        let target_abs_path = remove_dots_from_path(&target_abs_path);
        debug!("target absolute path {:?}", target_abs_path);

        let target_abs_path = if target_abs_path.starts_with(&source_dir_abs_path) {
            let source_file_abs_path = target_abs_path;
            debug!("provided path of a source {:?}", source_file_abs_path);

            let source_name = source_file_abs_path.to_str().unwrap().to_owned();
            let source_rel_str = file_path_relative_to(&source_file_abs_path, &source_dir_abs_path).to_str().unwrap().to_owned();
            let target_file_rel_to_target_dir = source_rel_to_target_rel(
                &source_rel_str, &settings.dot_prefix,
                &settings.symlink_postfix, &settings.encrypted_postfix,
            );
            let target_file_abs_path = PathBuf::from_iter(vec![target_dir_abs_path.to_str().unwrap(), &target_file_rel_to_target_dir]);
            let target_file_abs_path = remove_dots_from_path(&target_file_abs_path);
            debug!("inferred target {:?}", target_file_abs_path);

            if let Some(pattern) = check_path_matches_regex_component_wise(&target_ignore_regex, &PathBuf::from(&target_file_rel_to_target_dir)) {
                if *force {
                    info!("target {:?} is ignored, --force overrides, will remove /{}/ from ignore file", target_file_abs_path, pattern);
                    patterns_to_remove.push(pattern);
                } else {
                    info!("target {:?} is ignored by regex /{}/ in file {:?}", target_file_abs_path, pattern, target_ignore_file_path);
                    continue;
                }
            }

            if !target_file_abs_path.exists() && source_file_abs_path.exists() {
                if source_name.ends_with(&settings.symlink_postfix) {
                    let source_file_content = fs::read_to_string(&source_file_abs_path)?;
                    debug!("source is a symlink file, pointing to {}", source_file_content);
                    tasks.push(PullTask::CreateOrUpdateSymlink(target_file_abs_path, source_file_content));
                    continue; // success
                } else if source_name.ends_with(&settings.encrypted_postfix) {
                    debug!("decrypting source {:?}\n\tto target {:?}", source_file_abs_path, target_file_abs_path);
                    tasks.push(PullTask::Decrypt(target_file_abs_path, source_file_abs_path));
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
            } else if target_file_abs_path.exists() && source_name.ends_with(&settings.encrypted_postfix) {
                debug!("target {:?} exists, source is encrypted, checking timestamps", target_file_abs_path);

                let cmp = compare_files_by_timestamps(
                    &target_file_abs_path, &source_file_abs_path,
                    get_sync_time(state, &source_file_abs_path, &source_dir_abs_path).map(|st| &**st),
                )?;

                    handle_encrypted_timestamps(cmp, &source_file_abs_path, &target_file_abs_path, *force, &mut tasks)?;
                continue;
            }
            // TODO check if the pointee of the symlink also is under management and needs to be pulled.
            target_file_abs_path
        } else {
            target_abs_path
        };

        let target_rel_path = file_path_relative_to(&target_abs_path, &target_dir_abs_path);
        if let Some(pattern) = check_path_matches_regex_component_wise(&target_ignore_regex, &target_rel_path) {
            if *force {
                info!("target {:?} is ignored, --force overrides, will remove /{}/ from ignore file", target_abs_path, pattern);
                patterns_to_remove.push(pattern);
            } else {
                info!("target {:?} is ignored by regex /{}/ in file {:?}", target_abs_path, pattern, target_ignore_file_path);
                continue; // ok
            }
        }

        // encrypted source files handled in source-traversal path (above)
        // and in the non-source-traversal existing-target branch (below)

        if target_abs_path.exists() {
            if target_abs_path.is_symlink() {
                let target_symlink_followed_abs_path = fs::canonicalize(&target_abs_path)
                    .map_err(|e| DfmError::Other(format!(
                        "Target symlink {:?} is broken (points to non-existent path): {}",
                        target_abs_path, e
                    )))?;

                let source_file_abs_path = filepath_in_source_dir(&settings.dot_prefix, &target_dir_abs_path, &source_dir_abs_path, &target_abs_path, None);
                if target_symlink_followed_abs_path == source_file_abs_path {
                    info!("target symlink {:?}\n\tpoints to the source file {:?}, skipping...", target_abs_path, source_file_abs_path);
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
                // Check for encrypted source before giving up
                let source_encrypted_abs_path = filepath_in_source_dir(&settings.dot_prefix, &target_dir_abs_path, &source_dir_abs_path, &target_abs_path, Some(&settings.encrypted_postfix));
                if source_encrypted_abs_path.exists() {
                    debug!("target {:?} exists, encrypted source found, checking timestamps", target_abs_path);

                    let cmp = compare_files_by_timestamps(
                        &target_abs_path, &source_encrypted_abs_path,
                        get_sync_time(state, &source_encrypted_abs_path, &source_dir_abs_path).map(|st| &**st),
                    )?;

                    handle_encrypted_timestamps(cmp, &source_encrypted_abs_path, &target_abs_path, *force, &mut tasks)?;
                    continue; // either skipped or task pushed, proceed to next file
                }
                info!("target {:?} is unmanaged,\n\tno source {:?} found, skipping...", target_abs_path, source_abs_path);
                continue; // TODO is this an error?
            }

            let sync_time_opt = get_sync_time(state, &source_abs_path, &source_dir_abs_path);

            let cmp = compare_files_by_timestamps(&target_abs_path, &source_abs_path, sync_time_opt.map(|st| &**st))?;

            match cmp {
                CompareByTimestamp::BothModified => {
                    warn!("both source and target were modified, merge needed");
                    require_force(*force, "target and source have conflicting modifications")?;
                },
                CompareByTimestamp::NonModified => {
                    if *force {
                        info!("force flag set, copying despite no modifications");
                    } else {
                        info!("both source and target were not modified, no action needed, skipping...");
                        continue; // success
                    }
                },
                CompareByTimestamp::TargetModified => {
                    warn!("target was modified, pulling source will overwrite those changes");
                    require_force(*force, "target was modified")?;
                },
                CompareByTimestamp::SourceModified => {
                    info!("only the source was modified")
                },
                CompareByTimestamp::NeverSynchronized => {
                    warn!("target {:?}\n\tand source {:?}\n\twere not synchronized.", target_abs_path, source_abs_path);
                    error_list.push(format!("target {:?} and source {:?} were not synchronized", target_abs_path, source_abs_path));
                    if !*force {
                        continue;
                    }
                },
            }
            tasks.push(PullTask::Copy(target_abs_path.clone(), source_abs_path));
        } else {
            // target file does not exist
            debug!("target {:?} does not exist", target_abs_path);

            let source_file_abs_path = filepath_in_source_dir(&settings.dot_prefix, &target_dir_abs_path, &source_dir_abs_path, &target_abs_path, None);
            if source_file_abs_path.exists() {
                if source_file_abs_path.is_dir() {
                    // Source is a directory — walk it recursively to find all files
                    // and create individual pull tasks for each.
                    for entry in WalkDir::new(&source_file_abs_path)
                        .follow_links(false)
                        .follow_root_links(false)
                    {
                        let entry = entry.map_err(|e| {
                            let msg = e.to_string();
                            let inner = e.into_io_error()
                                .unwrap_or_else(|| std::io::Error::other(msg));
                            DfmError::Io(inner)
                        })?;
                        if entry.file_type().is_dir() {
                            continue;
                        }
                        let source_file = entry.path().to_path_buf();
                        let relative = source_file
                            .strip_prefix(&source_file_abs_path)
                            .map_err(|e| DfmError::Other(e.to_string()))?;
                        let target_file = target_abs_path.join(relative);
                        info!("source {:?} will be copied\n\tto the target {:?}", source_file, target_file);
                        if *target_must_be_symlink {
                            tasks.push(PullTask::CreateOrUpdateSymlink(target_file, source_file.to_str().unwrap().to_owned()));
                        } else {
                            tasks.push(PullTask::Copy(target_file, source_file));
                        }
                    }
                    continue; // success
                }

                info!("source {:?} will be copied\n\tto the target {:?}", source_file_abs_path, target_abs_path);
                if *target_must_be_symlink {
                    tasks.push(PullTask::CreateOrUpdateSymlink(target_abs_path.clone(), source_file_abs_path.to_str().unwrap().to_owned()));
                } else {
                    tasks.push(PullTask::Copy(target_abs_path.clone(), source_file_abs_path));
                }
                continue; // success
            }

            let source_encrypted_file_abs_path = filepath_in_source_dir(&settings.dot_prefix, &target_dir_abs_path, &source_dir_abs_path, &target_abs_path, Some(&settings.encrypted_postfix));
            if source_encrypted_file_abs_path.exists() {
                info!("encrypted source {:?} will be decrypted\n\tto the target {:?}", source_encrypted_file_abs_path, target_abs_path);
                tasks.push(PullTask::Decrypt(target_abs_path.clone(), source_encrypted_file_abs_path));
                continue; // success
            }

            let source_symlink_file_abs_path = filepath_in_source_dir(&settings.dot_prefix, &target_dir_abs_path, &source_dir_abs_path, &target_abs_path, Some(&settings.symlink_postfix));
            if source_symlink_file_abs_path.exists() {
                info!("source symlink file {:?} will be used to create a target symlink", source_symlink_file_abs_path);
                let source_file_content = fs::read_to_string(&source_symlink_file_abs_path)?;
                tasks.push(PullTask::CreateOrUpdateSymlink(target_abs_path.clone(), source_file_content));
                continue; // success
            }

            return Err(DfmError::NotFound(
                format!("for target {:?} no corresponding source file found", target_abs_path)
            ));
        }
    }

    if !error_list.is_empty() {
        require_force(*force, "improper operation")?;
    }

    if tasks.is_empty() {
        info!("{}", msg_nothing_to_do());
        return Ok(());
    }

    if dry_run {
        info!("{}", msg_dry_run());
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
                        _ => return Err(e.into()),
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
            },
            PullTask::Decrypt(target_file, source_file) => {
                info!("decrypt source {:?}\n\tto target {:?}", source_file, target_file);
                if dry_run {
                    continue;
                }

                dfm::crypt::read_zip_file(settings, source_file, target_file)?;

                update_sync_state(state, source_file, target_file, &source_dir_abs_path)?;
            },
        }
    }

    if !dry_run && !patterns_to_remove.is_empty() {
        let ignore_file_path = calc_local_ignore_file()?;
        if ignore_file_path.exists() {
            let content = fs::read_to_string(&ignore_file_path)?;
            let remaining: Vec<&str> = content.lines()
                .filter(|line| {
                    let trimmed = line.trim();
                    trimmed.is_empty() || !patterns_to_remove.iter().any(|p| p == trimmed)
                })
                .collect();
            let new_content = remaining.join("\n");
            fs::write(&ignore_file_path, new_content)?;
            info!("removed {} pattern(s) from ignore file", patterns_to_remove.len());
        }
    }

    Ok(())
}
