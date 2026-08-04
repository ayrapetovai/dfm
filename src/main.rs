mod commands;

use clap::{Parser, Subcommand};
use log::warn;
use std::path::PathBuf;

use dfm::*;

use commands::*;

static LONG_ABOUT: &str = r#"This program is designed to manage dotfiles which are usually
configuration files in user's home directory."#;

#[derive(Parser, Debug)]
#[command(version, about = "Dotfile Manager", long_about = LONG_ABOUT)]
struct Args {
    #[command(subcommand)]
    command: Command,

    /// Do not perform actions, only checks and reports.
    #[arg(long, short = 'n')]
    dry_run: bool,

    /// Verbosity level: 0 - quiet, 1 - brief, 2 - info, 3 - debug.
    #[arg(
        long,
        short = 'v',
        num_args = 1,
        default_value_t = 1,
        value_name = "LEVEL_NUMBER"
    )]
    verbosity: usize, // 0 - don't output anything, 1 - brief info, 2 - info print action, 3 - print debug

    /// Use other config.
    #[arg(long, short = 'c', num_args = 1, required = false, value_name = "PATH")]
    config: Option<PathBuf>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Initialize state file and config file with the source directory.
    Init {
        /// Specifies the path to the source directory.
        #[arg(required = true, value_name = "SOURCE")]
        path_to_source: PathBuf,

        /// Specifies the path to the target directory. [default: $HOME]
        #[arg(required = false, value_name = "TARGET")]
        path_to_target: Option<PathBuf>,

        /// Run only checks, no changes will be made to filesystem.
        #[arg(long, short = 'n')]
        dry_run: bool,
    },

    /// Remove state of the program and the source directory.
    Purge {
        /// Run only checks, no changes will be made to filesystem.
        #[arg(long, short = 'n')]
        dry_run: bool,

        /// Do not remove source directory.
        #[arg(long, short = 's')]
        keep_source: bool,

        /// Do not remove config file.
        #[arg(long, short = 'c')]
        keep_config_file: bool,

        /// Remove the source directory even if it contains changes.
        #[arg(long, short = 'f')]
        force: bool,
    },

    /// Add file under management, or copy changes to the source directory.
    Add {
        /// Files to be copied to the source directory from target.
        /// If omitted - add all files in the target directory.
        #[arg(value_name = "PATH")]
        paths: Option<Vec<PathBuf>>,

        /// Overwrite source file on conflict and add symlinks.
        #[arg(long, short = 'f')]
        force: bool,

        /// Move file to the source directory, create a symlink on place of it.
        #[arg(long, short = 's')]
        symlink: bool,

        /// Copy encrypted form of file to the source directory.
        /// Replace existing unencrypted source file if any exists.
        #[arg(long, short = 'e')]
        encrypt: bool,

        /// Run only checks, no changes will be made to filesystem.
        #[arg(long, short = 'n')]
        dry_run: bool,
    },

    /// Copy changes from the source directory to the target directory.
    Pull {
        /// Files to be updated from source directory to target.
        /// If omitted - pull all files in the source directory.
        #[arg(value_name = "PATH")]
        paths: Option<Vec<PathBuf>>,

        /// Overwrite target file on conflict.
        #[arg(long, short)]
        force: bool,

        /// Create a symlink instead of file.
        #[arg(long, short = 's')]
        symlink: bool,

        /// Run only checks, no changes will be made to filesystem.
        #[arg(long, short = 'n')]
        dry_run: bool,
    },

    /// Show status of managed files.
    Status {
        /// Full report: include up-to-date and ignored entries.
        #[arg(long, short = 'a')]
        all: bool,

        /// One line per file with two-letter status code.
        #[arg(long, short = 's')]
        short: bool,

        /// Stable machine-readable output (tab-separated, never paged).
        #[arg(long)]
        porcelain: bool,

        /// Only conflicted (BothModified) entries.
        #[arg(long, short = 'c')]
        conflicted: bool,

        /// Only modified entries (target or source).
        #[arg(long, short = 'm')]
        modified: bool,

        /// Only unmanaged entries.
        #[arg(long, short = 'U')]
        unmanaged: bool,

        /// Only managed entries (inverse of --unmanaged).
        #[arg(long, short = 'M')]
        managed: bool,

        /// Only unpulled entries (source-only).
        #[arg(long, short = 'p')]
        unpulled: bool,

        /// Only ignored entries.
        #[arg(long, short = 'i')]
        ignored: bool,

        /// List active ignore patterns.
        #[arg(long, short = 'l')]
        ignored_patterns: bool,

        /// List unused (stale) ignore patterns.
        #[arg(long, short = 'u')]
        unused_patterns: bool,
    },

    /// Perform 3-way merge on conflicting files.
    Merge {
        /// Files to merge, if omitted - all conflicting files.
        #[arg(value_name = "PATH")]
        paths: Option<Vec<PathBuf>>,

        /// Run only checks, no changes will be made to filesystem.
        #[arg(long, short = 'n')]
        dry_run: bool,
    },

    // must check conflicts
    /// Remove file from management (does not delete target file).
    Forget {
        paths: Option<Vec<PathBuf>>,

        /// Delete source file on conflict.
        #[arg(long, short)]
        force: bool,

        /// Run only checks, no changes will be made to filesystem.
        #[arg(long, short = 'n')]
        dry_run: bool,
    },

    /// Ignore a file when processing other subcommands (does not delete target of source file).
    /// `paths`/`patterns` add records while `remove` deletes them, so the three
    /// are mutually exclusive: mixing them disambiguates no meaningful operation
    /// and `ignore` previously ran only the `remove` branch, silently dropping
    /// the rest. An ArgGroup error is clearer than silent partial behavior.
    #[command(group(
        clap::ArgGroup::new("ignore_input")
            .required(true)
            .multiple(false)
            .args(["paths", "patterns", "remove"])
    ))]
    Ignore {
        /// Ignore files.
        #[arg(num_args = 1.., value_name = "PATH")]
        paths: Option<Vec<PathBuf>>,

        /// Add an ignore regular expression
        #[arg(long, short = 'p', num_args = 1.., value_name = "REGEXP")]
        patterns: Option<Vec<String>>,

        /// Remove records from the target ignore file.
        #[arg(long, short = 'r', num_args = 1.., value_name = "RECORD")]
        remove: Option<Vec<String>>,

        /// Run only checks, no changes will be made to filesystem.
        #[arg(long, short = 'n')]
        dry_run: bool,
    },

    /// Get or set config properties.
    Config {
        /// Print the specified config property.
        #[arg(long, short, num_args = 1, required = false, required_unless_present_any = ["set", "list"], value_name = "NAME")]
        get: Option<String>,

        /// Set config property to a specified value.
        #[arg(long, short, num_args = 2, required = false, required_unless_present_any = ["get", "list"], value_names = ["NAME", "VALUE"])]
        set: Option<Vec<String>>,

        /// List all config properties.
        #[arg(long, short, required = false, required_unless_present_any = ["get", "set"])]
        list: bool,
    },

    /// Print paths
    Paths,
}

fn main_logic() -> Result<(), dfm::DfmError> {
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
    let state_opt = match &path_to_state_file {
        Some(p) => read_state(p).ok(),
        None => None,
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
        Some(p) => read_config(p).ok(),
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
        } => with_state(state_opt, path_to_state_file.as_ref(), |state| {
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
        }),
        Command::Pull {
            paths,
            force,
            symlink,
            dry_run,
        } => with_state(state_opt, path_to_state_file.as_ref(), |state| {
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
        }),
        Command::Forget {
            paths,
            force,
            dry_run,
        } => with_state_even_if_error(state_opt, path_to_state_file.as_ref(), |state| {
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
        }),
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
        Command::Merge { paths, dry_run } => {
            with_state(state_opt, path_to_state_file.as_ref(), |state| {
                merge_command(
                    &settings,
                    &xdg,
                    MergeArgs {
                        paths,
                        dry_run: resolve_dry_run(dry_run, args.dry_run),
                    },
                    state,
                )
            })
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
            ignored,
            ignored_patterns,
            unused_patterns,
        } => {
            let state = state_opt
                .ok_or_else(|| dfm::DfmError::NotFound("state file is not found".into()))?;
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
                    ignored,
                    ignored_patterns,
                    unused_patterns,
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
    path_to_state_file: Option<&PathBuf>,
    f: impl FnOnce(&mut StateObject) -> Result<T, dfm::DfmError>,
) -> Result<T, dfm::DfmError> {
    let mut state =
        state_opt.ok_or_else(|| dfm::DfmError::NotFound("state file is not found".into()))?;
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
    path_to_state_file: Option<&PathBuf>,
    f: impl FnOnce(&mut StateObject) -> Result<T, dfm::DfmError>,
) -> Result<T, dfm::DfmError> {
    let mut state =
        state_opt.ok_or_else(|| dfm::DfmError::NotFound("state file is not found".into()))?;
    let state_path = path_to_state_file.ok_or_else(|| {
        dfm::DfmError::InvalidInput("state file path could not be resolved".into())
    })?;
    let result = f(&mut state);
    write_state(state_path, &state)?;
    result
}

fn main() {
    if let Err(e) = main_logic() {
        eprintln!("{}", e);
        std::process::exit(1);
    }
}
