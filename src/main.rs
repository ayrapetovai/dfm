use std::fs;
use std::fs::File;
use std::io::Read;
use std::ops::{Add};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::str::FromStr;
use clap::{Parser, Subcommand, ValueEnum};

// opts https://docs.rs/clap/latest/clap/_derive/_cookbook/git_derive/index.html
// toml https://docs.rs/toml/latest/toml/
// env https://docs.rs/envmnt/latest/envmnt/

// $ cat ~/.cellar/.dfm-root
// ./dotfiles

// $ cat ~/.cellar/dotfiles
// .

static CONFIG_FILE_NAME_IN_HOME: &str = ".dfm.toml";
static CONFIG_FILE_NAME_IN_XDG_CONFIG: &str = "config.toml";

#[derive(Serialize, Deserialize)]
struct Config {
    source_dir: String,
    target_dir: String,
    dot_prefix: Option<String>,
    symlinks: Option<bool>,
    hooks: Option<Vec<Hook>>,
}

#[derive(Serialize, Deserialize)]
struct Hook {
    when: String,
    execute: String,
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {

    #[command(subcommand)]
    command: Command,

    //arbitrary_command: String,

    /// Do not perform actions only checks and reports.
    #[arg(long, num_args = 0, default_value_t = false)]
    dry_run: bool,

    /// Report exhaustiveness level: 0, 1, 2.
    #[arg(long, num_args = 1, default_value_t = 1, value_name = "LEVEL")]
    verbose: u8, // 0 - don't output anything, 1 - print action, 2 - print debug

    /// Use other config.
    #[arg(long, num_args = 1, required = false, value_name = "PATH")]
    config: Option<PathBuf>,

    /// Prefix for dotfiles in source directory.
    #[arg(long, num_args = 1, required = false, value_name = "PREFIX")]
    dot_prefix: Option<String>,
}

#[derive(Debug, Subcommand)]
enum Command {

    // ./config/dfm/config.toml must be crated only with `init` no other command cannot do this
    // because otherwise it will create an empty config file with no source dir and no target dir
    /// Check marker files in source directory.
    /// Copies the config file from the source directory to the target directory.
    /// Updates the source directory location variable in config.
    Init {
        /// Specifies the source directory.
        #[arg(required = true)]
        path: PathBuf,
    },

    /// If no conflict detected copies files from the source directory to the target directory.
    /// The files considered to be managed after this operation.
    #[command(arg_required_else_help = true)]
    Apply {
        // empty means all, alright?
        /// Files to be updated from source directory to target.
        paths: Option<Vec<PathBuf>>,
        /// Invert pattern matching.
        invert_match: bool, // -v
        /// Run merge tool on conflicts.
        merge: bool,
    },

    Status {
        all: bool,
        managed: bool,
        unmanaged: bool,
        unapplyed: bool,
        ignored: bool,
        ignored_patterns: bool,
        unused_ignored_patterns: bool,
        difference: bool,
    },

    /// If no conflicts detected copies files from the target directory to the source directory.
    /// The files considered to be managed after this operation.
    #[command(arg_required_else_help = true)]
    Add {
        /// Files to be copied to the source directory from target.
        paths: Vec<PathBuf>,

        /// Run merge tool on conflicts.
        #[arg(long, short, num_args = 0, default_value_t = false)]
        merge: bool,

        /// Force managing files outside target directory.
        #[arg(long, short, num_args = 0, default_value_t = false)]
        allow_foreign: bool,

        /// Overwrite source file on conflict.
        #[arg(long, short, num_args = 0, default_value_t = false)]
        overwrite: bool,
    },

    // must check conflicts
    Forget {
        path: PathBuf,
    },

    // TODO add to .config/dfm/config.toml?
    // .dfm-ignored-paths
    // .dfm-ignored-patterns
    Ignore {
        paths: Option<Vec<PathBuf>>,
        pattern: String,
        what: IgnoreTargetType,
    },

    Set {
        source: PathBuf,
        target: PathBuf,
    },

    Get {
        source: PathBuf,
        target: PathBuf,
    },
    PrintDefaultConfig,
}

#[derive(ValueEnum, Copy, Clone, Debug, PartialEq, Eq)]
enum IgnoreTargetType {
    Path,
    Pattern,
}

// init run
// if config file exists, check that specified source dir exists and contains .dfm-source file,
// if not create dir and file, print "already initialized"
// exit
// take a path, check if it != $HOME look for .fdm-source, if exists read the source dir path from it, if not exists
// search in the path recursively until find a file .fdm-source, if not found
// error, print help
// create ~/.config/dfm/config.toml, if exists
// do not override values, write source dir path to it, if source path has expanded $HOME prefix
// write source path with a $HOME prepended and relative path.
// Write $HOME to target path variable of the config
// if --config is given then error, print help
fn init_command(config: Option<Config>, args: &Args) {
    let Command::Init { path, .. } = &args.command else {
        panic!("Command is not init")
    };

    println!("init with path {}", path.to_str().unwrap());
}

fn add_command(config_opt: Option<Config>, args: &Args) {
    let Command::Add { paths, merge, allow_foreign: foreign, overwrite, .. } = &args.command else {
        panic!("Command is not add")
    };

    let Some(config) = config_opt else {
        panic!("Config file absent")
    };

    println!("add paths {:?}, merge {}, foreign {}, overwrite {}", paths.to_owned(), merge, foreign, overwrite);

    let target_dir_abs_path = match PathBuf::from_str(envmnt::expand(config.target_dir.as_str(), None).as_str()) {
        Ok(p) => fs::canonicalize(p).unwrap(),
        Err(e) => panic!("target directory path is bad {}", e)
    };

    let source_dir_abs_path = match PathBuf::from_str(envmnt::expand(config.source_dir.as_str(), None).as_str()) {
        Ok(p) => fs::canonicalize(p).unwrap(),
        Err(e) => panic!("source directory path is bad {}", e)
    };

    for target_file_path in paths {
        let Ok(target_file_abs_path) = fs::canonicalize(target_file_path) else {
            eprintln!("skipping path {}", target_file_path.to_str().unwrap()); // TODO dont unwrap
            continue;
        };

        if target_file_abs_path.exists() {
            eprintln!("{} does not exist", target_file_path.to_str().unwrap());
            continue;
        }

        let real_target_file_abs_path = if target_file_abs_path.is_symlink() {
            let Ok(symlink_info) = target_file_abs_path.symlink_metadata() else {
                eprintln!("cannot read metadata of the symlink {}", target_file_path.to_str().unwrap());
                continue;
            };
            let Ok(real_path) = fs::read_link(target_file_abs_path) else {
                eprintln!("cannot follow link {}", target_file_path.to_str().unwrap());
                continue;
            };
            // TODO if target file is a symlink and it points to the source file, then do nothing
            if !real_path.exists() {
                eprintln!("symlink {} points to nothing", target_file_path.to_str().unwrap());
                continue;
            }
            real_path
        } else {
            target_file_abs_path
        };

        let mut target_file_rel_to_target_dir_path_opt : Option<PathBuf> = None;
        let mut path_components = Vec::new();
        for target_file_parent in real_target_file_abs_path.ancestors() {
            if target_dir_abs_path.eq(target_file_parent) {
                // TODO test this algorithm, it seems to be broken
                target_file_rel_to_target_dir_path_opt = Some(PathBuf::from_iter(path_components));
                break;
            }
            path_components.insert(0, target_file_parent.file_name().unwrap());
        };

        if target_file_rel_to_target_dir_path_opt.is_none() && !foreign {
            eprintln!("file {} does not belong to target directory {}", target_file_path.to_str().unwrap(), target_dir_abs_path.to_str().unwrap());
            eprintln!("use --foreign to add it anyway");
            continue;
        }

        let target_file_rel_to_target_dir_path = if target_file_rel_to_target_dir_path_opt.is_none() {
            todo!() // something like "../../../home/user/other/target/dir/file"
        } else {
            target_file_rel_to_target_dir_path_opt.unwrap()
        };

        let source_file_abs_path = PathBuf::from_iter(vec![source_dir_abs_path.to_str().unwrap(), target_file_rel_to_target_dir_path.to_str().unwrap()]);
        // TODO if any symlink to source path?

        // TODO read and compare files real_target_file_abs_path and source_file_abs_path
        //  if both have changes and not --overwrite then error

        // how to find out if file have changes?
        // compare modification time and content
        // if content differs, and source file was modified after target file -> error
    }
}

fn read_config() -> Option<Config> {
    let path_to_config_file = envmnt::get_or_panic("HOME")
        .add("/")
        .add(CONFIG_FILE_NAME_IN_HOME);
    eprintln!("config file path {}", path_to_config_file);
    let config_file = File::open(path_to_config_file);
    let mut config_file_content = String::new();
    config_file.unwrap().read_to_string(&mut config_file_content).expect("TODO: panic message");
    let config_opt: Option<Config> = match toml::from_str(config_file_content.as_str()) {
        Err(_) => None,
        Ok(c) => Some(c)
    };
    config_opt
}

fn main() {
    let args = Args::parse();

    if !envmnt::exists("HOME") {
        eprintln!("Environment variable $HOME is not set")
    }

    // regular run
    // try to read ~/.config/fdm/config.toml, if not found
    // try to read ~/.dfm.toml, if not found
    // error, print help

    match args.command {
        Command::Init { .. } => {
            init_command(read_config(), &args)
        },
        Command::Add { .. } => {
            add_command(read_config(), &args)
        }
        _ => {
            println!("is not implemented")
        }
    }
}
