use std::env;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use log::{debug, info};
use regex::Regex;

/// Ensure that FILE (opened in append mode) starts writing on a fresh line —
/// if the file is non-empty and does not end with `\n`, write one first.
fn ensure_trailing_newline(path: &PathBuf) -> Result<(), DfmError> {
    if !path.exists() {
        return Ok(());
    }
    let content = fs::read_to_string(path)?;
    if !content.is_empty() && !content.ends_with('\n') {
        let mut f = fs::OpenOptions::new()
            .write(true)
            .append(true)
            .open(path)?;
        writeln!(f)?;
    }
    Ok(())
}

use dfm::*;
use crate::{Args, Command, DfmError};
use super::{resolve_dry_run, msg_dry_run, msg_nothing_to_do};

pub fn ignore_command(settings: &Settings, args: &Args) -> Result<(), DfmError> {
    let Command::Ignore {
        paths,
        patterns,
        remove,
        dry_run,
    } = &args.command else {
        return Err(DfmError::Unsupported(format!("unreachable code reached: command {:?} is not `ignore`", args.command)));
    };

    let dry_run = resolve_dry_run(*dry_run, args.dry_run);

    debug!("ignore paths {:?}, patterns {:?}, remove {:?}, dry-run {}", paths, patterns, remove, dry_run);

    if let Some(records) = remove {
        return remove_ignore_records(records, dry_run);
    }

    let (target_dir_abs_path, source_dir_abs_path) = calc_working_dir_paths(&settings)?;
    let local_ignore_file_path = calc_local_ignore_file()?;
    let target_ignore_regex = load_ignore_regex(&local_ignore_file_path)?;

    let source_ignore_file_path = calc_source_ignore_file(&source_dir_abs_path)?;
    let source_ignore_regex = load_ignore_regex(&source_ignore_file_path)?;

    let traversed_paths = match paths {
        Some(p) => p,
        None => &vec![]
    };

    debug!("traversing result is {:?}", traversed_paths);

    let mut target_ignore_paths = vec![];
    let mut source_ignore_lines = vec![];

    for path in traversed_paths {
        debug!("check path {:?}", path);
        let (abs_path, canonicalize_failed) = match fs::canonicalize(path) {
            Ok(p) => (p, false),
            Err(_) => {
                // File doesn't exist on disk (e.g., unpulled managed file).
                // Build a conceptual absolute path so starts_with checks work.
                let p = PathBuf::from(path);
                let abs = if p.is_relative() {
                    env::current_dir()?.join(&p)
                } else {
                    p
                };
                (abs, true)
            }
        };

        // TODO check if file to be ignored is already added to source then
        //  report error, ignore is failed.

        if abs_path.starts_with(&source_dir_abs_path) {
            let rel_path = file_path_relative_to(&abs_path, &source_dir_abs_path);
            // Ignoring a source-side file should ignore the actual target file,
            // so the stored entry is the source-relative path with the dot
            // prefix stripped (e.g. source `dot_my.log` -> target `.my.log`
            // stored as `^my\.log$`).
            let canonical = rel_path.to_str().unwrap().replace(&settings.dot_prefix, "");
            let canonical_line = format!("^{}$", regex::escape(&canonical));
            let matched = check_path_matches_regex_component_wise(&source_ignore_regex, &rel_path)
                .or_else(|| check_path_matches_regex_component_wise(&source_ignore_regex, &PathBuf::from(&canonical)));
            if let Some(matched) = matched {
                if matched == canonical_line {
                    info!("source path {:?} is ignored already", path);
                } else {
                    info!("source path {:?} is ignored already, migrating entry {:?} to {:?}", path, matched, canonical_line);
                    if !dry_run {
                        migrate_ignore_line(&source_ignore_file_path, &matched, &canonical_line)?;
                    }
                }
                continue;
            } else {
                debug!("adding path {:?} to source ignore file {:?}", path, source_ignore_file_path);
                source_ignore_lines.push(canonical_line);
                continue;
            }
        }

        if abs_path.starts_with(&target_dir_abs_path) {
            let rel_path = file_path_relative_to(&abs_path, &target_dir_abs_path);
            if check_path_matches_regex_component_wise(&target_ignore_regex, &rel_path).is_some() {
                info!("target path {:?} is ignored already", path);
                continue;
            } else {
                debug!("adding path {:?} to target ignore file {:?}", path, local_ignore_file_path);
                target_ignore_paths.push(path);
                continue;
            }
        }

        if canonicalize_failed {
            return Err(DfmError::InvalidInput(format!(
                "path {:?} does not exist", path
            )));
        }

        debug!("path {:?} was not processed", path);
    }

    let mut target_ignore_regexps = vec![];

    if let Some(patterns_args) = patterns  {
        for pattern in patterns_args {
            if let Err(e) = Regex::new(pattern) {
                return Err(DfmError::other(format!("invalid regex pattern: {}", e)));
            }

            debug!("adding regex /{}/", pattern);
            target_ignore_regexps.push(pattern);
        }
    }

    if target_ignore_paths.is_empty() &&
        source_ignore_lines.is_empty() &&
        target_ignore_regexps.is_empty()
    {
        info!("{}", msg_nothing_to_do());
        return Ok(());
    }

    if dry_run {
        info!("{}", msg_dry_run());
    }

    debug!("adding ignore records to local ignore file {:?}", local_ignore_file_path);

    if !target_ignore_paths.is_empty() {
        if !dry_run {
            ensure_trailing_newline(&local_ignore_file_path)?;
        }
        let mut target_ignore_file = open_or_create_target_ignore_file()?;
        for ignore_path in target_ignore_paths {
            info!("add path {:?} to {:?}", ignore_path, local_ignore_file_path);
            if dry_run {
                continue;
            }

            let escaped_path_str = regex::escape(ignore_path.to_str().unwrap());
            if let Err(e) = writeln!(target_ignore_file, "{}", escaped_path_str) {
                return Err(e.into());
            }
        }
    }

    if !target_ignore_regexps.is_empty() {
        if !dry_run {
            ensure_trailing_newline(&local_ignore_file_path)?;
        }
        let mut target_ignore_file = open_or_create_target_ignore_file()?;
        for pattern in target_ignore_regexps {
            info!("add regex /{}/ to {:?}", pattern, local_ignore_file_path);
            if dry_run {
                continue;
            }

            if let Err(e) = writeln!(target_ignore_file, "{}", pattern) {
                return Err(e.into());
            }
        }
    }

    if !source_ignore_lines.is_empty() {
        if !dry_run {
            ensure_trailing_newline(&source_ignore_file_path)?;
        }
        let mut source_ignore_file = open_or_create_file(&source_ignore_file_path)?;
        for ignore_line in source_ignore_lines {
            info!("add path {:?} to {:?}", ignore_line, source_ignore_file_path);
            if dry_run {
                continue;
            }

            if let Err(e) = writeln!(source_ignore_file, "{}", ignore_line) {
                return Err(e.into());
            }
        }
    }

    Ok(())
}

/// Replace `old` with `new` in the ignore file, keeping all other lines.
/// If `new` is already present, `old` is only dropped (no duplicate).
fn migrate_ignore_line(ignore_file_path: &PathBuf, old: &str, new: &str) -> Result<(), DfmError> {
    let content = fs::read_to_string(ignore_file_path)?;
    let out: Vec<String> = content
        .lines()
        .filter(|l| *l != old)
        .map(|l| l.to_string())
        .collect();
    let out = if out.iter().any(|l| *l == new) {
        out
    } else {
        let mut out = out;
        out.push(new.to_owned());
        out
    };
    let mut text = out.join("\n");
    if !text.is_empty() {
        text.push('\n');
    }
    fs::write(ignore_file_path, text)?;
    Ok(())
}

fn remove_ignore_records(records: &[String], dry_run: bool) -> Result<(), DfmError> {
    let ignore_file_path = calc_local_ignore_file()?;

    if !ignore_file_path.exists() {
        info!("ignore file {:?} does not exist, nothing to remove", ignore_file_path);
        return Ok(());
    }

    let content = fs::read_to_string(&ignore_file_path)?;
    let lines: Vec<&str> = content.lines().collect();
    let mut removed = Vec::new();
    let remaining: Vec<String> = lines
        .iter()
        .filter(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() || records.iter().any(|r| {
                r.as_str() == trimmed || regex::escape(r.as_str()) == trimmed
            }) {
                if !trimmed.is_empty() {
                    removed.push(trimmed.to_string());
                }
                false
            } else {
                true
            }
        })
        .map(|l| l.to_string())
        .collect();

    if removed.is_empty() {
        info!("no matching records found in ignore file");
        return Ok(());
    }

    if dry_run {
        info!("dry run specified, would remove {} record(s) from {:?}: {:?}",
              removed.len(), ignore_file_path, removed);
        return Ok(());
    }

    fs::write(&ignore_file_path, remaining.join("\n"))?;
    info!("removed {} record(s) from {:?}: {:?}",
          removed.len(), ignore_file_path, removed);
    Ok(())
}
