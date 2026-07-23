mod commands;

use std::path::PathBuf;
use clap::{Parser, Subcommand};

use dfm::*;

use commands::*;

// opts https://docs.rs/clap/latest/clap/_derive/_cookbook/git_derive/index.html
// toml https://docs.rs/toml/latest/toml/
// env https://docs.rs/envmnt/latest/envmnt/
// xdg https://wiki.archlinux.org/title/XDG_Base_Directory
// aes https://rust.howtos.io/a-guide-to-symmetric-encryption-in-rust/

static LONG_ABOUT: &'static str = 
r#"This program is designed to manage dotfiles which are usually
configuration files in user's home directory."#;

#[derive(Parser, Debug)]
#[command(version, about = "Dotfile Manager", long_about = LONG_ABOUT)]
struct Args {

    #[command(subcommand)]
    command: Command,

    //arbitrary_command: String,

    /// Do not perform actions, only checks and reports.
    #[arg(long, short = 'n', num_args = 0, default_value_t = false)]
    dry_run: bool,

    /// Verbosity level: 0 - quite, 1 - brief, 2 - info, 3 - debug.
    #[arg(long, short = 'v', num_args = 1, default_value_t = 1, value_name = "LEVEL_NUMBER")]
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
        #[arg(long, short = 'n', num_args = 0, default_value_t = false)]
        dry_run: bool,
    },

    /// Remove state of the program and the source directory.
    #[command(arg_required_else_help = false)]
    Purge {
        /// Run only checks, no changes will be made to filesystem.
        #[arg(long, short = 'n', num_args = 0, default_value_t = false)]
        dry_run: bool,

        /// Do not remove source directory.
        #[arg(long, short = 's', num_args = 0, default_value_t = false)]
        keep_source: bool,

        /// Do not remove config file.
        #[arg(long, short = 'c', num_args = 0, default_value_t = false)]
        keep_config_file: bool,

        /// Remove the source directory even if it contains changes.
        #[arg(long, short = 'f', num_args = 0, default_value_t = false)]
        force: bool,
    },

    // TODO rename to `push`?
    /// Add file under management, or copy changes to the source directory.
    #[command(arg_required_else_help = false)]
    Add {
        /// Files to be copied to the source directory from target.
        /// If omitted - add all files in the target directory.
        #[arg(value_name = "PATH")]
        paths: Option<Vec<PathBuf>>,

        /// Run merge tool on conflicts.
        #[arg(long, short, num_args = 0, default_value_t = false)]
        merge: bool,

        /// Force managing files outside target directory.
        #[arg(long, short, num_args = 0, default_value_t = false)]
        allow_foreign: bool,

        /// Overwrite source file on conflict and add symlinks.
        #[arg(long, short = 'f', num_args = 0, default_value_t = false)]
        force: bool,

        /// Move file to the source directory, create a symlink on place of it.
        #[arg(long, short = 's', num_args = 0, default_value_t = false)]
        symlink: bool,

        /// Copy encrypted form of file to the source directory.
        /// Replace existing unencrypted source file if any exists.
        #[arg(long, short = 'e', num_args = 0, default_value_t = false)]
        encrypt: bool,

        /// Run only checks, no changes will be made to filesystem.
        #[arg(long, short = 'n', num_args = 0, default_value_t = false)]
        dry_run: bool,
    },

    /// Copy changes from the source directory to the target directory.
    #[command(arg_required_else_help = false)]
    Pull {
        /// Files to be updated from source directory to target.
        /// If omitted - pull all files in the source directory.
        #[arg(value_name = "PATH")]
        paths: Option<Vec<PathBuf>>,

        /// Run merge tool on conflicts.
        #[arg(long, short, num_args = 0, default_value_t = false)]
        merge: bool,

        /// Overwrite target file on conflict.
        #[arg(long, short, num_args = 0, default_value_t = false)]
        force: bool,

        /// Create a symlink instead of file.
        #[arg(long, short = 's', num_args = 0, default_value_t = false)]
        symlink: bool,

        /// Run only checks, no changes will be made to filesystem.
        #[arg(long, short = 'n', num_args = 0, default_value_t = false)]
        dry_run: bool,
    },

    /// Show status of managed files. [default: show difference]
    Status {
        /// Difference between target files and source files [default]
        #[arg(long, short = 'd', num_args = 0, default_value_t = true)]
        difference: bool,

        /// Full report: differences, management, ignoring.
        #[arg(long, short = 'a', num_args = 0, default_value_t = false)]
        all: bool,

        /// List managed files
        #[arg(long, short = 'm', num_args = 0, default_value_t = false)]
        managed: bool,

        /// List unmanaged files.
        #[arg(long, short = 'M', num_args = 0, default_value_t = false)]
        unmanaged: bool,

        /// Source files that was pulled.
        #[arg(long, short = 'p', num_args = 0, default_value_t = false)]
        pulled: bool,

        /// Source files that was not pulled.
        #[arg(long, short = 'P', num_args = 0, default_value_t = false)]
        never_pulled: bool,

        /// List ignored files.
        #[arg(long, short = 'i', num_args = 0, default_value_t = false)]
        ignored: bool,

        /// List pattern used to ignore files.
        #[arg(long, short = 'l', num_args = 0, default_value_t = false)]
        ignored_patterns: bool,

        /// List unused ignore patterns.
        #[arg(long, short = 'u', num_args = 0, default_value_t = false)]
        useless_patterns: bool,
    },

    // TODO implement like https://man7.org/linux/man-pages/man1/git-mergetool.1.html
    /// Perform 2-way merge on conflicting files.
    Merge {
        /// Files to merge, if omitted - all conflicting files.
        #[arg(value_name = "PATH")]
        paths: Option<Vec<PathBuf>>,

        /// Use specified merge command
        #[arg(long, short = 't', num_args = 1, value_name = "COMMAND")]
        tool: Option<String>,
    },

    // must check conflicts
    /// Remove file from management (does not delete the file).
    Forget {
        paths: Option<Vec<PathBuf>>,

        /// Delete source file on conflict.
        #[arg(long, short, num_args = 0, default_value_t = false)]
        force: bool,

        /// Run only checks, no changes will be made to filesystem.
        #[arg(long, short = 'n', num_args = 0, default_value_t = false)]
        dry_run: bool,
    },

    /// Ignore a file when processing other subcommands.
    #[command(arg_required_else_help = true)]
    Ignore {
        #[arg(num_args = 0.., value_name = "PATH")]
        paths: Option<Vec<PathBuf>>,

        #[arg(long, short = 'p', num_args = 0.., value_name = "REGEXP")]
        patterns: Option<Vec<String>>,

        /// Run only checks, no changes will be made to filesystem.
        #[arg(long, short = 'n', num_args = 0, default_value_t = false)]
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
        #[arg(long, short, num_args = 0, required = false, required_unless_present_any = ["get", "set"])]
        list: bool,
    },

    /// Print paths
    #[command(arg_required_else_help = false)]
    Paths
}

fn main() -> Result<(), dfm::DfmError> {
    let args = Args::parse();

    if let Err(e) = stderrlog::new()
        .module(module_path!())
        .verbosity(args.verbosity)
        .show_level(args.verbosity > 2)
        .init() {
        return Err(dfm::DfmError::other(e));
    }

    let path_to_state_file = calc_state_file_path()?;
    let state_opt = match read_state(&path_to_state_file) {
        Ok(s) => Some(s),
        Err(_) => None
    };

    let default_settings = create_default_settings();
    let path_to_config_file = calc_config_file_path()?;
    let config_from_file = match read_config(&path_to_config_file) {
        Ok(c) => Some(c),
        Err(_) => None
    };
    let settings =  merge_settings(&default_settings, &config_from_file, state_opt.as_ref());

    return match args.command {
        Command::Init { .. } => {
            init_command(&settings, &args)
        },
        Command::Config { .. } => {
            config_command(&args, &path_to_config_file)
        },
        Command::Purge { .. } => {
            purge_command(&settings, &args, &path_to_config_file)
        },
        Command::Add { .. } => {
            if state_opt.is_none() {
                return Err(dfm::DfmError::NotFound(format!("state file is not found {:?}", path_to_state_file)));
            }
            let mut state = state_opt.unwrap();
            add_command(&settings, &args, &mut state)?;
            write_state(&path_to_state_file, &state)
        },
        Command::Pull { .. } => {
            if state_opt.is_none() {
                return Err(dfm::DfmError::NotFound(format!("state file is not found {:?}", path_to_state_file)));
            }
            let mut state = state_opt.unwrap();
            pull_command(&settings, &args, &mut state)?;
            write_state(&path_to_state_file, &state)
        },
        Command::Forget { .. } => {
            if state_opt.is_none() {
                return Err(dfm::DfmError::NotFound(format!("state file is not found {:?}", path_to_state_file)));
            }
            let mut state = state_opt.unwrap();
            forget_command(&settings, &args, &mut state)?;
            write_state(&path_to_state_file, &state)
        },
        Command::Ignore { .. } => {
            ignore_command(&settings, &args)
        },
        Command::Paths => {
            paths_command(&settings, &path_to_config_file, &path_to_state_file)
        },
        _ => {
            Err(dfm::DfmError::Unsupported(format!("subcommand {:?} is not implemented yet", args)))
        }
    };
}
