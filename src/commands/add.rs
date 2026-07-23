use std::{env, fs};
use std::fs::File;
use std::io::Write;
use crate::DfmError;
use std::path::PathBuf;
use std::time::SystemTime;

use filetime_creation::FileTime;

use log::{debug, error, info, warn};
use regex::RegexSet;

use dfm::*;
use crate::{Args, Command};
use super::{sync_file_copy, resolve_dry_run, require_force};

pub fn add_command(settings: &Settings, args: &Args, state: &mut StateObject) -> Result<(), DfmError> {
    let Command::Add {
        paths,
        merge,
        allow_foreign: foreign,
        force,
        symlink,
        encrypt,
        dry_run,
    } = &args.command else {
        return Err(DfmError::Unsupported(format!("unreachable code reached: command {:?} is not `add`", args.command)));
    };

    let dry_run = resolve_dry_run(*dry_run, args.dry_run);

    debug!("add paths {:?}, merge {}, foreign {}, force {}, symlink {}, encrypt {}", paths, merge, foreign, force, symlink, encrypt);

    if *symlink && *encrypt {
        error!("Cannot encrypt source for symlink target");
        return Err(DfmError::other("wrong arguments"));
    }

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

    let target_ignore_file_path = calc_local_ignore_file()?;
    let target_ignore_regex = load_ignore_regex(&target_ignore_file_path)?;

    #[derive(Debug)]
    enum AddTask {
        Copy(PathBuf, PathBuf),
        CopyEncryptedFile(PathBuf, PathBuf),
        CreateSymlinkFilePointer(PathBuf, String),
        CopyAndSymlink(PathBuf, PathBuf),
    }

    let mut tasks: Vec<AddTask> = Vec::new();

    debug!("::check state procedure begins");

    let mut conflict_detected = false;
    let mut error_messages = vec![];

    for target_path in traversed_paths.iter() {
        debug!("checking {:?}", target_path);

        let target_path = if target_path.is_symlink() {
            debug!("target {:?} is a symlink", target_path);

            if *encrypt {
                error!("Cannot encrypt source for symlink target");
                error_messages.push(format!("Target {:?} is a symlink, encryption is impossible", target_path));
                continue; // error
            }

            let current_dir = env::current_dir()?;

            let target_symlink_abs_path_raw = PathBuf::from_iter(vec![current_dir, target_path.clone()]);
            let root = PathBuf::from("/");
            let mut target_symlink_abs_path = fs::canonicalize(target_symlink_abs_path_raw.parent().get_or_insert(&root))?;
            target_symlink_abs_path.push(target_symlink_abs_path_raw.file_name()
                .ok_or_else(|| DfmError::InvalidInput("path has no file name".into()))?);
            let target_symlink_abs_path = target_symlink_abs_path;

            if let Some(pattern) = check_path_matches_regex(&target_ignore_regex, &target_symlink_abs_path) {
                info!("target symlink {:?} is ignored by regex /{}/ in file {:?}", target_symlink_abs_path, pattern, target_ignore_file_path);
                continue;
            }

            let target_symlink_pointee_rel_path = fs::read_link(&target_symlink_abs_path)?;
            let target_symlink_pointee_abs_path = fs::canonicalize(&target_symlink_pointee_rel_path)?;
            debug!("target symlink {:?}\n\tpoints to {:?}", target_symlink_abs_path, target_symlink_pointee_abs_path);

            let source_symlink_file_abs_path = filepath_in_source_dir(
                &settings.dot_prefix, &target_dir_abs_path, &source_dir_abs_path,
                &target_symlink_abs_path, Some(&settings.symlink_postfix)
            );
            let source_symlink_file_exists = source_symlink_file_abs_path.exists();
            let source_symlink_file_points_to_right_target = if source_symlink_file_exists {
                 match fs::read_to_string(&source_symlink_file_abs_path) {
                    Ok(file_content) => {
                        debug!("source symlink file {:?}\n\tpoints to \"{}\"", source_symlink_file_abs_path, file_content);
                        file_content.trim().eq(target_symlink_pointee_rel_path.to_str().unwrap())
                    },
                    _ => false
                }
            } else {
                false
            };
            if *force || source_symlink_file_exists && !source_symlink_file_points_to_right_target {
                if !source_symlink_file_points_to_right_target {
                    debug!("source symlink file points to the wrong file, must be {:?}", &target_symlink_pointee_rel_path);
                }
                tasks.push(AddTask::CreateSymlinkFilePointer(source_symlink_file_abs_path.clone(), target_symlink_pointee_rel_path.to_str().unwrap().to_owned()));
            } else if source_symlink_file_points_to_right_target {
                debug!("for target symlink {:?},\n\tsource symlink file {:?} already exists, skipping...", target_symlink_abs_path, source_symlink_file_abs_path);
            } else if !target_symlink_pointee_abs_path.starts_with(&source_dir_abs_path) {
                debug!("for target symlink {:?},\n\tdoes not have a source symlink file {:?}", target_symlink_abs_path, source_symlink_file_abs_path);
                tasks.push(AddTask::CreateSymlinkFilePointer(source_symlink_file_abs_path.clone(), target_symlink_pointee_rel_path.to_str().unwrap().to_owned()));
            } else {
                debug!("target symlink {:?}\n\tpointee is managed as {:?}", source_symlink_file_abs_path, target_symlink_pointee_abs_path);
            };
            target_symlink_pointee_abs_path
        } else {
            target_path.clone()
        };

        // target is not a symlink

        let target_abs_path = fs::canonicalize(&target_path)?;

        if target_abs_path.starts_with(&source_dir_abs_path) {
            info!("target {:?} resides in source directory, ignoring", target_abs_path);
            continue;
        }

        if !target_abs_path.starts_with(&target_dir_abs_path) {
            info!("target {:?} does not reside in target directory {:?}, skipping...", target_abs_path, target_dir_abs_path);
            continue;
        }

        if let Some(pattern) = check_path_matches_regex(&target_ignore_regex, &target_abs_path) {
            println!("target {:?} is ignored by regex /{}/ in file {:?}", target_abs_path, pattern, target_ignore_file_path);
            continue;
        }

        let to_be_encrypted_regex_set = RegexSet::new(settings.force_encryption_for.iter().map(|r| r.as_str().to_owned())).unwrap();
        let encrypt = if let Some(pattern) = check_path_matches_regex(&to_be_encrypted_regex_set, &target_abs_path) {
            debug!("target {:?} is forced to be encrypted by regex /{}/ from config file", target_abs_path, pattern);
            true
        } else {
            *encrypt
        };

        let encrypted_source_abs_path = filepath_in_source_dir(&settings.dot_prefix, &target_dir_abs_path, &source_dir_abs_path, &target_abs_path, Some(&settings.encrypted_postfix));
        let regular_source_abs_path = filepath_in_source_dir(&settings.dot_prefix, &target_dir_abs_path, &source_dir_abs_path, &target_abs_path, None);

        let (source_is_encrypted, source_abs_path) = if encrypted_source_abs_path.exists() || encrypt {
            if regular_source_abs_path.exists() {
                warn!("target must be encrypted but unencrypted source is present {:?}", source_dir_abs_path);
                // FIXME check if source is modified
                // if not modified then remove it, and create and encrypted copy of target instead of it
                // if only source modified then ???
                // if both modified then ???
            }
            (true, encrypted_source_abs_path)
        } else {
            (false, regular_source_abs_path)
        };

        // NOTE: directories are already handled by list_directory — it traverses and
        // returns individual files, which are then encrypted one-by-one.

        debug!("analysing source file {:?}", source_abs_path);

        // check if a conflict could take a place
        if source_abs_path.exists() {
            let source_file_rel_path = file_path_relative_to(&source_abs_path, &source_dir_abs_path);
            let source_file_rel_path = remove_dots_from_path(&source_file_rel_path);
            let sync_time_opt = state.syncs.get(source_file_rel_path.to_str().unwrap());

            let cmp = compare_files_by_timestamps(&target_abs_path, &source_abs_path, sync_time_opt)?;

            // conflict cases
            match cmp {
                CompareByTimestamp::BothModified => {
                    println!("both target {:?} and source {:?} were modified independently, `add` on this target will overwrite source",
                        target_abs_path, source_abs_path);
                    conflict_detected = true;
                    if !force {
                        continue;
                    }
                },
                CompareByTimestamp::SourceModified => {
                    println!("source {:?} was modified, `add`ing the target {:?} will overwrite changes in source.",
                              source_abs_path, target_abs_path);
                    conflict_detected = true;
                    if !force {
                        continue;
                    }
                },
                CompareByTimestamp::NonModified => {
                    println!("neither target nor source were modified");
                    // conflict_detected = true;
                    // TODO check if file content is not different
                    if !force {
                        continue;
                    }
                },
                CompareByTimestamp::TargetModified => {
                    println!("only target {:?} was modified, no conflicts", target_abs_path);
                },
                CompareByTimestamp::NeverSynchronized => {
                    if !force {
                        warn!("target {:?}\n\tand source {:?}\n\twere not synchronized.", target_abs_path, source_abs_path);
                        warn!("Use --force to replace source with target");
                        continue; // TODO error?
                    }
                },
            }

            info!("no conflict detected for target {:?}", target_abs_path);
        } else {
            info!("source file {:?} does not exist", source_abs_path);
        }

        if *symlink && (encrypt || source_is_encrypted) {
            error!("Cannot combine --symlink with encryption for {:?}", target_abs_path);
            error_messages.push(format!("Target {:?} is encrypted but --symlink was requested", target_abs_path));
        } else if encrypt || source_is_encrypted {
            tasks.push(AddTask::CopyEncryptedFile(target_abs_path, source_abs_path));
        } else if *symlink {
            tasks.push(AddTask::CopyAndSymlink(target_abs_path, source_abs_path));
        } else {
            tasks.push(AddTask::Copy(target_abs_path, source_abs_path));
        }
    }

    if !error_messages.is_empty() {
        for error_message in &error_messages {
            error!("{}", error_message);
        }
        require_force(*force, "error occurred")?;
    }

    if dry_run {
        info!("dry run specified, no changes will be made");
    }

    if conflict_detected {
        // require_force ensures we only error without --force
        require_force(*force, "conflicts")?;
        warn!("conflicts detected, proceeding with --force");
    }

    if tasks.is_empty() {
        info!("nothing to do");
        return Ok(());
    }

    debug!("::copy procedure begins, {} tasks", tasks.len());

    for task in tasks {
        match task {
            AddTask::Copy(target_file, source_file) => {
                info!("copy target {:?} to source {:?}", target_file, source_file);
                if dry_run {
                    continue;
                }

                sync_file_copy(&target_file, &source_file, &source_file, state, &source_dir_abs_path)?;
            },
            AddTask::CopyAndSymlink(target_file, source_file) => {
                info!("copy target {:?} to source {:?} and replace target with symlink", target_file, source_file);
                if dry_run {
                    continue;
                }

                // 1. Copy file content to source
                sync_file_copy(&target_file, &source_file, &source_file, state, &source_dir_abs_path)?;

                // 2. Remove the original target file
                fs::remove_file(&target_file)?;

                // 3. Create a symlink at the target pointing to the source file
                let target_parent = target_file.parent()
                    .ok_or_else(|| DfmError::other("target file has no parent directory"))?
                    .to_path_buf();
                let link_target = file_path_relative_to(&source_file, &target_parent);
                symlink::symlink_file(&link_target, &target_file)?;
            },
            AddTask::CopyEncryptedFile(target_file, source_file) => {
                info!("copy encrypted target {:?} to source {:?}", target_file, source_file);
                if dry_run {
                    continue;
                }

                dfm::crypt::write_zip_file(settings, &target_file, &source_file)?;

                // Record sync time and sync mtimes (same as sync_file_copy for Copy)
                let sync_creation = SystemTime::now();
                let source_rel_path = file_path_relative_to(&source_file, &source_dir_abs_path);
                let source_rel_path = remove_dots_from_path(&source_rel_path);
                state.syncs.insert(source_rel_path.to_str().unwrap().to_string(), sync_creation);

                let ft = FileTime::from_system_time(sync_creation);
                filetime_creation::set_file_mtime(&target_file, ft)?;
                filetime_creation::set_file_mtime(&source_file, ft)?;
            },
            AddTask::CreateSymlinkFilePointer(source_symlink, points_to) => {
                info!("directing source symlink file {:?} to the pointee of the target symlink {:?}", source_symlink, points_to);
                if dry_run {
                    continue;
                }

                // open if exists or create, if it doesn't
                let mut symlink_file = File::create(&source_symlink)?;
                symlink_file.write(points_to.as_bytes())?;
            },
        }
    }
    Ok(())
}
