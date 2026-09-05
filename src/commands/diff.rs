use std::fs;
use std::io::IsTerminal;
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
    print_paged, write_stdout, update_sync_state, msg_dry_run,
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

/// What a user-provided path resolves to: a diffable pair of regular files, a
/// symlink that only has pointees to report, or an explanation of why there is
/// nothing to diff. `diff` prints the non-pair cases and succeeds; `diff
/// --editable` turns them into errors.
enum Resolution {
    Pair { target_abs: PathBuf, source_abs: PathBuf },
    TargetSymlink { target_abs: PathBuf, label: String },
    SourcePointer { target_label: String, source_abs: PathBuf },
    Unavailable(String),
}

/// Resolve a single user-provided path (target-side or source-side) to both
/// sides of the diff.
fn resolve_pair(
    settings: &Settings,
    target_dir_abs_path: &Path,
    source_dir_abs_path: &Path,
    target_ignore_regex: &regex::RegexSet,
    user_path: &Path,
) -> Result<Resolution, DfmError> {
    let user_path_str = user_path.to_string_lossy();

    let path_abs = cli_path_to_abs(user_path)?;

    if path_abs.starts_with(source_dir_abs_path) {
        // Provided path is in the source directory — infer the target.
        let source_abs = path_abs;
        let source_rel = state_key_for(&source_abs, source_dir_abs_path);
        let (target_rel, target_abs) =
            source_rel_to_target_abs(&source_rel, target_dir_abs_path, settings);

        if !path_exists(&source_abs)? {
            return Ok(Resolution::Unavailable(if path_exists(&target_abs)? {
                format!("{} is not managed", user_path_str)
            } else {
                format!("{} does not exist", user_path_str)
            }));
        }
        if let Some(pattern) = check_path_matches_regex_component_wise(
            target_ignore_regex, &PathBuf::from(&target_rel),
        ) {
            return Ok(Resolution::Unavailable(format!("{} is ignored by {}", user_path_str, pattern)));
        }
        if !path_exists(&target_abs)? {
            return Ok(Resolution::Unavailable(format!("{} is not pulled", target_abs.display())));
        }
        if target_abs.is_symlink() {
            let label = target_abs.to_string_lossy().into_owned();
            return Ok(Resolution::TargetSymlink { target_abs, label });
        }
        return Ok(Resolution::Pair { target_abs, source_abs });
    }

    // Provided path is a target path — find the source.
    let target_abs = path_abs;
    if !target_abs.starts_with(target_dir_abs_path) {
        return Ok(Resolution::Unavailable(format!("{} is not managed", user_path_str)));
    }
    let target_rel = file_path_relative_to(&target_abs, target_dir_abs_path);
    if let Some(pattern) = check_path_matches_regex_component_wise(
        target_ignore_regex, &target_rel,
    ) {
        return Ok(Resolution::Unavailable(format!("{} is ignored by {}", user_path_str, pattern)));
    }
    if !path_exists(&target_abs)? {
        let has_source = resolve_source_variant(
            settings, target_dir_abs_path, source_dir_abs_path, &target_abs,
        ).is_some();
        return Ok(Resolution::Unavailable(if has_source {
            format!("{} is not pulled", user_path_str)
        } else {
            format!("{} does not exist", user_path_str)
        }));
    }
    if target_abs.is_symlink() {
        let label = user_path_str.into_owned();
        return Ok(Resolution::TargetSymlink { target_abs, label });
    }

    let Some((variant, source_abs)) = resolve_source_variant(
        settings, target_dir_abs_path, source_dir_abs_path, &target_abs,
    ) else {
        return Ok(Resolution::Unavailable(format!("{} is not managed", user_path_str)));
    };
    if variant == SourceVariant::Symlink {
        // The source is a symlink pointer but the target is a regular file —
        // there is nothing to diff.
        return Ok(Resolution::SourcePointer {
            target_label: user_path_str.into_owned(),
            source_abs,
        });
    }
    Ok(Resolution::Pair { target_abs, source_abs })
}

/// Report the diff of a single user-provided path: run the diff tool for a
/// real pair, and print the explanation for everything else.
fn diff_one_path(
    settings: &Settings,
    state: &StateObject,
    target_dir_abs_path: &Path,
    source_dir_abs_path: &Path,
    target_ignore_regex: &regex::RegexSet,
    user_path: &Path,
) -> Result<(), DfmError> {
    match resolve_pair(
        settings, target_dir_abs_path, source_dir_abs_path, target_ignore_regex, user_path,
    )? {
        Resolution::Pair { target_abs, source_abs } => diff_regular(
            settings, state, source_dir_abs_path, &target_abs, &source_abs,
            &user_path.to_string_lossy(),
        ),
        Resolution::TargetSymlink { target_abs, label } => print_symlink_pointees(
            settings, target_dir_abs_path, source_dir_abs_path, &target_abs, &label,
        ),
        Resolution::SourcePointer { target_label, source_abs } => {
            let pointer = read_symlink_pointer(&source_abs)?;
            println!("{} has source symlink pointer {} pointing to {}", target_label, source_abs.display(), pointer);
            Ok(())
        }
        Resolution::Unavailable(message) => {
            println!("{}", message);
            Ok(())
        }
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
    // The scratch dir is only needed when the source is encrypted (to hold the
    // decrypted `{source}`); plain diffs must not write into the source dir.
    let mut scratch = Scratch::new(source_dir_abs_path);
    let _guard = DirGuard(scratch.dir.clone());

    let source_arg = match &decrypted_source {
        Some(bytes) => write_scratch_source(&mut scratch, target_abs, bytes)?,
        None => source_abs.to_path_buf(),
    };

    let (prog, args) = build_diff_program(&settings.diff_tool_command, "diff_tool_command", target_abs, &source_arg)?;

    info!("running diff tool: {} {:?}", prog, args);

    let mut child = match Command::new(&prog).args(&args).spawn() {
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

    // Scratch dir for decrypted encrypted sources, removed on every exit path.
    // The directory is only created when an encrypted source is actually
    // diffed, so `diff --all` also works on read-only source dirs (the plain
    // `{source}` is passed straight through).
    let mut scratch = Scratch::new(&source_dir_abs);
    let _guard = DirGuard(scratch.dir.clone());

    // Stream each diff to stdout when it is not a terminal so bulk output
    // (e.g. redirected to a file) is never buffered in memory; on a terminal,
    // collect into one buffer so the whole report pages through one pager.
    let mut sink = if std::io::stdout().is_terminal() {
        DiffSink::Buffer(String::new())
    } else {
        DiffSink::Stdout
    };

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

        let source_abs = remove_dots_from_path(&source_dir_abs.join(source_rel));
        let target_abs = remove_dots_from_path(&target_dir_abs.join(&target_rel));

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
                let source_arg = prepare_source(settings, &mut scratch, &target_abs, &source_abs, is_encrypted)?;
                run_diff_capture(
                    &settings.diff_all_tool_command_target,
                    "diff_all_tool_command_target",
                    &target_abs,
                    &source_arg,
                    &mut sink,
                )?;
            }
            CompareByTimestamp::SourceModified => {
                let source_arg = prepare_source(settings, &mut scratch, &target_abs, &source_abs, is_encrypted)?;
                run_diff_capture(
                    &settings.diff_all_tool_command_source,
                    "diff_all_tool_command_source",
                    &target_abs,
                    &source_arg,
                    &mut sink,
                )?;
            }
            // Up-to-date and never-synchronized files produce nothing.
            CompareByTimestamp::NonModified | CompareByTimestamp::NeverSynchronized => {}
        }
    }

    sink.finish()
}

/// The `.current_diff` scratch directory inside the source directory, created
/// on first use: a diff that needs no scratch (every source plain) leaves the
/// source directory untouched and works when it is read-only.
struct Scratch {
    dir: PathBuf,
    ready: bool,
}

impl Scratch {
    fn new(source_dir_abs: &Path) -> Self {
        Scratch { dir: source_dir_abs.join(".current_diff"), ready: false }
    }

    /// The scratch directory, created with mode 0700 on the first call. A
    /// stale directory (left behind by a killed tool) is dropped first.
    fn dir(&mut self) -> Result<&Path, DfmError> {
        if !self.ready {
            if self.dir.exists() {
                fs::remove_dir_all(&self.dir).map_err(|e| io_err(&self.dir, e))?;
            }
            create_private_temp_dir(&self.dir)?;
            self.ready = true;
        }
        Ok(&self.dir)
    }

    /// Path of a scratch file named after the target file, e.g.
    /// `source.bashrc` for `role` "source".
    fn file_for(&mut self, role: &str, target_abs: &Path) -> Result<PathBuf, DfmError> {
        let file_name = target_abs
            .file_name()
            .ok_or_else(|| DfmError::Other("target path has no file name".into()))?
            .to_string_lossy()
            .into_owned();
        Ok(self.dir()?.join(format!("{}.{}", role, file_name)))
    }
}

/// Write decrypted source bytes into the scratch directory and return the path
/// the diff tool must read them from.
fn write_scratch_source(
    scratch: &mut Scratch,
    target_abs: &Path,
    plaintext: &[u8],
) -> Result<PathBuf, DfmError> {
    let decrypted_path = scratch.file_for("source", target_abs)?;
    fs::write(&decrypted_path, plaintext).map_err(|e| io_err(&decrypted_path, e))?;
    Ok(decrypted_path)
}

/// Decrypt an encrypted source into memory, announcing which file needs the
/// password first.
fn decrypt_source(
    settings: &Settings,
    target_abs: &Path,
    source_abs: &Path,
) -> Result<Vec<u8>, DfmError> {
    let inner_name = file_path_relative_to(target_abs, Path::new(&settings.target_dir));
    dfm::crypt::announce_encryption_password(&inner_name.to_string_lossy());
    let (decrypted, _mode) = dfm::crypt::read_encrypted_bytes(settings, source_abs)?;
    Ok(decrypted)
}

/// Resolve the `{source}` argument: the plain source path, or for an encrypted
/// source a scratch copy of its decrypted plaintext (requires the password).
fn prepare_source(
    settings: &Settings,
    scratch: &mut Scratch,
    target_abs: &Path,
    source_abs: &Path,
    is_encrypted: bool,
) -> Result<PathBuf, DfmError> {
    if !is_encrypted {
        return Ok(source_abs.to_path_buf());
    }
    let decrypted = decrypt_source(settings, target_abs, source_abs)?;
    write_scratch_source(scratch, target_abs, &decrypted)
}

/// Run one non-interactive diff for `--all`, forwarding its captured stdout to
/// the sink (buffered for paging, or streamed to stdout for non-terminal use).
fn run_diff_capture(
    template: &Option<String>,
    config_key: &str,
    target_abs: &Path,
    source_arg: &Path,
    sink: &mut DiffSink,
) -> Result<(), DfmError> {
    let (prog, args) = build_diff_program(template, config_key, target_abs, source_arg)?;

    info!("running diff: {} {:?}", prog, args);

    let child = match Command::new(&prog)
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
        sink.write(&chunk)?;
    }
    Ok(())
}

/// Where `diff --all` sends each captured diff: buffered for the single pager
/// on a terminal, or streamed straight to stdout otherwise.
enum DiffSink {
    Buffer(String),
    Stdout,
}

impl DiffSink {
    fn write(&mut self, chunk: &str) -> Result<(), DfmError> {
        match self {
            DiffSink::Buffer(buf) => {
                buf.push_str(chunk);
                Ok(())
            }
            DiffSink::Stdout => write_stdout(chunk),
        }
    }

    fn finish(self) -> Result<(), DfmError> {
        match self {
            DiffSink::Buffer(buf) => print_paged(&buf),
            DiffSink::Stdout => Ok(()),
        }
    }
}

/// Typed, per-command arguments for `diff --editable` (built by the dispatcher).
pub struct DiffEditableArgs {
    pub paths: Vec<PathBuf>,
    pub dry_run: bool,
}

/// Edit managed files through the diff tool and write the result back to both
/// sides (`diff -e`). This is the only diff mode that modifies files, so it is
/// dispatched through `with_state`.
///
/// The tool is handed a private, writable copy of each side; when it exits 0,
/// every copy that changed is written back to its own file — the target copy to
/// the target, the source copy to the source (re-encrypted when the source is
/// encrypted) — and the sync state is updated. The two sides are written
/// independently, so the files may end up differing; that is the point of
/// editing files that were diverged to begin with. A non-zero tool exit, or no
/// change at all, discards everything.
pub fn diff_editable_command(
    settings: &Settings,
    xdg: &Xdg,
    args: DiffEditableArgs,
    state: &mut StateObject,
) -> Result<(), DfmError> {
    let DiffEditableArgs { paths, dry_run } = args;
    debug!("editable diff paths {:?}", paths);

    if dry_run {
        info!("{}", msg_dry_run());
    }

    let (target_dir_abs_path, source_dir_abs_path) = calc_working_dir_paths(settings)?;
    let target_ignore_regex = load_ignore_regex(&calc_local_ignore_file(xdg)?)?;

    // The guard is armed before any work so the copies are removed on every
    // exit path, including the "contents differ" error.
    let mut scratch = Scratch::new(&source_dir_abs_path);
    let _guard = DirGuard(scratch.dir.clone());

    for path in &paths {
        let (target_abs, source_abs) = match resolve_pair(
            settings, &target_dir_abs_path, &source_dir_abs_path, &target_ignore_regex, path,
        )? {
            Resolution::Pair { target_abs, source_abs } => (target_abs, source_abs),
            Resolution::TargetSymlink { label, .. } => {
                return Err(DfmError::InvalidInput(format!("{} is a symlink and cannot be edited", label)));
            }
            Resolution::SourcePointer { target_label, .. } => {
                return Err(DfmError::InvalidInput(format!(
                    "{} is managed as a symlink and cannot be edited", target_label
                )));
            }
            Resolution::Unavailable(message) => return Err(DfmError::InvalidInput(message)),
        };
        edit_one_pair(
            settings, state, &source_dir_abs_path, &mut scratch, &target_abs, &source_abs, dry_run,
        )?;
    }
    Ok(())
}

/// Run the editable diff for one resolved pair. Each side is edited through its
/// own writable copy, and every side whose copy changed is written back to its
/// file — the two sides may legitimately end up differing.
fn edit_one_pair(
    settings: &Settings,
    state: &mut StateObject,
    source_dir_abs_path: &Path,
    scratch: &mut Scratch,
    target_abs: &Path,
    source_abs: &Path,
    dry_run: bool,
) -> Result<(), DfmError> {
    let source_is_encrypted = source_abs
        .to_string_lossy()
        .ends_with(&settings.encrypted_postfix);

    let target_copy = scratch.file_for("target", target_abs)?;
    let source_copy = scratch.file_for("source", target_abs)?;
    fs::copy(target_abs, &target_copy).map_err(|e| io_copy_err(target_abs, &target_copy, e))?;
    if source_is_encrypted {
        let decrypted = decrypt_source(settings, target_abs, source_abs)?;
        fs::write(&source_copy, decrypted).map_err(|e| io_err(&source_copy, e))?;
    } else {
        fs::copy(source_abs, &source_copy).map_err(|e| io_copy_err(source_abs, &source_copy, e))?;
    }
    // A copy inherits the mode of the file it came from — a read-only dotfile
    // would leave the tool unable to save the edit.
    make_owner_writable(&target_copy)?;
    make_owner_writable(&source_copy)?;

    if dry_run {
        info!("would edit {:?} with the diff tool (dry run)", target_abs);
        return Ok(());
    }

    // Each copy knows its own original fingerprint: the sides may already
    // differ, so a side counts as edited only relative to its own content.
    let target_original = compute_sha256(&target_copy)?;
    let source_original = compute_sha256(&source_copy)?;

    run_editable_tool(settings, target_abs, &target_copy, &source_copy)?;

    let target_edited = compute_sha256(&target_copy)? != target_original;
    let source_edited = compute_sha256(&source_copy)? != source_original;
    if !target_edited && !source_edited {
        info!("{:?} was not changed by the diff tool", target_abs);
        return Ok(());
    }

    write_back_edits(
        settings, target_abs, source_abs, &target_copy, &source_copy,
        target_edited, source_edited,
    )?;

    // Synchronization is only honest when the saved files hold equal content:
    // a per-side edit that left the pair differing must not be recorded as
    // synchronized.
    if compute_sha256(&target_copy)? == compute_sha256(&source_copy)? {
        update_sync_state(state, source_abs, target_abs, source_dir_abs_path)?;
        info!("edited {:?} and {:?}", target_abs, source_abs);
    } else {
        info!(
            "edited {:?} and {:?} to different content, not synchronized",
            target_abs, source_abs
        );
    }
    Ok(())
}

/// Spawn the editable diff tool on the two scratch copies. A non-zero exit is
/// the user rejecting the edit — the caller discards the copies.
fn run_editable_tool(
    settings: &Settings,
    target_abs: &Path,
    target_copy: &Path,
    source_copy: &Path,
) -> Result<(), DfmError> {
    let (prog, args) = build_diff_program(
        &settings.diff_editable_tool_command, "diff_editable_tool_command", target_copy, source_copy,
    )?;

    info!("running editable diff tool: {} {:?}", prog, args);

    let mut child = match Command::new(&prog).args(&args).spawn() {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(DfmError::NotFound(format!("diff tool {} not found", prog)));
        }
        Err(e) => return Err(DfmError::Io(e)),
    };

    let status = child.wait().map_err(DfmError::Io)?;
    if !status.success() {
        return Err(DfmError::Other(format!(
            "diff tool exited with status {}, {:?} was not changed", status, target_abs
        )));
    }
    Ok(())
}

/// Write each edited scratch copy back to its own file, keeping the permissions
/// each file already had. The two sides are written independently — the target
/// and the source may end up differing, which is the point of editing files
/// that were diverged to begin with.
fn write_back_edits(
    settings: &Settings,
    target_abs: &Path,
    source_abs: &Path,
    target_copy: &Path,
    source_copy: &Path,
    target_edited: bool,
    source_edited: bool,
) -> Result<(), DfmError> {
    if target_edited {
        copy_keeping_permissions(target_copy, target_abs)?;
    }
    if source_edited {
        let source_is_encrypted = source_abs
            .to_string_lossy()
            .ends_with(&settings.encrypted_postfix);
        if source_is_encrypted {
            // Encrypt the edited source copy so its plaintext is stored; the
            // target file still provides the real target-relative inner name,
            // file mode and enclosing directory modes for the blob.
            dfm::crypt::write_encrypted_source(settings, source_copy, target_abs, source_abs)?;
        } else {
            copy_keeping_permissions(source_copy, source_abs)?;
        }
    }
    Ok(())
}

/// Overwrite an existing file with `from`, restoring the destination's own
/// permissions afterwards (`fs::copy` would stamp the scratch copy's mode on
/// it).
fn copy_keeping_permissions(from: &Path, to: &Path) -> Result<(), DfmError> {
    let permissions = fs::metadata(to).map_err(|e| io_err(to, e))?.permissions();
    fs::copy(from, to).map_err(|e| io_copy_err(from, to, e))?;
    fs::set_permissions(to, permissions).map_err(|e| io_err(to, e))
}

/// Grant the owner read+write on a scratch copy.
fn make_owner_writable(path: &Path) -> Result<(), DfmError> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = fs::metadata(path).map_err(|e| io_err(path, e))?.permissions();
    permissions.set_mode(permissions.mode() | 0o600);
    fs::set_permissions(path, permissions).map_err(|e| io_err(path, e))
}

/// Split a diff template and substitute the `{target}`/`{source}` placeholders.
/// The program and args are returned separately so a missing tool can be
/// reported precisely; shared by the per-path and `--all` diff modes.
fn build_diff_program(
    template: &Option<String>,
    config_key: &str,
    target_abs: &Path,
    source_arg: &Path,
) -> Result<(String, Vec<String>), DfmError> {
    let command = resolve_tool_command(template, "diff", config_key)?;
    let (prog, args) = split_command(&command);
    let prog = prog.ok_or_else(|| DfmError::Other("diff command is empty".into()))?;
    let target_str = target_abs.to_string_lossy();
    let source_str = source_arg.to_string_lossy();
    let args: Vec<String> = args.iter().map(|a| {
        a.replace("{target}", target_str.as_ref())
         .replace("{source}", source_str.as_ref())
    }).collect();
    Ok((prog.to_string(), args))
}
