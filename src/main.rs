use std::borrow::ToOwned;
use std::{env, fs};
use std::fs::File;
use std::io::Read;
use std::ops::Add;
use std::os::unix::fs::PermissionsExt;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::str::FromStr;
use clap::{Parser, Subcommand, ValueEnum};
use filetime_creation::set_file_mtime;
use filetime_creation::FileTime;

// opts https://docs.rs/clap/latest/clap/_derive/_cookbook/git_derive/index.html
// toml https://docs.rs/toml/latest/toml/
// env https://docs.rs/envmnt/latest/envmnt/

// $ cat ~/.cellar/.dfm-root
// ./dotfiles

// $ cat ~/.cellar/dotfiles
// .

static CONFIG_FILE_NAME_IN_HOME: &str = ".dfm.toml";
#[allow(dead_code)]
static CONFIG_FILE_NAME_IN_XDG_CONFIG: &str = "./config/dfm/config.toml";

#[derive(Serialize, Deserialize, Clone)]
struct Config {
    source_dir: String,
    target_dir: String,
    dot_prefix: Option<String>,
    symlinks: Option<bool>,
    hooks: Option<Vec<Hook>>,
}

#[derive(Serialize, Deserialize, Clone)]
struct Hook {
    when: String,
    execute: String,
}

#[derive(Parser, Debug)]
#[command(version, about = "Dotfile Manager", long_about = None)]
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

        /// Move target file to the source directory, and create a symlink in the target directory.
        #[arg(long, short, num_args = 0, default_value_t = false)]
        symlink: bool,
    },

    // must check conflicts
    Forget {
        path: PathBuf,
    },

    // TODO add to .config/dfm/config.toml?
    // .dfm_ignored_paths
    // .dfm_ignored_patterns
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

fn init_command(_config: &Config, args: &Args) {
    let Command::Init { path, .. } = &args.command else {
        panic!("unreachable code reached: command {:?} is not `init`", args.command)
    };

    println!("init with path {}", path.to_str().unwrap());
}

fn add_command(config: &Config, args: &Args) {
    let Command::Add {
        paths,
        merge,
        allow_foreign: foreign,
        overwrite,
        symlink,
        ..
    } = &args.command else {
        panic!("unreachable code reached: command {:?} is not `add`", args.command)
    };

    println!("add paths {:?}, merge {}, foreign {}, overwrite {}, symlink {}", paths.to_owned(), merge, foreign, overwrite, symlink);

    if config.source_dir.trim().is_empty() {
        println!("failed to read source directory path, does config file present on path {}?", "<todo>");
        return // TODO with error
    }

    println!("using target directory from config (original) {:?}", config.target_dir);

    let target_dir_path_expanded = envmnt::expand(&config.target_dir, None);
    println!("using target directory from config (expanded) {}", target_dir_path_expanded);

    let target_dir_abs_path = match PathBuf::from_str(target_dir_path_expanded.as_str()) {
        Ok(p) => match fs::canonicalize(p.clone()){
            Ok(p) => p,
            Err(e) => {
                println!("cannot obtain an absolute path to the target directory {:?}: {}", p, e);
                return // TODO with error
            }
        },
        Err(e) => panic!("target directory path is bad {}", e)
    };

    println!("using source directory from config (original) {:?}", config.source_dir);

    let source_dir_path_expanded = envmnt::expand(&config.source_dir, None);
    println!("using source directory from config (expanded) {}", source_dir_path_expanded);

    let source_dir_abs_path = match PathBuf::from_str(source_dir_path_expanded.as_str()) {
        Ok(p) => match fs::canonicalize(p) {
            Ok(p) => p,
            Err(e) => {
                println!("cannot access to source directory path {}: {}", source_dir_path_expanded, e);
                return // TODO with error
            }
        },
        Err(e) => {
            println!("source directory path is bad {}", e);
            return // TODO with error
        }
    };

    // TODO fill this list with files if target was a directory
    //  but not here, after all needed check.
    let mut target_to_source_list: Vec<(PathBuf, PathBuf)> = Vec::new();

    for target_path in paths {
        println!("for argument {:?}", target_path);

        if target_path.is_symlink() {
            eprintln!("target {:?} is a symlink", target_path);
            let target_symlink_metadata = match target_path.metadata() {
                Ok(m) => m,
                Err(e) => {
                    // the symlink is broken
                    println!("failed to obtain metadata of the target symlink {:?}: {}", target_path, e);
                    continue;
                }
            };

            let current_dir = match env::current_dir() {
                Ok(p) => p,
                Err(e) => {
                    panic!("cannot obtain current working directory path: {}", e);
                }
            };

            let target_symlink_abs_path = PathBuf::from_iter(vec![current_dir, target_path.clone()]);
            println!("target symlink absolute path {:?}", target_symlink_abs_path);

            // TODO if this symlink does not point to the file from the source directory then
            //  remember that we have a symlink to this target, create a symlink-stub in
            //  the source directory. Else ignore the symlink.
            //  if symlink name was    $TARGET_DIR/.file.txt
            //  then create            $SOURCE_DIR/dot_file.txt.symlink
            //  There is a case when the user has a symlink in the target directory but
            //  they want to add a real file under the management such that
            //  there would be a real file in the source dir, so they could apply that file
            //  on the other machines as a regular file.
            println!("handling symlinks is not implemented yet");
            continue;
        }

        let target_abs_path = match fs::canonicalize(target_path) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("cannot obtain the absolute path of the argument {:?}: {}, skipping...", target_path, e);
                continue;
            }
        };

        // TODO Check if this is respected tby the code
        // when source dir is:
        //     /home/user/cellar/dotfiles
        // target dir is bad: 
        //     /home/user/cellar/dotfiles
        //     /home/user/cellar
        //     /home/user
        //     /home
        //     /
        //     /home/user/cellar/ansible
        //     /home/user/.config

        let Ok(target_file_meta) = target_abs_path.metadata() else {
            eprintln!("failed to read target file metadata");
            continue;
        };

        if target_abs_path.is_dir() {
            if source_dir_abs_path.starts_with(target_abs_path.clone()) {
                eprintln!("The given target {:?} contains the source directory {:?}",
                          target_abs_path, source_dir_abs_path);
                continue;
            }

            // TODO traverse the target directory, take all paths, and add them to args...
            println!("target {:?} is a directory, `add` does not support directories yet", target_abs_path);
            continue;
        }

        let mut target_file_rel_to_target_dir_path_opt : Option<PathBuf> = None;
        let mut path_components = Vec::new();
        for target_file_parent in target_abs_path.ancestors() {
            if target_dir_abs_path.eq(target_file_parent) {
                target_file_rel_to_target_dir_path_opt = Some(PathBuf::from_iter(path_components));
                break;
            }
            if let Some(filename) = target_file_parent.file_name() {
                path_components.insert(0, filename);
            }
        };

        if target_file_rel_to_target_dir_path_opt.is_none() {
            eprintln!("file {:?} does not belong to target directory {:?}", target_path, target_dir_abs_path);
            if !foreign {
                eprintln!("use --foreign to add it anyway");
                continue;
            }
        }

        let target_file_rel_to_target_dir_path = if target_file_rel_to_target_dir_path_opt.is_none() {
            todo!("the adoption of foreign files is not implemented yet");
            // something like "../../../home/user/other/target/dir/file"
            // when this relative path will be concatenated with the source directory path we'll get:
            // /home/user/dotfiles/../../../home/user/other/target/dir/file
            // which will be resolved to /home/user/other/target/dir/file
            // and added as /home/user/dotfiles/root_home/user/other/target/dir/file
        } else {
            target_file_rel_to_target_dir_path_opt.unwrap()
        };

        println!("target file path relative to target directory {:?}", target_file_rel_to_target_dir_path.to_str());

        // TODO this converts path '.files/.file' to 'dot_files/.file' - the '.file's dot remains unchanged, is it ok?
        let source_file_rel_to_source_dir_path = target_file_rel_to_target_dir_path
            .to_str().unwrap().replace(".", config.dot_prefix.clone().unwrap().as_str());

        println!("source file path relative to source directory {}", source_file_rel_to_source_dir_path);
        let source_file_abs_path = PathBuf::from_iter(vec![source_dir_abs_path.to_str().unwrap(), &source_file_rel_to_source_dir_path]);
        // TODO if any symlink to source path?

        let source_file_exists = source_file_abs_path.exists();

        // check if a conflict could take a place
        if source_file_exists {
            let Ok(source_file_meta) = source_file_abs_path.metadata() else {
                eprintln!("failed to read source {:?}'s metadata", source_file_abs_path);
                continue;
            };

            if target_file_meta.is_dir() && source_file_meta.is_file() {
                eprintln!("target {:?} is a directory while source {:?} is a file",
                    source_file_abs_path, target_abs_path);
                continue;
            }

            if target_file_meta.is_file() && source_file_meta.is_dir() {
                eprintln!("target {:?} is a file while source {:?} is a directory",
                          source_file_abs_path, target_abs_path);
                continue;
            }

            if target_file_meta.is_dir() && source_file_meta.is_dir() {
                eprintln!("directory merging is not yet implemented");
                continue;
            }

            let source_file_created = match source_file_meta.created() {
                Ok(t) => t,
                Err(e) => {
                    panic!("this filesystem does not support creation time for files (try to recompile the program): {}", e);
                }
            };
            let target_file_modified = target_file_meta.modified().unwrap();
            let source_file_modified = source_file_meta.modified().unwrap();

            // TODO if verbose
            println!("current state:\n target: mtime={:?}\n source: btime={:?},\n         mtime={:?}",
                     target_file_modified, source_file_created, source_file_modified);

            let both_not_modified = target_file_modified == source_file_created &&
                source_file_created == source_file_modified;
            let only_source_modified = target_file_modified == source_file_created &&
                source_file_created < source_file_modified || target_file_modified < source_file_modified;
            let only_target_modified = target_file_modified > source_file_created &&
                source_file_created == source_file_modified || target_file_modified > source_file_modified;
            let both_modified = target_file_modified > source_file_created &&
                source_file_created < source_file_modified;

            // TODO if source file does not required to be changed still
            //  need to check its permissions, and copy them if needed.
            //  Modifying permission does not make modification date change.

            // TODO if target file was a symlink need to check if this link
            //  points to the particular source file, and fix it if needed.

            // conflict cases
            if both_modified {
                eprintln!("both target {:?} and source {:?} were modified independently, `add` on this target will overwrite source",
                          target_abs_path, source_file_abs_path);
                if !overwrite {
                    continue;
                }
            }

            if only_source_modified {
                eprintln!("source {:?} was modified, `add`ing the target {:?} will overwrite changes in source.",
                          source_file_abs_path, target_abs_path);
                if !overwrite {
                    continue;
                }
            }

            if both_not_modified {
                eprintln!("not target nor source was modified");
                if !overwrite {
                    continue;
                }
            }

            if only_target_modified { // TODO if verbose
                eprintln!("only target {:?} was modified, no conflicts", target_abs_path);
            }

            eprintln!("no conflict detected for target {:?}", target_abs_path);
        } else if true { // TODO if verbose
            println!("source file {:?} does not exist", source_file_abs_path)
        }

        target_to_source_list.push((target_abs_path, source_file_abs_path));
    }
    // TODO check if one can be moved to the other
    //  if content differs

    // TODO filter target duplicates
    // TODO file conflicts like: (tgt1 -> src1) and (tgt2 -> src1) and (tgt1 != tgt2)

    for (target_file, source_file) in target_to_source_list {
        println!("::copy procedure begins, copying {:?} to {:?}", target_file, source_file);
        if let Err(e) = fs::remove_file(source_file.clone()) {
            println!("failed to remove source {:?}: {}", source_file, e);
        } else {
            println!("source {:?} removed", source_file);
        }

        if let Err(e) = fs::copy(target_file.clone(), source_file.clone()) {
            eprintln!("copy failed: {}", e)
        } else {
            eprintln!("target {:?} copied to source {:?}", target_file, source_file)
        }

        let permissions = target_file.metadata().unwrap().permissions();
        println!("copy permissions {:o}", permissions.mode());
        if let Err(e) = fs::set_permissions(source_file.clone(), permissions.clone()) {
            println!("failed to set permissions {:?} to source {:?}: {}", permissions.mode(), source_file, e)
        }

        // TODO fix: if command line is `dfm add .dir` we fall this far, must stop earlier
        println!("set metadata to {:?}", source_file);
        let source_file_meta = source_file.metadata().unwrap();
        let source_creation_time =  source_file_meta.created().unwrap();
        let source_creation = FileTime::from_system_time(source_creation_time);

        if let Err(e)  = set_file_mtime(target_file.clone(), source_creation) {
            eprintln!("failed to set mtime for target {:?}: {}", target_file, e);
        }

        if let Err(e)  = set_file_mtime(source_file.clone(), source_creation) {
            eprintln!("failed to set mtime for source {:?}: {}", target_file, e);
        }

        let source_file_meta = source_file.metadata().unwrap();
        let target_file_meta = target_file.metadata().unwrap();

        let target_file_modified = target_file_meta.modified().unwrap();
        let source_file_created = source_file_meta.created().unwrap();
        let source_file_modified = source_file_meta.modified().unwrap();

        // TODO if verbose
        println!("final state:\n target: mtime={:?}\n source: btime={:?},\n         mtime={:?}",
                 target_file_modified, source_file_created, source_file_modified);
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
    let config_opt: Option<Config> = match toml::from_str(&config_file_content) {
        Err(_) => None,
        Ok(c) => Some(c)
    };
    config_opt
}

fn merge_configs(default: &Config, custom_opt: &Option<Config>) -> Config {
    match custom_opt {
        Some(custom) =>
            Config {
                source_dir: custom.source_dir.to_owned(),
                target_dir: if ! custom.target_dir.is_empty() {
                        custom.target_dir.to_owned()
                    } else {
                        default.target_dir.to_owned()
                    },
                dot_prefix: if custom.dot_prefix.is_some() {
                        custom.dot_prefix.to_owned()
                    } else {
                        default.dot_prefix.to_owned()
                    },
                symlinks: if custom.symlinks.is_some() {
                        custom.symlinks.to_owned()
                    } else {
                        default.symlinks.to_owned()
                    },
                hooks: None,
            },
        None => default.clone()
    }
}

fn main() {
    let args = Args::parse();

    if !envmnt::exists("HOME") {
        eprintln!("Environment variable $HOME is not set")
    }

    let default_config = Config {
        source_dir: "".to_owned(),
        target_dir: "$HOME".to_owned(),
        dot_prefix: Some("dot_".to_owned()),
        symlinks: None,
        hooks: None,
    };

    let config = read_config();
    let merged_config =  merge_configs(&default_config, &config);

    match args.command {
        Command::Init { .. } => {
            init_command(&merged_config, &args)
        },
        Command::Add { .. } => {
            add_command(&merged_config, &args)
        }
        _ => {
            println!("subcommand {:?} is not implemented yet", args.command)
        }
    }
}
