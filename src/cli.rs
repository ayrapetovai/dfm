use std::path::PathBuf;

use clap::{Parser, Subcommand};

// TODO this text must descrybe the behevior of the application in details.
static LONG_ABOUT: &str = r#"This program is designed to manage dotfiles which are usually
configuration files in user's home directory."#;

#[derive(Parser, Debug)]
#[command(version, about = "Dotfile Manager", long_about = LONG_ABOUT)]
pub struct Args {
    #[command(subcommand)]
    pub command: Command,

    /// Do not perform actions, only checks and reports.
    #[arg(long, short = 'n')]
    pub dry_run: bool,

    /// Verbosity level: 0 - quiet, 1 - brief, 2 - info, 3 - debug.
    #[arg(
        long,
        short = 'v',
        num_args = 1,
        default_value_t = 1,
        value_name = "LEVEL_NUMBER"
    )]
    pub verbosity: usize, // 0 - don't output anything, 1 - brief info, 2 - info print action, 3 - print debug

    /// Use other config.
    #[arg(long, short = 'c', num_args = 1, required = false, value_name = "PATH")]
    pub config: Option<PathBuf>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
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

        /// Only show status for the given paths.
        #[arg(value_name = "PATH")]
        paths: Option<Vec<PathBuf>>,
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

    /// Ignore a file when processing other subcommands.
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

    /// Encrypt a single file with a password into a dfm-encrypted blob.
    Encrypt {
        /// File to encrypt.
        #[arg(value_name = "PATH")]
        path: PathBuf,

        /// Output file (defaults to <PATH>.encrypted).
        #[arg(long, short = 'o', value_name = "OUT")]
        output: Option<PathBuf>,
    },

    /// Decrypt a dfm-encrypted file.
    Decrypt {
        /// Encrypted file.
        #[arg(value_name = "PATH")]
        path: PathBuf,

        /// Output file (defaults to the input name without the encrypted
        /// postfix; required when the input has no postfix).
        #[arg(long, short = 'o', value_name = "OUT")]
        output: Option<PathBuf>,
    },
}

