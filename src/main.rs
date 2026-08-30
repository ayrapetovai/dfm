mod commands;

use clap::Parser;
use log::warn;
use std::io;
use std::path::PathBuf;

use dfm::*;

use commands::*;
use dfm::cli::{Args, Command};

fn main_logic() -> Result<(), dfm::DfmError> {
    check_root_privileges()?;

    let args = Args::parse();
    let xdg = microxdg::Xdg::new()?;

    if let Err(e) = stderrlog::new()
        .module(module_path!())
        .verbosity(args.verbosity)
        .show_level(args.verbosity > 2)
        .init()
    {
        return Err(dfm::DfmError::other(e));
    }

    let path_to_state_file = match calc_state_file_path(&xdg) {
        Ok(p) => Some(p),
        Err(e) => {
            warn!(
                "state file path could not be resolved: {}; continuing without state",
                e
            );
            None
        }
    };
    let (state_opt, state_read_error) = match &path_to_state_file {
        Some(p) => match read_state(p) {
            Ok(state) => (Some(state), None),
            Err(DfmError::NotFound(_)) => (None, None),
            Err(e) => (None, Some(e)),
        },
        None => (None, None),
    };

    let default_settings = create_default_settings();
    let path_to_config_file = match &args.config {
        Some(path) => Some(path.clone()),
        None => match calc_config_file_path(&xdg) {
            Ok(p) => Some(p),
            Err(e) => {
                warn!(
                    "config file path could not be resolved: {}; continuing without config",
                    e
                );
                None
            }
        },
    };
    let config_from_file = match &path_to_config_file {
        Some(p) => match read_config(p) {
            Ok(config) => Some(config),
            // A missing config is normal on the first run — defaults apply.
            Err(DfmError::NotFound(_)) => None,
            Err(e) => {
                // A corrupt/unreadable config must not silently change
                // behavior: fall back to defaults but tell the user.
                warn!(
                    "config file {:?} could not be read ({}); continuing with default settings",
                    p, e
                );
                None
            }
        },
        None => None,
    };
    let settings = merge_settings(&default_settings, &config_from_file, state_opt.as_ref());

    match args.command {
        Command::Init {
            path_to_source,
            path_to_target,
            dry_run,
        } => init_command(
            &settings,
            &xdg,
            InitArgs {
                path_to_source,
                path_to_target,
                dry_run: resolve_dry_run(dry_run, args.dry_run),
            },
        ),
        Command::Config { get, set, list } => match &path_to_config_file {
            Some(p) => config_command(
                ConfigArgs {
                    get,
                    set,
                    list,
                    dry_run: args.dry_run,
                },
                p,
            ),
            None => Err(dfm::DfmError::NotFound(
                "config file path could not be resolved".into(),
            )),
        },
        Command::Purge {
            dry_run,
            keep_source,
            keep_config_file,
            force,
        } => purge_command(
            &settings,
            &xdg,
            PurgeArgs {
                dry_run: resolve_dry_run(dry_run, args.dry_run),
                keep_source,
                keep_config_file,
                force,
            },
            &path_to_config_file,
        ),
        Command::Add {
            paths,
            force,
            symlink,
            encrypt,
            dry_run,
        } => with_state(
            state_opt,
            state_read_error.as_ref(),
            path_to_state_file.as_ref(),
            |state| {
                add_command(
                    &settings,
                    &xdg,
                    AddArgs {
                        paths,
                        force,
                        symlink,
                        encrypt,
                        dry_run: resolve_dry_run(dry_run, args.dry_run),
                    },
                    state,
                )
            },
        ),
        Command::Sync {
            paths,
            force,
            dry_run,
        } => with_state(
            state_opt,
            state_read_error.as_ref(),
            path_to_state_file.as_ref(),
            |state| {
                sync_command(
                    &settings,
                    &xdg,
                    SyncArgs {
                        paths,
                        force,
                        dry_run: resolve_dry_run(dry_run, args.dry_run),
                    },
                    state,
                )
            },
        ),
        Command::Pull {
            paths,
            force,
            symlink,
            dry_run,
        } => with_state(
            state_opt,
            state_read_error.as_ref(),
            path_to_state_file.as_ref(),
            |state| {
                pull_command(
                    &settings,
                    &xdg,
                    PullArgs {
                        paths,
                        force,
                        symlink,
                        dry_run: resolve_dry_run(dry_run, args.dry_run),
                    },
                    state,
                )
            },
        ),
        Command::Forget {
            paths,
            force,
            dry_run,
        } => with_state_even_if_error(
            state_opt,
            state_read_error.as_ref(),
            path_to_state_file.as_ref(),
            |state| {
                forget_command(
                    &settings,
                    &xdg,
                    ForgetArgs {
                        paths,
                        force,
                        dry_run: resolve_dry_run(dry_run, args.dry_run),
                    },
                    state,
                )
            },
        ),
        Command::Ignore {
            paths,
            patterns,
            remove,
            dry_run,
        } => ignore_command(
            &settings,
            &xdg,
            IgnoreArgs {
                paths,
                patterns,
                remove,
                dry_run: resolve_dry_run(dry_run, args.dry_run),
            },
        ),
        Command::Paths => paths_command(
            &settings,
            &xdg,
            PathsArgs {},
            &path_to_config_file,
            &path_to_state_file,
        ),
        Command::Encrypt { path, output } => {
            encrypt_command(&settings, EncryptArgs { path, output })
        }
        Command::Decrypt { path, output } => {
            decrypt_command(&settings, DecryptArgs { path, output })
        }
        Command::Merge { paths, dry_run } => with_state(
            state_opt,
            state_read_error.as_ref(),
            path_to_state_file.as_ref(),
            |state| {
                merge_command(
                    &settings,
                    &xdg,
                    MergeArgs {
                        paths,
                        dry_run: resolve_dry_run(dry_run, args.dry_run),
                    },
                    state,
                )
            },
        ),
        Command::Diff { paths, all } => {
            if paths.is_none() {
                // With no paths, diff all modified files (the `--all` default).
                // Unlike other commands this still needs the state file to know
                // which files are managed, so it requires initialization.
                let state = state_opt
                    .ok_or_else(|| state_unavailable_error(state_read_error.as_ref()))?;
                diff_command(&settings, &xdg, DiffArgs { paths: None, all }, &state)
            } else {
                let state = state_opt
                    .ok_or_else(|| state_unavailable_error(state_read_error.as_ref()))?;
                diff_command(&settings, &xdg, DiffArgs { paths, all }, &state)
            }
        }
        Command::Status {
            all,
            short,
            porcelain,
            conflicted,
            modified,
            unmanaged,
            managed,
            unpulled,
            encrypted,
            ignored,
            ignored_patterns,
            unused_patterns,
            paths,
        } => {
            let state =
                state_opt.ok_or_else(|| state_unavailable_error(state_read_error.as_ref()))?;
            status_command(
                &settings,
                &xdg,
                StatusArgs {
                    all,
                    short,
                    porcelain,
                    conflicted,
                    modified,
                    unmanaged,
                    managed,
                    unpulled,
                    encrypted,
                    ignored,
                    ignored_patterns,
                    unused_patterns,
                    paths,
                },
                &state,
            )
        }
    }
}

/// Unwrap the state file, run a mutating command against it, then persist the
/// result back to disk. Converts a missing state file or state file path into
/// an error instead of a panic and removes the repetitive `state_opt.unwrap()` /
/// `path_to_state_file.unwrap()` noise.
fn with_state<T>(
    state_opt: Option<StateObject>,
    state_read_error: Option<&dfm::DfmError>,
    path_to_state_file: Option<&PathBuf>,
    f: impl FnOnce(&mut StateObject) -> Result<T, dfm::DfmError>,
) -> Result<T, dfm::DfmError> {
    let mut state = state_opt.ok_or_else(|| state_unavailable_error(state_read_error))?;
    let state_path = path_to_state_file.ok_or_else(|| {
        dfm::DfmError::InvalidInput("state file path could not be resolved".into())
    })?;
    let result = f(&mut state)?;
    write_state(state_path, &state)?;
    Ok(result)
}

/// Like `with_state`, but always persists the state file even when the
/// command returns an error. Used by `forget`: the in-memory state entry is
/// cleaned up before the source deletion runs, so on a filesystem error the
/// entry must still be written back (and the deletion error reported).
fn with_state_even_if_error<T>(
    state_opt: Option<StateObject>,
    state_read_error: Option<&dfm::DfmError>,
    path_to_state_file: Option<&PathBuf>,
    f: impl FnOnce(&mut StateObject) -> Result<T, dfm::DfmError>,
) -> Result<T, dfm::DfmError> {
    let mut state = state_opt.ok_or_else(|| state_unavailable_error(state_read_error))?;
    let state_path = path_to_state_file.ok_or_else(|| {
        dfm::DfmError::InvalidInput("state file path could not be resolved".into())
    })?;
    let result = f(&mut state);
    write_state(state_path, &state)?;
    result
}

/// Build the error reported when the state file is required but unavailable:
/// a corrupt state file surfaces its parse/validation error, while a missing
/// one reports the usual "not found".
fn state_unavailable_error(state_read_error: Option<&dfm::DfmError>) -> dfm::DfmError {
    match state_read_error {
        Some(dfm::DfmError::InvalidData(msg)) => dfm::DfmError::InvalidData(msg.clone()),
        Some(dfm::DfmError::Io(e)) => dfm::DfmError::Io(io::Error::new(e.kind(), e.to_string())),
        Some(e) => dfm::DfmError::InvalidData(e.to_string()),
        None => dfm::DfmError::NotFound("state file is not found".into()),
    }
}

fn main() {
    if let Err(e) = main_logic() {
        eprintln!("{}", e);
        std::process::exit(1);
    }
}
