use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use log::{debug, error, info, warn};

use dfm::*;
use crate::{Args, Command, DfmError};
use microxdg::Xdg;
use super::{resolve_dry_run, require_force, get_sync_time, remove_sync_state,
            source_rel_to_target_rel, list_directory_or_error,
            msg_dry_run, msg_nothing_to_do, report_progress};

fn source_to_state_key(source_abs: &PathBuf, source_dir_abs: &PathBuf) -> String {
    let rel = file_path_relative_to(source_abs, source_dir_abs);
    let rel = remove_dots_from_path(&rel);
    rel.to_str().unwrap().to_string()
}

pub fn forget_command(settings: &Settings, xdg: &Xdg, args: &Args, state: &mut StateObject) -> Result<(), DfmError> {
    let Command::Forget {
        paths,
        force,
        dry_run,
        ..
    } = &args.command else {
        return Err(DfmError::Unsupported(format!("unreachable code reached: command {:?} is not `forget`", args.command)));
    };

    let dry_run = resolve_dry_run(*dry_run, args.dry_run);

    debug!("forget paths {:?}, force {}, dry-run {}", paths, force, dry_run);

    let (target_dir_abs_path, source_dir_abs_path) = calc_working_dir_paths(&settings)?;

    let forget_all = paths.is_none();
    let paths = match paths {
        Some(p) => p.clone(),
        None => vec![target_dir_abs_path.clone()]
    };

    let target_ignore_file_path = calc_local_ignore_file(xdg)?;
    let target_ignore_regex = load_ignore_regex(&target_ignore_file_path)?;

    let traversed_paths = list_directory_or_error(
        &paths,
        &target_dir_abs_path,
        Some(TraversalFilter::PruneIgnoredDirs(&target_ignore_regex)),
        "in targets",
    )?;
    debug!("traversing result is {:?}", traversed_paths);

    enum ForgetTask {
        Delete(PathBuf),
        RemoveState(String),
    }

    let mut tasks: Vec<ForgetTask> = Vec::new();
    let mut error_messages: Vec<String> = vec![];

    debug!("::check state procedure begins");

    let mut progress = ProgressLine::new();
    for (i, target_path) in traversed_paths.iter().enumerate() {
        report_progress(&mut progress, i + 1, traversed_paths.len());
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
                    continue;
                }
            } else {
                debug!("symlink {:?}\n\tdoes not have source symlink file {:?}, skipping...", target_abs_path, source_symlink_file_abs_path);
            }
        }

        let target_abs_path_res = fs::canonicalize(&target_path);
        if target_abs_path_res.is_err() {
            if target_path.is_symlink() {
                debug!("symlink {:?} is broken: {:?}", target_path, target_abs_path_res);
                continue;
            }

            let source_file_abs_path = PathBuf::from_iter(vec![&source_dir_abs_path, &target_path]);
            if source_file_abs_path.exists() {
                info!("source {:?} will be removed", source_file_abs_path);
                tasks.push(ForgetTask::Delete(source_file_abs_path));
                continue;
            }

            let target_abs_path = PathBuf::from_iter(vec![&target_dir_abs_path, &target_path]);
            let target_abs_path = remove_dots_from_path(&target_abs_path);

            let plain_source = filepath_in_source_dir(
                &settings.dot_prefix, &target_dir_abs_path, &source_dir_abs_path,
                &target_abs_path, None,
            );
            let encrypted_source = filepath_in_source_dir(
                &settings.dot_prefix, &target_dir_abs_path, &source_dir_abs_path,
                &target_abs_path, Some(&settings.encrypted_postfix),
            );
            let symlink_source = filepath_in_source_dir(
                &settings.dot_prefix, &target_dir_abs_path, &source_dir_abs_path,
                &target_abs_path, Some(&settings.symlink_postfix),
            );

            let source_abs_path = if plain_source.exists() {
                plain_source
            } else if encrypted_source.exists() {
                encrypted_source
            } else if symlink_source.exists() {
                symlink_source
            } else {
                info!("source for {:?} does not exist, skipping...", target_path);
                continue;
            };

            info!("source {:?} will be removed", source_abs_path);
            tasks.push(ForgetTask::Delete(source_abs_path));
            continue;
        } else {
            let target_abs_path = target_abs_path_res?;
            if target_abs_path.starts_with(&source_dir_abs_path) {
                let source_abs_path = target_abs_path;
                debug!("target {:?} resides in source directory", source_abs_path);
                if source_abs_path.to_str().unwrap().ends_with(&settings.symlink_postfix) {
                    let source_symlink_file_abs_path = source_abs_path;
                    let source_rel_path = file_path_relative_to(&source_symlink_file_abs_path, &source_dir_abs_path);
                    let source_rel_str = source_rel_path.to_str().unwrap();
                    let target_rel_str = source_rel_to_target_rel(
                        source_rel_str, &settings.dot_prefix,
                        &settings.symlink_postfix, &settings.encrypted_postfix,
                    );
                    let target_symlink_abs_path = PathBuf::from_iter(vec![target_dir_abs_path.to_str().unwrap(), &target_rel_str]);
                    if target_symlink_abs_path.exists() {
                        let target_symlink_pointee_path = match fs::read_link(&target_symlink_abs_path) {
                            Ok(p) => p,
                            Err(e) => return Err(e.into()),
                        };
                        let source_file_content = fs::read_to_string(&source_symlink_file_abs_path).unwrap();
                        if source_file_content.trim().eq(target_symlink_pointee_path.to_str().unwrap()) {
                            info!("target symlink {:?}\n\tpoints to {:?}, skipping...", target_symlink_abs_path, target_symlink_pointee_path.to_str().unwrap());
                            tasks.push(ForgetTask::Delete(source_symlink_file_abs_path));
                            continue;
                        } else {
                            info!("target symlink {:?}\n\tpoints to {:?},\n\tmust point to {:?}", target_symlink_abs_path, target_symlink_pointee_path.to_str().unwrap(), source_file_content);
                            if *force {
                                tasks.push(ForgetTask::Delete(source_symlink_file_abs_path));
                            } else {
                                info!("specify --force to delete source {:?}", source_symlink_file_abs_path);
                            }
                            continue;
                        }
                    }
                } else {
                    info!("source {:?} will be removed", source_abs_path);
                    tasks.push(ForgetTask::Delete(source_abs_path));
                    continue;
                }
            } else if target_abs_path.starts_with(&target_dir_abs_path) {
                debug!("target {:?} resides in target directory", target_abs_path);

                let plain_source = filepath_in_source_dir(&settings.dot_prefix, &target_dir_abs_path, &source_dir_abs_path, &target_abs_path, None);
                let encrypted_source = filepath_in_source_dir(&settings.dot_prefix, &target_dir_abs_path, &source_dir_abs_path, &target_abs_path, Some(&settings.encrypted_postfix));

                let source_abs_path = if plain_source.exists() {
                    plain_source
                } else if encrypted_source.exists() {
                    encrypted_source
                } else {
                    info!("source for {:?} does not exist, skipping...", target_abs_path);
                    continue;
                };

                let sync_time_opt = get_sync_time(state, &source_abs_path, &source_dir_abs_path);

                let cmp = compare_files(&settings.encrypted_postfix, &target_abs_path, &source_abs_path, sync_time_opt)?;
                if CompareByTimestamp::SourceModified == cmp {
                    if *force {
                        info!("source {:?} was modified, removing source", source_abs_path);
                        tasks.push(ForgetTask::Delete(source_abs_path.clone()));
                    } else {
                        warn!("source {:?} was modified, use --force to remove", source_abs_path);
                        error_messages.push("source was modified".into());
                    }
                    continue;
                }

                if CompareByTimestamp::BothModified == cmp {
                    if *force {
                        info!("source {:?} and target {:?} were both modified, removing source", source_abs_path, target_abs_path);
                        tasks.push(ForgetTask::Delete(source_abs_path.clone()));
                    } else {
                        warn!("source {:?} and target {:?} were both modified, use --force to remove", source_abs_path, target_abs_path);
                        error_messages.push("source and target were modified".into());
                    }
                    continue;
                }
                if CompareByTimestamp::TargetModified == cmp {
                    if *force {
                        info!("target {:?} was modified, removing source", target_abs_path);
                        tasks.push(ForgetTask::Delete(source_abs_path.clone()));
                    } else {
                        warn!("target {:?} was modified, use --force to remove", target_abs_path);
                        error_messages.push("target was modified".into());
                    }
                    continue;
                }
                info!("source {:?} will be removed", source_abs_path);
                tasks.push(ForgetTask::Delete(source_abs_path));
                continue;
            } else {
                warn!("target {:?}\n\tresides outside the target directory {:?}, skipping...", target_abs_path, target_dir_abs_path);
                continue;
            }
        }
    }
    progress.clear();

    // Build set of state keys already covered by tasks
    let mut processed_keys: HashSet<String> = HashSet::new();
    for task in &tasks {
        match task {
            ForgetTask::Delete(source_abs) => {
                processed_keys.insert(source_to_state_key(source_abs, &source_dir_abs_path));
            },
            ForgetTask::RemoveState(key) => {
                processed_keys.insert(key.clone());
            },
        }
    }

    // Process state entries not covered by the traversal (orphaned / unpulled).
    // Only run when no explicit paths were given ("forget all" mode).
    let orphan_keys: Vec<String> = if forget_all {
        state.syncs.keys()
            .filter(|k| !processed_keys.contains(k.as_str()))
            .cloned()
            .collect()
    } else {
        vec![]
    };
    let orphan_total = orphan_keys.len();
    for (i, key) in orphan_keys.into_iter().enumerate() {
        report_progress(&mut progress, i + 1, orphan_total);
        let source_abs = PathBuf::from_iter([source_dir_abs_path.to_str().unwrap(), &key]);
        let source_abs = remove_dots_from_path(&source_abs);

        if source_abs.exists() {
            let sync_time = &state.syncs[&key];
            let source_meta = source_abs.metadata()?;
            let source_mtime = source_meta.modified()?;
            if source_mtime > sync_time.mtime {
                if *force {
                    info!("source {:?} was modified, removing with --force", key);
                    tasks.push(ForgetTask::Delete(source_abs));
                } else {
                    warn!("source {:?} was modified, use --force to remove", key);
                    error_messages.push(format!("source {:?} was modified", key));
                }
            } else {
                info!("source {:?} will be removed", source_abs);
                tasks.push(ForgetTask::Delete(source_abs));
            }
        } else {
            info!("source for {:?} does not exist, removing state entry", key);
            tasks.push(ForgetTask::RemoveState(key));
        }
    }
    progress.clear();

    if !error_messages.is_empty() {
        let joined = format!("forget failed: {}", error_messages.join("; "));
        error!("{}", joined);
        require_force(*force, joined)?;
    }

    if tasks.is_empty() {
        info!("{}", msg_nothing_to_do());
        return Ok(());
    }

    if dry_run {
        info!("{}", msg_dry_run());
    }

    debug!("::remove procedure begins, {} tasks", tasks.len());

    // Phase 1: Delete source files (best-effort, never abort mid-phase)
    let mut delete_errors: Vec<(String, String)> = Vec::new();
    for task in &tasks {
        let ForgetTask::Delete(source_file) = task else { continue; };
        info!("delete {:?}", source_file);
        if dry_run {
            continue;
        }
        if let Err(e) = fs::remove_file(source_file) {
            if e.kind() == std::io::ErrorKind::NotFound {
                debug!("{:?} was already removed, skipping", source_file);
            } else {
                warn!("failed to delete {:?}: {}", source_file, e);
                delete_errors.push((source_file.to_str().unwrap().to_string(), e.to_string()));
            }
        }
    }

    // Phase 2: Remove state entries for all processed files (infallible)
    for task in &tasks {
        match task {
            ForgetTask::Delete(source_file) => {
                remove_sync_state(state, source_file, &source_dir_abs_path);
            },
            ForgetTask::RemoveState(key) => {
                state.syncs.remove(key);
            },
        }
    }

    // Phase 3: Clean up empty parent directories (best-effort)
    for task in &tasks {
        let ForgetTask::Delete(source_file) = task else { continue; };
        if dry_run {
            continue;
        }
        let mut parent_opt = source_file.parent();
        while let Some(dir) = parent_opt {
            parent_opt = dir.parent();
            if dir != source_dir_abs_path && dir.starts_with(&source_dir_abs_path) {
                let mut read_dir_entries = match dir.read_dir() {
                    Ok(entries) => entries,
                    Err(_) => break,
                };
                if read_dir_entries.next().is_none() {
                    if let Err(e) = fs::remove_dir(dir) {
                        warn!("failed to remove empty directory {:?}: {}", dir, e);
                    }
                } else {
                    break;
                }
            } else {
                break;
            }
        }
    }

    if !delete_errors.is_empty() {
        error!("some source files could not be deleted: {:?}", delete_errors);
        let summary: String = delete_errors.iter()
            .map(|(path, err)| format!("{}: {}", path, err))
            .collect::<Vec<_>>()
            .join("; ");
        return Err(DfmError::Other(format!("failed to delete source files: {}", summary)));
    }

    Ok(())
}
