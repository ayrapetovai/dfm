use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use log::{debug, info};

use dfm::*;
use crate::DfmError;
use microxdg::Xdg;
use super::{
    resolve_tool_command, split_command, DirGuard, create_private_temp_dir,
    state_key_for, source_rel_to_target_abs, resolve_source_variant,
    read_symlink_pointer, get_sync_time, SourceVariant, cli_path_to_abs,
    print_paged,
};

/// Typed, per-command arguments for `diff` (built by the dispatcher).
pub struct DiffArgs {
    pub paths: Option<Vec<PathBuf>>,
    pub all: bool,
}

/// Show a diff of the given paths using the configured diff tool. The command
/// never modifies any file: the sync state is only read, and an encrypted
/// source is decrypted into a transient directory that is removed afterwards.
pub fn diff_command(
    settings: &Settings,
    xdg: &Xdg,
    args: DiffArgs,
    state: &StateObject,
) -> Result<(), DfmError> {
    let DiffArgs { paths, all: _all } = args;
    // With no explicit paths, batch-diff every modified file (`--all`, the
    // default). Explicit paths keep the per-path interactive behavior below.
    if paths.is_none() {
        return diff_all(settings, xdg, state);
    }
    let Some(paths) = paths else {
        return Ok(());
    };
    debug!("diff paths {:?}", paths);

    let (target_dir_abs_path, source_dir_abs_path) = calc_working_dir_paths(settings)?;

    let target_ignore_file_path = calc_local_ignore_file(xdg)?;
    let target_ignore_regex = load_ignore_regex(&target_ignore_file_path)?;

    for path in &paths {
        match diff_one_path(
            settings, state, &target_dir_abs_path, &source_dir_abs_path,
            &target_ignore_regex, path,
        ) {
            Ok(()) => {}
            Err(e) if e.is_permission_denied() => {
                warn_unreadable(path, &e);
            }
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

/// Resolve a single user-provided path (target-side or source-side) and run
/// the diff for it.
fn diff_one_path(
    settings: &Settings,
    state: &StateObject,
    target_dir_abs_path: &Path,
    source_dir_abs_path: &Path,
    target_ignore_regex: &regex::RegexSet,
    user_path: &Path,
) -> Result<(), DfmError> {
    let user_path_str = user_path.to_string_lossy();

    let path_abs = cli_path_to_abs(user_path)?;

    if path_abs.starts_with(source_dir_abs_path) {
        // Provided path is in the source directory — infer the target.
        let source_abs = path_abs;
        let source_rel = state_key_for(&source_abs, source_dir_abs_path);
        let (target_rel, target_abs) =
            source_rel_to_target_abs(&source_rel, target_dir_abs_path, settings);

        if !path_exists(&source_abs)? {
            if !path_exists(&target_abs)? {
                println!("{} does not exist", user_path_str);
            } else {
                println!("{} is not managed", user_path_str);
            }
            return Ok(());
        }
        if let Some(pattern) = check_path_matches_regex_component_wise(
            target_ignore_regex, &PathBuf::from(&target_rel),
        ) {
            println!("{} is ignored by {}", user_path_str, pattern);
            return Ok(());
        }
        if !path_exists(&target_abs)? {
            println!("{} is not pulled", target_abs.display());
            return Ok(());
        }
        if target_abs.is_symlink() {
            print_symlink_pointees(
                settings, target_dir_abs_path, source_dir_abs_path,
                &target_abs, &target_abs.to_string_lossy(),
            )?;
            return Ok(());
        }
        diff_regular(settings, state, source_dir_abs_path, &target_abs, &source_abs, &user_path_str)
    } else {
        // Provided path is a target path — find the source.
        let target_abs = path_abs;
        if !target_abs.starts_with(target_dir_abs_path) {
            println!("{} is not managed", user_path_str);
            return Ok(());
        }
        let target_rel = file_path_relative_to(&target_abs, target_dir_abs_path);
        if let Some(pattern) = check_path_matches_regex_component_wise(
            target_ignore_regex, &target_rel,
        ) {
            println!("{} is ignored by {}", user_path_str, pattern);
            return Ok(());
        }
        if !path_exists(&target_abs)? {
            if resolve_source_variant(
                settings, target_dir_abs_path, source_dir_abs_path, &target_abs,
            ).is_some() {
                println!("{} is not pulled", user_path_str);
            } else {
                println!("{} does not exist", user_path_str);
            }
            return Ok(());
        }
        if target_abs.is_symlink() {
            print_symlink_pointees(
                settings, target_dir_abs_path, source_dir_abs_path,
                &target_abs, &user_path_str,
            )?;
            return Ok(());
        }

        let Some((variant, source_abs)) = resolve_source_variant(
            settings, target_dir_abs_path, source_dir_abs_path, &target_abs,
        ) else {
            println!("{} is not managed", user_path_str);
            return Ok(());
        };
        if variant == SourceVariant::Symlink {
            // The source is a symlink pointer but the target is a regular
            // file — there is nothing to diff.
            let pointer = read_symlink_pointer(&source_abs)?;
            println!("{} has source symlink pointer {} pointing to {}", user_path_str, source_abs.display(), pointer);
            return Ok(());
        }
        diff_regular(settings, state, source_dir_abs_path, &target_abs, &source_abs, &user_path_str)
    }
}

/// Whether a path is present. Unlike `Path::exists()` — which swallows
/// permission errors and reports an unreadable file as absent — a
/// permission-denied path is reported as an error (skipped with a warning by
/// the caller), not misclassified as "unmanaged".
fn path_exists(path: &Path) -> Result<bool, DfmError> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(io_err(path, e)),
    }
}

/// Print the target symlink's pointee and the source's pointee, and stop.
fn print_symlink_pointees(
    settings: &Settings,
    target_dir_abs_path: &Path,
    source_dir_abs_path: &Path,
    target_abs: &Path,
    symlink_label: &str,
) -> Result<(), DfmError> {
    let target_pointee = fs::read_link(target_abs).map_err(|e| io_err(target_abs, e))?;
    println!("{} is a symlink pointing to {}", symlink_label, target_pointee.display());

    match resolve_source_variant(settings, target_dir_abs_path, source_dir_abs_path, target_abs) {
        Some((SourceVariant::Symlink, source_abs)) => {
            let pointer = read_symlink_pointer(&source_abs)?;
            println!("{} points to {}", source_abs.display(), pointer);
        }
        Some((_, source_abs)) => {
            println!("source is {}", source_abs.display());
        }
        None => {
            println!("no source found");
        }
    }
    Ok(())
}

/// Compare target and source (mtime/content, like `add`), print the
/// "synchronized" verdict when the contents match, or run the diff tool when
/// they differ.
fn diff_regular(
    settings: &Settings,
    state: &StateObject,
    source_dir_abs_path: &Path,
    target_abs: &Path,
    source_abs: &Path,
    user_path_str: &str,
) -> Result<(), DfmError> {
    let sync_time_opt = get_sync_time(state, source_abs, source_dir_abs_path);
    let cmp = compare_files(
        &settings.encrypted_postfix, target_abs, source_abs, sync_time_opt,
    )?;
    if cmp == CompareByTimestamp::NonModified {
        println!("{} is synchronized", user_path_str);
        return Ok(());
    }

    let source_is_encrypted = source_abs
        .to_string_lossy()
        .ends_with(&settings.encrypted_postfix);
    if source_is_encrypted {
        let inner_name = file_path_relative_to(target_abs, Path::new(&settings.target_dir));
        dfm::crypt::announce_encryption_password(&inner_name.to_string_lossy());
        let (decrypted, _mode) = dfm::crypt::read_encrypted_bytes(settings, source_abs)?;
        let target_bytes = fs::read(target_abs).map_err(|e| io_err(target_abs, e))?;
        if decrypted == target_bytes {
            println!("{} is synchronized", user_path_str);
            return Ok(());
        }
        run_diff(settings, source_dir_abs_path, target_abs, source_abs, Some(decrypted))
    } else if compute_sha256(target_abs)? != compute_sha256(source_abs)? {
        run_diff(settings, source_dir_abs_path, target_abs, source_abs, None)
    } else {
        println!("{} is synchronized", user_path_str);
        Ok(())
    }
}

/// Run the configured diff tool against the target and the source, exactly as
/// the merge tool is run: the command template is split into program + args
/// (no shell) with `{target}`/`{source}` replaced by real paths. A missing
/// diff tool is an error (exit code 1). When the source is encrypted, its
/// decrypted plaintext is written to a transient file that is substituted for
/// `{source}` — it is offered as a file, not via stdin, so an interactive tool
/// like `vimdiff` does not read stdin into an extra buffer that would then
/// block `:qa` (E37/E162).
fn run_diff(
    settings: &Settings,
    source_dir_abs_path: &Path,
    target_abs: &Path,
    source_abs: &Path,
    decrypted_source: Option<Vec<u8>>,
) -> Result<(), DfmError> {
    // The temp dir is only needed when the source is encrypted (to hold the
    // decrypted `{source}`); plain diffs must not write into the source dir.
    let diff_dir = source_dir_abs_path.join(".current_diff");
    let _guard = DirGuard(diff_dir.clone());

    let source_arg = if let Some(bytes) = &decrypted_source {
        if diff_dir.exists() {
            fs::remove_dir_all(&diff_dir).map_err(|e| io_err(&diff_dir, e))?;
        }
        create_private_temp_dir(&diff_dir)?;
        let file_name = target_abs
            .file_name()
            .ok_or_else(|| DfmError::Other("target path has no file name".into()))?
            .to_string_lossy();
        let decrypted_path = diff_dir.join(format!("source.{}", file_name));
        fs::write(&decrypted_path, bytes).map_err(|e| io_err(&decrypted_path, e))?;
        decrypted_path
    } else {
        source_abs.to_path_buf()
    };

    let command = resolve_tool_command(&settings.diff_tool_command, "diff", "diff_tool_command")?;
    let (prog, args) = split_command(&command);
    let prog = prog.ok_or_else(|| DfmError::Other("diff command is empty".into()))?;
    let target_str = target_abs.to_string_lossy();
    let source_str = source_arg.to_string_lossy();
    let args: Vec<String> = args.iter().map(|a| {
        a.replace("{target}", target_str.as_ref())
         .replace("{source}", source_str.as_ref())
    }).collect();

    info!("running diff tool: {} {:?}", prog, args);

    let mut child = match Command::new(prog).args(&args).spawn() {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(DfmError::NotFound(format!("diff tool {} not found", prog)));
        }
        Err(e) => return Err(DfmError::Io(e)),
    };

    let status = child.wait().map_err(DfmError::Io)?;
    if !status.success() {
        debug!("diff tool exited with status {}", status);
    }
    Ok(())
}

/// Batch-diff every modified managed file (`diff --all`, the default when no
/// paths are given). For each modified entry the matching non-interactive diff
/// template runs with its stdout captured, all files' diffs are concatenated,
/// and the whole report goes through the same pager `status` uses.
fn diff_all(
    settings: &Settings,
    xdg: &Xdg,
    state: &StateObject,
) -> Result<(), DfmError> {
    let (target_dir_abs, source_dir_abs) = calc_working_dir_paths(settings)?;

    let target_ignore_regex = load_ignore_regex(&calc_local_ignore_file(xdg)?)?;
    let source_ignore_regex = load_ignore_regex(&calc_source_ignore_file(&source_dir_abs))?;

    // Scratch dir for decrypted encrypted sources; removed on every exit path.
    let diff_dir = source_dir_abs.join(".current_diff");
    let _guard = DirGuard(diff_dir.clone());
    if diff_dir.exists() {
        fs::remove_dir_all(&diff_dir).map_err(|e| io_err(&diff_dir, e))?;
    }
    create_private_temp_dir(&diff_dir)?;

    let mut output = String::new();
    for (source_rel, sync_time) in &state.syncs {
        // Managed symlinks are never "modified" — skip them entirely.
        if source_rel.ends_with(&settings.symlink_postfix) {
            continue;
        }
        let is_encrypted = source_rel.ends_with(&settings.encrypted_postfix);

        let target_rel = source_rel_to_target_rel(
            source_rel,
            &settings.dot_prefix,
            &settings.symlink_postfix,
            &settings.encrypted_postfix,
        );

        // Ignored on either side is not offered for diffing.
        if check_path_matches_regex_component_wise(&target_ignore_regex, &PathBuf::from(&target_rel)).is_some() {
            continue;
        }
        if check_path_matches_regex_component_wise(&source_ignore_regex, &PathBuf::from(source_rel)).is_some() {
            continue;
        }

        let target_abs = target_dir_abs.join(&target_rel);
        let source_abs = source_dir_abs.join(source_rel);

        // Both sides must be present for a diff; anything else (unpulled,
        // stale) produces nothing.
        if !target_abs.exists() || !source_abs.exists() {
            continue;
        }

        match compare_files(
            &settings.encrypted_postfix,
            &target_abs,
            &source_abs,
            Some(sync_time),
        )? {
            CompareByTimestamp::TargetModified | CompareByTimestamp::BothModified => {
                let source_arg = prepare_source(settings, &diff_dir, &target_abs, &source_abs, is_encrypted)?;
                run_diff_capture(
                    &settings.diff_all_tool_command_target,
                    "diff_all_tool_command_target",
                    &target_abs,
                    &source_arg,
                    &mut output,
                )?;
            }
            CompareByTimestamp::SourceModified => {
                let source_arg = prepare_source(settings, &diff_dir, &target_abs, &source_abs, is_encrypted)?;
                run_diff_capture(
                    &settings.diff_all_tool_command_source,
                    "diff_all_tool_command_source",
                    &target_abs,
                    &source_arg,
                    &mut output,
                )?;
            }
            // Up-to-date and never-synchronized files produce nothing.
            CompareByTimestamp::NonModified | CompareByTimestamp::NeverSynchronized => {}
        }
    }

    print_paged(&output)
}

/// Resolve the `{source}` argument: the plain source path, or for an encrypted
/// source a scratch copy of its decrypted plaintext (requires the password).
fn prepare_source(
    settings: &Settings,
    diff_dir: &Path,
    target_abs: &Path,
    source_abs: &Path,
    is_encrypted: bool,
) -> Result<PathBuf, DfmError> {
    if !is_encrypted {
        return Ok(source_abs.to_path_buf());
    }
    let inner_name = file_path_relative_to(target_abs, Path::new(&settings.target_dir));
    dfm::crypt::announce_encryption_password(&inner_name.to_string_lossy());
    let (decrypted, _mode) = dfm::crypt::read_encrypted_bytes(settings, source_abs)?;
    let file_name = target_abs
        .file_name()
        .ok_or_else(|| DfmError::Other("target path has no file name".into()))?
        .to_string_lossy();
    let decrypted_path = diff_dir.join(format!("source.{}", file_name));
    fs::write(&decrypted_path, decrypted).map_err(|e| io_err(&decrypted_path, e))?;
    Ok(decrypted_path)
}

/// Run one non-interactive diff for `--all`, capturing stdout into the shared
/// output buffer. The template is split into program + args (no shell) and
/// `{target}`/`{source}` are substituted, like the merge tool.
fn run_diff_capture(
    template: &Option<String>,
    config_key: &str,
    target_abs: &Path,
    source_arg: &Path,
    output: &mut String,
) -> Result<(), DfmError> {
    let command = resolve_tool_command(template, "diff", config_key)?;
    let (prog, args) = split_command(&command);
    let prog = prog.ok_or_else(|| DfmError::Other("diff command is empty".into()))?;
    let target_str = target_abs.to_string_lossy();
    let source_str = source_arg.to_string_lossy();
    let args: Vec<String> = args.iter().map(|a| {
        a.replace("{target}", target_str.as_ref())
         .replace("{source}", source_str.as_ref())
    }).collect();

    info!("running diff: {} {:?}", prog, args);

    let child = match Command::new(prog)
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
    {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(DfmError::NotFound(format!("diff tool {} not found", prog)));
        }
        Err(e) => return Err(DfmError::Io(e)),
    };

    let outcome = child.wait_with_output().map_err(DfmError::Io)?;
    if let Ok(chunk) = String::from_utf8(outcome.stdout) {
        output.push_str(&chunk);
    }
    Ok(())
}
