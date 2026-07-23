use std::fs;
use std::io::Write;
use log::{debug, error, info};
use regex::Regex;

use dfm::*;
use crate::{Args, Command, DfmError};
use super::resolve_dry_run;

pub fn ignore_command(settings: &Settings, args: &Args) -> Result<(), DfmError> {
    let Command::Ignore {
        paths,
        patterns,
        dry_run,
        ..
    } = &args.command else {
        return Err(DfmError::Unsupported(format!("unreachable code reached: command {:?} is not `ignore`", args.command)));
    };

    let dry_run = resolve_dry_run(*dry_run, args.dry_run);

    debug!("ignore paths {:?}, patterns {:?}, dry-run {}", paths, patterns, dry_run);

    let (target_dir_abs_path, source_dir_abs_path) = calc_working_dir_paths(&settings)?;
    let target_ignore_file_path = calc_local_ignore_file()?;
    let target_ignore_regex = load_ignore_regex(&target_ignore_file_path)?;

    let source_ignore_file_path = calc_source_ignore_file(&source_dir_abs_path)?;
    let source_ignore_regex = load_ignore_regex(&source_ignore_file_path)?;

    let traversed_paths = match paths {
        Some(p) => p,
        None if patterns.is_none() => {
            let ListDirectories {
                found: traversed_paths,
                errors: error_messages,
                ..
            } = list_directory(&vec![target_dir_abs_path.clone()], Some(&target_ignore_regex))?;

            if !error_messages.is_empty() {
                return Err(DfmError::InvalidData(
                    format!("failed to process some subdirectories or files in targets {:?}", error_messages)
                ));
            }

            &traversed_paths.clone()
        },
        None => &vec![]
    };

    debug!("traversing result is {:?}", traversed_paths);

    let mut target_ignore_paths = vec![];
    let mut source_ignore_paths = vec![];

    for path in traversed_paths {
        debug!("check path {:?}", path);
        let abs_path = fs::canonicalize(path)?;

        // TODO check if file to be ignored is already added to source then
        //  report error, ignore is failed.

        if abs_path.starts_with(&source_dir_abs_path) {
            let rel_path = file_path_relative_to(&abs_path, &source_ignore_file_path);
            if source_ignore_regex.matches(rel_path.to_str().unwrap()).matched_any() {
                info!("source path {:?} is ignored already", path);
                continue;
            } else {
                debug!("adding path {:?} to source ignore file {:?}", path, source_ignore_file_path);
                source_ignore_paths.push(path);
                continue;
            }
        }

        if abs_path.starts_with(&target_dir_abs_path) {
            let rel_path = file_path_relative_to(&abs_path, &target_ignore_file_path);
            if target_ignore_regex.matches(rel_path.to_str().unwrap()).matched_any() {
                info!("target path {:?} is ignored already", path);
                continue;
            } else {
                debug!("adding path {:?} to target ignore file {:?}", path, target_ignore_file_path);
                target_ignore_paths.push(path);
                continue;
            }
        }

        debug!("path {:?} was not processed", path);
    }

    let mut target_ignore_regexps = vec![];

    if let Some(patterns_args) = patterns  {
        for pattern in patterns_args {
            if let Err(e) = Regex::new(pattern) {
                error!("argument is invalid, {}", e);
                return Err(DfmError::other(e));
            }

            debug!("adding regex /{}/", pattern);
            target_ignore_regexps.push(pattern);
        }
    }

    if target_ignore_paths.is_empty() &&
        source_ignore_paths.is_empty() &&
        target_ignore_regexps.is_empty()
    {
        info!("nothing to do");
        return Ok(());
    }

    if dry_run {
        info!("dry run specified, no changes will be made");
    }

    debug!("::ignore procedure begins");

    if !target_ignore_paths.is_empty() {
        let mut target_ignore_file = open_or_create_target_ignore_file()?;
        for ignore_path in target_ignore_paths {
            info!("add path {:?} to {:?}", ignore_path, target_ignore_file_path);
            if dry_run {
                continue;
            }

            let escaped_path_str = regex::escape(ignore_path.to_str().unwrap());
            if let Err(e) = writeln!(target_ignore_file, "{}", escaped_path_str) {
                error!("failed write path to file: {}", e);
                return Err(e.into());
            }
        }
    }

    if !target_ignore_regexps.is_empty() {
        let mut target_ignore_file = open_or_create_target_ignore_file()?;
        for pattern in target_ignore_regexps {
            info!("add regex /{}/ to {:?}", pattern, target_ignore_file_path);
            if dry_run {
                continue;
            }

            if let Err(e) = writeln!(target_ignore_file, "{}", pattern) {
                error!("failed write regex to file: {}", e);
                return Err(e.into());
            }
        }
    }

    if !source_ignore_paths.is_empty() {
        let mut source_ignore_file = open_or_create_file(&source_ignore_file_path)?;
        for ignore_path in source_ignore_paths {
            info!("add path {:?} to {:?}", ignore_path, source_ignore_file_path);
            if dry_run {
                continue;
            }

            let escaped_path_str = regex::escape(ignore_path.to_str().unwrap());
            if let Err(e) = writeln!(source_ignore_file, "{}", escaped_path_str) {
                error!("failed write path to file: {}", e);
                return Err(e.into());
            }
        }
    }

    Ok(())
}
