pub(crate) mod add;
pub(crate) mod config;
pub(crate) mod diff;
pub(crate) mod encrypt;
pub(crate) mod forget;
pub(crate) mod ignore;
pub(crate) mod init;
pub(crate) mod merge;
pub(crate) mod paths;
pub(crate) mod pull;
pub(crate) mod purge;
pub(crate) mod status;
pub(crate) mod sync;

pub(crate) use add::add_command;
pub(crate) use config::config_command;
pub(crate) use diff::{diff_command, diff_editable_command};
pub(crate) use encrypt::{encrypt_command, decrypt_command};
pub(crate) use forget::forget_command;
pub(crate) use ignore::ignore_command;
pub(crate) use init::init_command;
pub(crate) use merge::merge_command;
pub(crate) use paths::paths_command;
pub(crate) use pull::pull_command;
pub(crate) use purge::purge_command;
pub(crate) use status::status_command;
pub(crate) use sync::sync_command;

pub(crate) use add::AddArgs;
pub(crate) use config::ConfigArgs;
pub(crate) use diff::{DiffArgs, DiffEditableArgs};
pub(crate) use encrypt::{EncryptArgs, DecryptArgs};
pub(crate) use forget::ForgetArgs;
pub(crate) use ignore::IgnoreArgs;
pub(crate) use init::InitArgs;
pub(crate) use merge::MergeArgs;
pub(crate) use paths::PathsArgs;
pub(crate) use pull::PullArgs;
pub(crate) use purge::PurgeArgs;
pub(crate) use status::StatusArgs;
pub(crate) use sync::SyncArgs;

use std::fs;
use std::env;
use std::io::{IsTerminal, Write};
use std::process::Stdio;
use crate::DfmError;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use filetime_creation::{set_file_mtime, set_symlink_file_times, FileTime};
use microxdg::Xdg;
use log::{debug, error, info, trace, log_enabled};
use regex::RegexSet;

use dfm::*;

/// Split a command line template into its program and arguments. Returns
/// `(None, [])` for an empty/whitespace-only command.
pub(crate) fn split_command(command: &str) -> (Option<&str>, Vec<&str>) {
    let parts: Vec<&str> = command.split_whitespace().collect();
    match parts.split_first() {
        Some((prog, args)) => (Some(*prog), args.to_vec()),
        None => (None, Vec::new()),
    }
}

// Shared state helpers

/// Compute the state key (source-relative path) for an absolute path under a
/// root directory. This is the canonical key used throughout: `file_path_relative_to`
/// followed by dot-normalization, lossily stringified.
pub(crate) fn state_key_for(abs_path: &Path, root: &Path) -> String {
    let rel = file_path_relative_to(abs_path, root);
    let rel = remove_dots_from_path(&rel);
    rel.to_string_lossy().into_owned()
}

/// Map a source-relative path (state key) to the target-relative path and the
/// absolute target path, stripping the dot-prefix and the symlink/encrypted
/// postfixes. Shared by pull, merge, status and purge so every command maps a
/// source path to its target identically.
pub(crate) fn source_rel_to_target_abs(
    source_rel: &str,
    target_dir_abs: &Path,
    settings: &Settings,
) -> (String, PathBuf) {
    let target_rel = source_rel_to_target_rel(
        source_rel,
        &settings.dot_prefix,
        &settings.symlink_postfix,
        &settings.encrypted_postfix,
    );
    (target_rel.clone(), remove_dots_from_path(&target_dir_abs.join(&target_rel)))
}

/// Resolve a user-provided CLI path argument to an absolute path. Relative
/// paths are anchored at the **current working directory**, following normal
/// shell semantics; the result is normalized lexically. Absolute paths are
/// normalized as-is.
pub(crate) fn cli_path_to_abs(path: &Path) -> Result<PathBuf, DfmError> {
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    Ok(remove_dots_from_path(&abs))
}

/// `cli_path_to_abs` plus a scope check: the resolved path must lie under the
/// target or the source directory. Without this, a leading `..` (or an
/// absolute path elsewhere on the system) silently climbs out of the managed
/// tree and commands either misbehave (`status` prints mangled relative
/// entries) or produce confusing errors.
pub(crate) fn cli_path_in_scope(
    arg: &Path,
    target_dir_abs: &Path,
    source_dir_abs: &Path,
) -> Result<PathBuf, DfmError> {
    let abs = cli_path_to_abs(arg)?;
    if abs.starts_with(target_dir_abs) || abs.starts_with(source_dir_abs) {
        Ok(abs)
    } else {
        Err(DfmError::InvalidInput(format!(
            "path {:?} resolves to {}, which is outside the target directory {}",
            arg,
            abs.display(),
            target_dir_abs.display()
        )))
    }
}

/// Get the sync time for a file, computing the state key from its absolute path.
pub(crate) fn get_sync_time<'a>(
    state: &'a StateObject,
    path: &Path,
    source_dir: &Path,
) -> Option<&'a SyncTime> {
    state.syncs.get(state_key_for(path, source_dir).as_str())
}

/// Remove the sync entry for a file, computing the state key from its absolute path.
pub(crate) fn remove_sync_state(
    state: &mut StateObject,
    path: &Path,
    source_dir: &Path,
) {
    state.syncs.remove(state_key_for(path, source_dir).as_str());
}

/// Insert/update a sync entry and set mtimes on both the source-side file
/// and the target-side file.
pub(crate) fn update_sync_state(
    state: &mut StateObject,
    source_abs: &Path,
    target_abs: &Path,
    source_dir_abs: &Path,
) -> Result<(), DfmError> {
    let sync_creation = SystemTime::now();
    let source_rel_path = state_key_for(source_abs, source_dir_abs);
    let sha256 = compute_sha256(source_abs)?;
    state.syncs.insert(source_rel_path, SyncTime { mtime: sync_creation, sha256 });
    let ft = FileTime::from_system_time(sync_creation);
    if target_abs.is_symlink() {
        // `set_file_mtime` follows the link and touches the pointee — for
        // system-owned pointees (e.g. /usr/lib/systemd/user/*.socket) that is
        // EPERM and would abort the add. Set the mtime of the symlink itself.
        set_symlink_file_times(target_abs, ft, ft, ft).map_err(|e| io_err(target_abs, e))?;
    } else {
        set_file_mtime(target_abs, ft).map_err(|e| io_err(target_abs, e))?;
    }
    set_file_mtime(source_abs, ft).map_err(|e| io_err(source_abs, e))?;
    Ok(())
}

/// Thin wrapper around `list_directory` that bundles the error check.
pub(crate) fn list_directory_or_error(
    paths: &[PathBuf],
    rel_base: &PathBuf,
    filter: Option<TraversalFilter<'_>>,
    context: &str,
) -> Result<Vec<PathBuf>, DfmError> {
    let ListDirectories { found, errors, .. } = list_directory(paths, rel_base, filter)?;
    if !errors.is_empty() {
        return Err(DfmError::InvalidData(
            format!("failed to process some subdirectories or files {}: {:?}", context, errors)
        ));
    }
    Ok(found)
}

/// Report a periodic progress heartbeat during bulk analysis loops.
/// No-op unless the batch is large enough to be worth reporting,
/// so small, fast operations stay quiet.
///
/// Each update overwrites the single progress line in place (`\r`), and the
/// caller erases the line when the operation is done via `progress.clear()`.
/// Progress is written straight to stderr (not through the `log` crate) so it
/// is visible at every verbosity level, including `-v 0`.
pub(crate) fn report_progress(progress: &mut ProgressLine, done: usize, total: usize) {
    const BULK_PROGRESS_MIN: usize = 100;
    // Scale the step with the batch size so large operations still only
    // update the line about 20 times per run.
    let step = (total / 20).max(BULK_PROGRESS_MIN);
    if total >= BULK_PROGRESS_MIN && (done.is_multiple_of(step) || done == total) {
        progress.set(&format!("processed {}/{} files", done, total));
    }
}

// Shared message templates

/// Message printed when --dry-run is active.
pub(crate) fn msg_dry_run() -> &'static str {
    "dry run specified, no changes will be made"
}

/// Message printed when there is nothing to do.
pub(crate) fn msg_nothing_to_do() -> &'static str {
    "nothing to do"
}

/// Message printed when a multi-file command fails partway through.
pub(crate) fn msg_tasks_failure(completed: usize, total: usize) -> String {
    format!("{} of {} tasks completed before failure", completed, total)
}

// Shared symlink-pointer / source-variant helpers

/// Read a symlink pointer file's content with surrounding whitespace trimmed.
pub(crate) fn read_symlink_pointer(pointer_file: &Path) -> Result<String, DfmError> {
    Ok(fs::read_to_string(pointer_file)
        .map_err(|e| io_err(pointer_file, e))?
        .trim().to_string())
}

/// Whether a symlink pointer file's (trimmed) content equals the pointee path.
pub(crate) fn symlink_pointer_matches(pointer_file: &Path, pointee: &str) -> Result<bool, DfmError> {
    Ok(read_symlink_pointer(pointer_file)? == pointee)
}

/// Which storage form a source file takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SourceVariant {
    Plain,
    Encrypted,
    Symlink,
}

/// Resolve the existing source counterpart(s) for `target_abs_path`, tried in
/// priority order: plain, encrypted, symlink pointer. Returns the first that
/// exists on disk, or `None` when the target has no source at all.
pub(crate) fn resolve_source_variant(
    settings: &Settings,
    target_dir_abs_path: &Path,
    source_dir_abs_path: &Path,
    target_abs_path: &Path,
) -> Option<(SourceVariant, PathBuf)> {
    let plain = filepath_in_source_dir(
        &settings.dot_prefix, target_dir_abs_path, source_dir_abs_path, target_abs_path, None,
    );
    if plain.exists() {
        return Some((SourceVariant::Plain, plain));
    }
    let encrypted = filepath_in_source_dir(
        &settings.dot_prefix, target_dir_abs_path, source_dir_abs_path, target_abs_path,
        Some(&settings.encrypted_postfix),
    );
    if encrypted.exists() {
        return Some((SourceVariant::Encrypted, encrypted));
    }
    let symlink = filepath_in_source_dir(
        &settings.dot_prefix, target_dir_abs_path, source_dir_abs_path, target_abs_path,
        Some(&settings.symlink_postfix),
    );
    if symlink.exists() {
        return Some((SourceVariant::Symlink, symlink));
    }
    None
}

// Shared --dry-run / --force helpers

/// Resolve the effective dry-run value: `true` if either the per-command flag
/// *or* the global `--dry-run` flag is set.
#[inline]
pub(crate) fn resolve_dry_run(cmd_dry_run: bool, args_dry_run: bool) -> bool {
    cmd_dry_run || args_dry_run
}

/// If `force` is `false`, return `Err(DfmError::Other(msg))`.
/// Useful for the common post-loop "force required" check.
///
/// When `force` is `true` the caller still needs to handle the case
/// (e.g. skip the conflict, or proceed despite errors); this helper
/// only covers the "reject without force" half.
/// Drop lines from an ignore file whose trimmed content makes `should_ignore`
/// return true. Blank lines and the full original text of kept lines are
/// preserved. A missing file is a no-op. When `dry_run` is true the file is not
/// written; the set of would-be-removed lines is still returned.
pub(crate) fn prune_ignore_file(
    ignore_file_path: &Path,
    should_ignore: impl Fn(&str) -> bool,
    dry_run: bool,
) -> Result<Vec<String>, DfmError> {
    if !ignore_file_path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(ignore_file_path).map_err(|e| io_err(ignore_file_path, e))?;
    let mut removed = Vec::new();
    let mut kept = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || !should_ignore(trimmed) {
            kept.push(line.to_string());
        } else {
            removed.push(trimmed.to_string());
        }
    }
    if !removed.is_empty() && !dry_run {
        atomic_write(ignore_file_path, kept.join("\n"))?;
    }
    Ok(removed)
}

/// After a successful add/pull, drop the ignore patterns that were matched by
/// the affected files, so those files no longer remain ignored. No-op on
/// dry-run or when nothing matched.
pub(crate) fn prune_matched_ignore_patterns(
    xdg: &Xdg,
    patterns_to_remove: &[String],
    dry_run: bool,
) -> Result<(), DfmError> {
    if dry_run || patterns_to_remove.is_empty() {
        return Ok(());
    }
    let removed = prune_ignore_file(
        &calc_local_ignore_file(xdg)?,
        |t| patterns_to_remove.iter().any(|p| *p == t),
        dry_run,
    )?;
    info!("removed {} pattern(s) from ignore file", removed.len());
    Ok(())
}

#[inline]
pub(crate) fn require_force(force: bool, msg: impl std::fmt::Display) -> Result<(), DfmError> {
    if force {
        Ok(())
    } else {
        Err(DfmError::Other(msg.to_string()))
    }
}

/// Outcome of an ignore-regex check on a target path.
#[derive(Eq, PartialEq, Clone, Copy)]
pub(crate) enum IgnoreHandling {
    /// The path is not ignored — process it.
    NotIgnored,
    /// The path is ignored and `--force` is set: the matched pattern was
    /// queued for removal; continue processing the path.
    Override,
    /// The path is ignored without `--force`: skip it (return early).
    Skip,
}

/// Handle the "target is ignored" case shared by `add` and `pull`. When the
/// target matches the ignore regex, `--force` queues the matched pattern for
/// removal (the caller proceeds); otherwise the caller must skip the target.
///
/// Returns `None` when the target is not ignored, or `Some(())` when it is
/// ignored and the caller should return early (no `--force`). When `--force`
/// is set the pattern is pushed to `patterns_to_remove` and `Some(())` is not
/// returned — the caller continues.
pub(crate) fn handle_ignore_or_override(
    target_ignore_regex: &RegexSet,
    rel_path: &Path,
    force: bool,
    patterns_to_remove: &mut Vec<String>,
    ignored_log_target: &Path,
    ignore_file_path: &Path,
) -> IgnoreHandling {
    let Some(pattern) = check_path_matches_regex_component_wise(target_ignore_regex, rel_path) else {
        return IgnoreHandling::NotIgnored;
    };
    if force {
        info!("target {:?} is ignored, --force overrides, will remove /{}/ from ignore file", ignored_log_target, pattern);
        patterns_to_remove.push(pattern);
        IgnoreHandling::Override
    } else {
        info!("target {:?} is ignored by regex /{}/ in file {:?}", ignored_log_target, pattern, ignore_file_path);
        IgnoreHandling::Skip
    }
}

/// True when `rel_path` matches the target ignore regex. Unlike
/// `handle_ignore_or_override`, this only answers "is it ignored?" and never
/// queues ignore-pattern removal, so the caller cannot override the ignore and
/// the ignore file is never edited. `sync` uses this because it must never
/// process or remove ignored files.
pub(crate) fn is_ignored(target_ignore_regex: &RegexSet, rel_path: &Path) -> bool {
    check_path_matches_regex_component_wise(target_ignore_regex, rel_path).is_some()
}

/// Shared copy + permissions + mtime + state update logic used by both
/// `add` (target → source) and `pull` (source → target).
///
/// `from` is the source of the copy (its permissions are preserved).
/// `to`   is the destination.
/// `source_file_in_source_dir` — the file residing in the source directory,
/// used to compute the state key.
pub(crate) fn sync_file_copy(
    from: &Path,
    to: &Path,
    source_file_in_source_dir: &Path,
    state: &mut StateObject,
    source_dir_abs_path: &Path,
) -> Result<(), DfmError> {
    let to_parent = to
        .parent()
        .ok_or_else(|| DfmError::Other(format!("cannot resolve parent directory of {:?}", to)))?;
    fs::create_dir_all(to_parent).map_err(|e| io_err(to_parent, e))?;
    fs::copy(from, to).map_err(|e| io_copy_err(from, to, e))?;

    let permissions = from.metadata().map_err(|e| io_err(from, e))?.permissions();
    trace!("copy permissions {:o}", permissions.mode());
    if let Err(e) = fs::set_permissions(to, permissions.clone()) {
        error!("failed to set permissions {:?} to {:?}: {}", permissions.mode(), to, e);
    }

    // `source_file_in_source_dir` is always the source-dir file.  The
    // "other" file (whichever of `from`/`to` is not the source-dir file)
    // must be passed as `target_abs` so `update_sync_state` sets its mtime.
    // In pull: from == source_file_in_source_dir, to = the target file.
    // In add:  to   == source_file_in_source_dir, from = the target file.
    let other = if source_file_in_source_dir == from { to } else { from };
    update_sync_state(state, source_file_in_source_dir, other, source_dir_abs_path)?;

    if log_enabled!(log::Level::Trace) {
        let from_meta = from.metadata().map_err(|e| io_err(from, e))?;
        let to_meta = to.metadata().map_err(|e| io_err(to, e))?;
        let to_modified = to_meta.modified().map_err(|e| io_err(to, e))?;
        let from_modified = from_meta.modified().map_err(|e| io_err(from, e))?;
        trace!("final state:\n from: mtime={:?}\n to: mtime={:?}",
             to_modified, from_modified);
    }

    Ok(())
}

/// Resolve a tool command template (merge/diff) from settings, or error when
/// it is missing or empty. `config_key` names the setting so the error tells
/// the user exactly what to set.
pub(crate) fn resolve_tool_command(
    command_opt: &Option<String>,
    tool_name: &str,
    config_key: &str,
) -> Result<String, DfmError> {
    if let Some(cmd) = command_opt
        && !cmd.is_empty()
    {
        return Ok(cmd.clone());
    }
    Err(DfmError::Other(format!(
        "no {} tool configured — set {} in config",
        tool_name, config_key
    )))
}

/// RAII guard that removes a directory on drop, so temp dirs like
/// `.current_merge/` / `.current_diff/` are cleaned up on every exit path
/// (including `?` returns).
pub(crate) struct DirGuard(PathBuf);

impl Drop for DirGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Create a private (0700) recursive temp directory for the merge/diff tools,
/// so decrypted content written there is not readable by other users while a
/// tool runs.
pub(crate) fn create_private_temp_dir(path: &Path) -> Result<(), DfmError> {
    use std::os::unix::fs::DirBuilderExt;
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)
        .map_err(|e| io_err(path, e))
}

/// Run the merge tool inside `.current_merge/` in the source directory.
///
/// Creates temporary copies named `target.<file>` (working dir side),
/// `source.<file>` (cellar side, decrypted if encrypted) and an empty
/// `result.<file>` for the merge tool's output.  The merge tool must
/// write the merged result into `{result}` — after it succeeds the
/// result file is copied back to both the target and the source, and
/// the sync state is updated.
pub(crate) fn run_merge(
    settings: &Settings,
    source_abs_path: &Path,
    target_abs_path: &Path,
    state: &mut StateObject,
    source_dir_abs_path: &Path,
) -> Result<(), DfmError> {
    // Resolve from the already-resolved absolute source path so the merge
    // directory lands in the real source directory regardless of how the
    // command was invoked.
    let merge_dir = source_dir_abs_path.join(".current_merge");
    // A stale `.current_merge` (e.g. left behind by a killed merge tool)
    // must not interfere: drop it before creating the fresh one.
    if merge_dir.exists() {
        debug!("removing stale merge directory {:?}", merge_dir);
        fs::remove_dir_all(&merge_dir).map_err(|e| io_err(&merge_dir, e))?;
    }
    let _guard = DirGuard(merge_dir.clone());
    create_private_temp_dir(&merge_dir)?;

    let file_name = target_abs_path
        .file_name()
        .ok_or_else(|| DfmError::Other("target path has no file name".into()))?
        .to_str()
        .ok_or_else(|| DfmError::Other("target file name is not valid UTF-8".into()))?;

    let source_is_encrypted = source_abs_path
        .to_str()
        .map(|s| s.ends_with(&settings.encrypted_postfix))
        .unwrap_or(false);

    // Copy both sides into the merge directory:
    //   target.<file> = working-dir side (always plain text)
    //   source.<file> = cellar side (decrypted if encrypted)
    //   result.<file> = merge tool writes output here
    let target_path = merge_dir.join(format!("target.{}", file_name));
    let source_path = merge_dir.join(format!("source.{}", file_name));
    let result_path = merge_dir.join(format!("result.{}", file_name));
    fs::copy(target_abs_path, &target_path).map_err(|e| io_copy_err(target_abs_path, &target_path, e))?;
    if source_is_encrypted {
        dfm::crypt::read_encrypted_file(settings, source_abs_path, &source_path)?;
    } else {
        fs::copy(source_abs_path, &source_path).map_err(|e| io_copy_err(source_abs_path, &source_path, e))?;
    }
    let command = resolve_tool_command(&settings.merge_tool_command, "merge", "merge_tool_command")?;

    // Parse command template: first token is the program, rest are arguments
    // with {target}, {source} and {result} replaced by actual temp file paths.
    let (prog, args) = split_command(&command);
    let prog = prog.ok_or_else(|| DfmError::Other("merge command is empty".into()))?;
    let target_str = target_path.to_string_lossy();
    let source_str = source_path.to_string_lossy();
    let result_str = result_path.to_string_lossy();
    let args: Vec<String> = args.iter().map(|a| {

        a.replace("{target}", target_str.as_ref())
         .replace("{source}", source_str.as_ref())
         .replace("{result}", result_str.as_ref())
    }).collect();

    info!("running merge tool: {} {:?}", prog, args);

    let mut child = match std::process::Command::new(prog).args(&args).spawn() {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(DfmError::NotFound(format!("merge tool {} not found", prog)));
        }
        Err(e) => return Err(DfmError::Io(e)),
    };
    let status = child.wait().map_err(DfmError::Io)?;

    if !status.success() || !result_path.exists() {
        let reason = if !status.success() {
            format!("merge tool exited with status {}", status)
        } else {
            format!("merge tool exited successfully but did not create {:?}", result_path)
        };
        return Err(DfmError::Other(reason));
    }

    // Copy the merged result to BOTH the source and the target
    if source_is_encrypted {
        dfm::crypt::write_encrypted_file(settings, &result_path, source_abs_path)?;
    } else {
        fs::copy(&result_path, source_abs_path).map_err(|e| io_copy_err(&result_path, source_abs_path, e))?;
    }
    fs::copy(&result_path, target_abs_path).map_err(|e| io_copy_err(&result_path, target_abs_path, e))?;

    // Update sync state and mtimes
    update_sync_state(state, source_abs_path, target_abs_path, source_dir_abs_path)?;

    info!("merge completed for {:?}", target_abs_path);
    Ok(())
}

// Pager

/// Write a string to stdout, tolerating a broken pipe: when the reader closes
/// the stream on purpose (e.g. `dfm status | head -1`) the write-failed error
/// from the closed pipe is not a failure of this command.
pub(crate) fn write_stdout(s: &str) -> Result<(), DfmError> {
    let mut out = std::io::stdout();
    match out.write_all(s.as_bytes()) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
        Err(e) => Err(DfmError::Io(e)),
    }
}

/// Write prebuilt output to a pager when stdout is an interactive terminal and
/// the output is taller than the terminal; otherwise write it directly. Shared
/// by `status` and `diff --all` so both page through the same mechanism.
pub(crate) fn print_paged(output: &str) -> Result<(), DfmError> {
    if !std::io::stdout().is_terminal() {
        return write_stdout(output);
    }
    let line_count = output.lines().count();
    let term_height = terminal_height().unwrap_or(24);

    if line_count > term_height {
        let pager_cmd = env::var("PAGER").unwrap_or_else(|_| "less -FRSX".to_string());
        let (prog, args) = split_command(&pager_cmd);
        let Some(prog) = prog else {
            return write_stdout(output);
        };
        let mut child = std::process::Command::new(prog)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()?;
        if let Some(mut stdin) = child.stdin.take() {
            if let Err(e) = stdin.write_all(output.as_bytes())
                && e.kind() != std::io::ErrorKind::BrokenPipe
            {
                return Err(DfmError::Io(e));
            }
            if let Err(e) = stdin.flush()
                && e.kind() != std::io::ErrorKind::BrokenPipe
            {
                return Err(DfmError::Io(e));
            }
        }
        child.wait()?;
        Ok(())
    } else {
        write_stdout(output)
    }
}

fn terminal_height() -> Option<usize> {
    if let Ok(output) = std::process::Command::new("stty")
        .arg("size")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        && let Ok(s) = String::from_utf8(output.stdout)
        && let Some(rows_str) = s.split_whitespace().next()
        && let Ok(rows) = rows_str.parse::<usize>()
    {
        return Some(rows);
    }
    None
}
