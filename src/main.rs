use std::borrow::ToOwned;
use std::{env, fs};
use std::fs::File;
use std::io::{ErrorKind, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use clap::{Parser, Subcommand, ValueEnum};
use filetime_creation::set_file_mtime;
use filetime_creation::FileTime;

use dfm::*;

// opts https://docs.rs/clap/latest/clap/_derive/_cookbook/git_derive/index.html
// toml https://docs.rs/toml/latest/toml/
// env https://docs.rs/envmnt/latest/envmnt/

static CONFIG_FILE_NAME_IN_HOME: &str = ".dfm.toml";
static CONFIG_FILE_NAME_IN_XDG_CONFIG: &str = "./config/dfm/config.toml";

#[derive(Parser, Debug)]
#[command(version, about = "Dotfile Manager", long_about = None)]
struct Args {

    #[command(subcommand)]
    command: Command,

    //arbitrary_command: String,

    /// Do not perform actions only checks and reports.
    #[arg(long, short = 'n', num_args = 0, default_value_t = false)]
    dry_run: bool,

    /// Report exhaustiveness level: 0, 1, 2.
    #[arg(long, short = 'v', num_args = 1, default_value_t = 1, value_name = "LEVEL")]
    verbose: u8, // 0 - don't output anything, 1 - print action, 2 - print debug

    /// Use other config.
    #[arg(long, short = 'c', num_args = 1, required = false, value_name = "PATH")]
    config: Option<PathBuf>,

    /// Prefix for dotfiles in source directory.
    #[arg(long, num_args = 1, required = false, value_name = "PREFIX")]
    dot_prefix: Option<String>,
}

#[derive(Debug, Subcommand)]
enum Command {

    // ./config/dfm/config.toml must be crated only with `init` no other command cannot do this
    // because otherwise it will create an empty config file with no source dir and no target dir
    /// Initialize config file with the source directory.
    Init {
        /// Specifies the source directory.
        #[arg(required = true)]
        path: PathBuf,
    },

    // TODO rename to `push`?
    /// Add file under management, or copy changes to the source directory.
    #[command(arg_required_else_help = false)]
    Add {
        /// Files to be copied to the source directory from target.
        paths: Option<Vec<PathBuf>>,

        /// Run merge tool on conflicts.
        #[arg(long, short, num_args = 0, default_value_t = false)]
        merge: bool,

        /// Force managing files outside target directory.
        #[arg(long, short, num_args = 0, default_value_t = false)]
        allow_foreign: bool,

        /// Overwrite source file on conflict and add symlinks.
        #[arg(long, short, num_args = 0, default_value_t = false)]
        // TODO rename to `force`?
        overwrite: bool,

        /// Move target file to the source directory, and create a symlink in the target directory.
        #[arg(long, short, num_args = 0, default_value_t = false)]
        symlink: bool,

        /// Run only checks, no changes will be made to filesystem.
        #[arg(long, short = 'n', num_args = 0, default_value_t = false)]
        dry_run: bool,
    },

    // TODO rename to `pull`?
    /// Copy changes for source directory to the target directory.
    #[command(arg_required_else_help = false)]
    Apply {
        /// Files to be updated from source directory to target.
        paths: Option<Vec<PathBuf>>,

        /// Invert pattern matching.
        #[arg(long, short, num_args = 0, default_value_t = false)]
        invert_match: bool, // -v

        /// Run merge tool on conflicts.
        #[arg(long, short, num_args = 0, default_value_t = false)]
        merge: bool,

        /// Overwrite target file on conflict.
        #[arg(long, short, num_args = 0, default_value_t = false)]
        // TODO rename to force
        overwrite: bool,

        /// Run only checks, no changes will be made to filesystem.
        #[arg(long, short = 'n', num_args = 0, default_value_t = false)]
        dry_run: bool,
    },

    /// Show status of managed files. [default: --difference]
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

        /// Source files that was not applied.
        #[arg(long, short, num_args = 0, default_value_t = false)]
        never_applied: bool,

        /// List ignored files.
        #[arg(long, short = 'i', num_args = 0, default_value_t = false)]
        ignored: bool,

        /// List pattern used to ignore files.
        #[arg(long, short = 'p', num_args = 0, default_value_t = false)]
        ignored_patterns: bool,

        /// List unused ignore patterns.
        #[arg(long, short = 'P', num_args = 0, default_value_t = false)]
        useless_patterns: bool,
    },

    // must check conflicts
    /// Remove file from management (does not delete the file).
    Forget {
        path: PathBuf,
    },

    // TODO add to .config/dfm/config.toml?
    // .dfm_ignored_paths
    // .dfm_ignored_patterns
    /// Ignore a file executing add, apply or status subcommands.
    Ignore {
        paths: Option<Vec<PathBuf>>,
        pattern: String,
        what: IgnoreTargetType,
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
    // TODO The supposed workflow is:
    //  Setting up an existing repo with dotfiles:
    //  - user downloads the repository, with the source directory locating at the root of
    //  the repository, or a one of its subdirectories.
    //  - user executes `$ dfm init path/to/repo/` or with path directly to the source directory.
    //  - user executes `$ dfm apply` to copy all the dotfiles to the home directory.
    //  or creating a new repo for dotfiles:
    //  - user crates a directory somewhere in filesystem to make it a source directory.
    //  - user executes `$ dfm init path/to/that/new/dir`.
    //  - user executes `$ dfm add` to add all dotfiles under the management.
    //  The given path considered to be the source directory path
    //  If not exists then exit with error.
    //  If this path contains a file .dfm-root, then the program reads the file content,
    //  the content is a path to the source root.
    //  If the path from the .dfm-root does not exist then exit with error.
    //  Recursively search for the source directory, by the way.
    //  Having source directory, search a config file of the program inside of it,
    //  apply the `apply` subcommand to the found config file.
    //  In the config file in the target directory, we must set the `source_dir` variable
    //  to the path of the source source directory.
    //  Create a file .dfm-root with content "." if not exists in the source directory.

    // TODO the apply subcommand must not overwrite the value of the source_dir variable of
    //  the programs config file. Actually the value mast not be manages somehow.
}

fn add_command(config: &Config, args: &Args) {
    let Command::Add {
        paths,
        merge,
        allow_foreign: foreign,
        overwrite,
        symlink,
        dry_run,
        ..
    } = &args.command else {
        panic!("unreachable code reached: command {:?} is not `add`", args.command)
    };

    println!("add paths {:?}, merge {}, foreign {}, overwrite {}, symlink {}", paths.to_owned(), merge, foreign, overwrite, symlink);

    let Ok((target_dir_abs_path, source_dir_abs_path)) = calc_working_dir_paths(&config) else {
        panic!("cannot obtain working directories paths");
    };

    // TODO add without paths-arguments must copy all files from the target dir to source
    let Some(paths) = paths else {
        println!("adding whole target directory is not implemented yet");
        return;
    };

    let ListDirectories {
        found: traversed_paths,
        errors: error_messages,
        ..
    } = list_directory(paths).unwrap();
    println!("traversing result is {:?}", traversed_paths);

    if !error_messages.is_empty() {
        println!("failed to process some subdirectories or files in targets {:?}", error_messages);
        return // TODO with error
    }

    #[derive(Debug)]
    enum AddTask {
        Copy(PathBuf, PathBuf),
        CreateSymlinkFilePointer(PathBuf, String),
    }

    let mut tasks: Vec<AddTask> = Vec::new();

    println!("::check state procedure begins");

    for target_path in traversed_paths.iter() {
        println!("for argument {:?}", target_path);

        let target_path = if target_path.is_symlink() {
            eprintln!("target {:?} is a symlink", target_path);
            let current_dir = match env::current_dir() {
                Ok(p) => p,
                Err(e) => {
                    panic!("cannot obtain current working directory path: {}", e);
                }
            };

            let target_symlink_abs_path_raw = PathBuf::from_iter(vec![current_dir, target_path.clone()]);
            let root = PathBuf::from("/");
            let mut target_symlink_abs_path = match fs::canonicalize(target_symlink_abs_path_raw.parent().get_or_insert(&root)) {
                Ok(p) => p,
                Err(e) => {
                    println!("cannot obtain the absolute path for target symlink {:?}: {}", target_symlink_abs_path_raw, e);
                    continue;
                }
            };
            target_symlink_abs_path.push(target_symlink_abs_path_raw.file_name().unwrap());
            let target_symlink_abs_path = target_symlink_abs_path;

            let target_symlink_pointee_rel_path = match fs::read_link(&target_symlink_abs_path) {
                Ok(p) => p,
                Err(e) => {
                    println!("cannot read target symlink {:?}: {}", target_symlink_abs_path, e);
                    continue;
                }
            };
            let target_symlink_pointee_abs_path = match fs::canonicalize(&target_symlink_pointee_rel_path) {
                Ok(p) => p,
                Err(e) => {
                    println!("cannot obtain the absolute path of the pointee of the target symlink {:?}: {}", target_symlink_abs_path, e);
                    continue;
                }
            };
            println!("target symlink {:?} points to {:?}", target_symlink_abs_path, target_symlink_pointee_abs_path);

            let source_symlink_file_abs_path = filepath_in_source_dir(&config, &target_dir_abs_path, &source_dir_abs_path, &target_symlink_abs_path, Some(".symlink"));
            let source_symlink_file_exists = source_symlink_file_abs_path.exists();
            let source_symlink_file_points_to_right_target = if source_symlink_file_exists {
                 match fs::read_to_string(&source_symlink_file_abs_path) {
                    Ok(file_content) => {
                        println!("source symlink file {:?} points to \"{}\"", source_symlink_file_abs_path, file_content);
                        file_content.trim().eq(target_symlink_pointee_rel_path.to_str().unwrap())
                    },
                    _ => false
                }
            } else {
                false
            };
            if *overwrite || source_symlink_file_exists && !source_symlink_file_points_to_right_target {
                if !source_symlink_file_points_to_right_target {
                    println!("source symlink file points to the wrong file, must be {:?}", &target_symlink_pointee_rel_path);
                }
                tasks.push(AddTask::CreateSymlinkFilePointer(source_symlink_file_abs_path.clone(), target_symlink_pointee_rel_path.to_str().unwrap().to_owned()));
            } else if source_symlink_file_points_to_right_target {
                println!("for target symlink {:?}, source symlink file {:?} already exists, skipping...", target_symlink_abs_path, source_symlink_file_abs_path);
            } else if !target_symlink_pointee_abs_path.starts_with(&source_dir_abs_path) {
                println!("for target symlink {:?}, does not have a source symlink file {:?}", target_symlink_abs_path, source_symlink_file_abs_path);
                tasks.push(AddTask::CreateSymlinkFilePointer(source_symlink_file_abs_path.clone(), target_symlink_pointee_rel_path.to_str().unwrap().to_owned()));
            } else {
                println!("target symlink {:?} pointee is managed as {:?}, to add a symlink to source directory use --overwrite", source_symlink_file_abs_path, target_symlink_pointee_abs_path);
            };
            target_symlink_pointee_abs_path
        } else {
            target_path.clone()
        };

        let target_abs_path = match fs::canonicalize(&target_path) {
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
        if target_abs_path.starts_with(&source_dir_abs_path) {
            println!("target {:?} resides in source directory, ignoring", target_abs_path);
            continue;
        }

        if !target_abs_path.starts_with(&target_dir_abs_path) {
            println!("target {:?} does not reside in target directory {:?}, skipping...", target_abs_path, target_dir_abs_path);
            continue;
        }

        let source_file_abs_path = filepath_in_source_dir(&config, &target_dir_abs_path, &source_dir_abs_path, &target_abs_path, None);

        // check if a conflict could take a place
        if source_file_abs_path.exists() {
            let cmp = match compare_files_by_timestamps(&target_abs_path, &source_file_abs_path) {
                Ok(c) => c,
                Err(e) => {
                    println!("failed to compare target file and source file: {:?}", e);
                    continue;
                }
            };

            // conflict cases
            match cmp {
                CompareByTimestamp::BothModified => {
                    eprintln!("both target {:?} and source {:?} were modified independently, `add` on this target will overwrite source",
                        target_abs_path, source_file_abs_path);
                    if ! overwrite {
                        continue;
                    }
                },
                CompareByTimestamp::SourceModified => {
                    eprintln!("source {:?} was modified, `add`ing the target {:?} will overwrite changes in source.",
                              source_file_abs_path, target_abs_path);
                    if !overwrite {
                        continue;
                    }
                },
                CompareByTimestamp::NonModified => {
                    eprintln!("neither target nor source were modified");
                    if !overwrite {
                        continue;
                    }
                },
                CompareByTimestamp::TargetModified => { // TODO if verbose
                    eprintln!("only target {:?} was modified, no conflicts", target_abs_path);
                },
            }

            eprintln!("no conflict detected for target {:?}", target_abs_path);
        } else if true { // TODO if verbose
            println!("source file {:?} does not exist", source_file_abs_path);
        }

        tasks.push(AddTask::Copy(target_abs_path, source_file_abs_path));
    }
    // TODO check if one can be moved to the other
    //  if content differs

    // TODO filter target duplicates
    // TODO file conflicts like: (tgt1 -> src1) and (tgt2 -> src1) and (tgt1 != tgt2)

    if *dry_run {
        println!("dry run specified, no changes will be made");
    }

    if tasks.is_empty() {
        println!("nothing to do");
        return; // TODO with success
    }

    println!("::copy procedure begins, {} tasks", tasks.len());

    for task in tasks {
        match task {
            AddTask::Copy(target_file, source_file) => {
                println!("copy target {:?} to source {:?}", target_file, source_file);
                if *dry_run {
                    continue;
                }

                if let Err(e) = fs::remove_file(source_file.clone()) {
                    println!("failed to remove source {:?}: {}", source_file, e);
                } else {
                    println!("source {:?} removed", source_file);
                }

                // This unwrap considered to be safe since source file resides in source dir,
                // thus it has a parent directory.
                if let Err(e) = fs::create_dir_all(source_file.parent().unwrap()) {
                    println!("cannot create source parent dir {:?}: {}", source_file.parent(), e)
                }

                if let Err(e) = fs::copy(&target_file, &source_file) {
                    eprintln!("copy failed: {}", e);
                    continue;
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
                let source_creation_time = source_file_meta.created().unwrap();
                let source_creation = FileTime::from_system_time(source_creation_time);

                if let Err(e) = set_file_mtime(target_file.clone(), source_creation) {
                    eprintln!("failed to set mtime for target {:?}: {}", target_file, e);
                }

                if let Err(e) = set_file_mtime(source_file.clone(), source_creation) {
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
            },
            AddTask::CreateSymlinkFilePointer(source_symlink, points_to) => {
                println!("directing source symlink file {:?} to the pointee of the target symlink {:?}", source_symlink, points_to);
                if *dry_run {
                    continue;
                }

                // open if exists or create, if it doesn't
                let mut symlink_file = match File::create(&source_symlink) {
                    Ok(f) => f,
                    Err(e) => {
                        println!("failed to create/open source symlink file {:?}: {}", source_symlink, e);
                        continue;
                    }
                };
                if let Err(e) = symlink_file.write(points_to.as_bytes()) {
                    println!("failed to write a path {} into the source symlink file {:?}: {}", points_to, source_symlink, e);
                    continue;
                }
            },
            // _ => {
            //     panic!("unsupported enumerator {:?}", add_task);
            // }
        }
    }
}

fn apply_command(config: &Config, args: &Args) {
    let Command::Apply {
        paths,
        merge,
        overwrite,
        dry_run,
        ..
    } = &args.command else {
        panic!("unreachable code reached: command {:?} is not `apply`", args.command)
    };

    println!("apply paths {:?}, merge {}, overwrite {}, dry-run {}", paths.to_owned(), merge, overwrite, dry_run);

    let Ok((target_dir_abs_path, source_dir_abs_path)) = calc_working_dir_paths(&config) else {
        panic!("cannot obtain working directories paths");
    };

    // TODO apply without paths-arguments must copy all files from the source dir to target
    let Some(paths) = paths else {
        println!("apply whole source directory is not implemented yet");
        return;
    };

    let ListDirectories{
        found: traversed_paths,
        errors: error_messages,
        ..
    } = list_directory(paths).unwrap();
    println!("traversing result is {:?}", traversed_paths);

    if !error_messages.is_empty() {
        println!("path traversing was done with errors: {:?}", error_messages);
        return // TODO exit with error
    }

    enum ApplyTask {
        Copy(PathBuf, PathBuf),
        CreateOrUpdateSymlink(PathBuf, String),
    }

    let mut tasks: Vec<ApplyTask> = vec![];

    for target_path in traversed_paths.iter() {
        let target_abs_path = PathBuf::from_iter(vec!(&target_dir_abs_path, &target_path));
        let target_abs_path = remove_dots_from_path(&target_abs_path);
        println!("target absolute path {:?}", target_abs_path);

        let target_abs_path = if target_abs_path.starts_with(&source_dir_abs_path) {
            let source_file_abs_path = target_abs_path;
            println!("provided path of a source {:?}", source_file_abs_path);

            let target_file_rel_to_target_dir = file_path_relative_to(&source_file_abs_path, &source_dir_abs_path);
            let dot_prefix = config.dot_prefix.clone();
            let target_file_rel_to_target_dir = target_file_rel_to_target_dir.to_str().unwrap().replace(&dot_prefix, ".");
            // TODO ".symlink" postfix is hardcoded
            let target_file_rel_to_target_dir = if source_file_abs_path.to_str().unwrap().ends_with(".symlink") {
                // TODO ".symlink" postfix is hardcoded
                target_file_rel_to_target_dir.replace(".symlink", "")
            } else {
                target_file_rel_to_target_dir
            };
            let target_file_abs_path = PathBuf::from_iter(vec![target_dir_abs_path.to_str().unwrap(), &target_file_rel_to_target_dir]);
            let target_file_abs_path = remove_dots_from_path(&target_file_abs_path);
            println!("inferred target {:?}", target_file_abs_path);

            if !target_file_abs_path.exists() && source_file_abs_path.exists() {
                // TODO ".symlink" postfix is hardcoded
                if source_file_abs_path.to_str().unwrap().ends_with(".symlink") {
                    let source_file_content = fs::read_to_string(&source_file_abs_path).unwrap();
                    println!("source is a symlink file, pointing to {}", source_file_content);
                    tasks.push(ApplyTask::CreateOrUpdateSymlink(target_file_abs_path, source_file_content));
                    continue; // success
                } else {
                    println!("regular file creating task");
                    tasks.push(ApplyTask::Copy(target_file_abs_path, source_file_abs_path));
                    continue; // success
                }
            } else if target_file_abs_path.is_symlink() && source_file_abs_path.exists() {
                let target_symlink_pointee = fs::read_link(&target_file_abs_path).unwrap();
                let source_file_content: String = fs::read_to_string(&source_file_abs_path).unwrap().trim().to_string();
                if !source_file_content.eq(target_symlink_pointee.to_str().unwrap()) {
                    println!("target symlink {:?} points to {:?}, must point to {:?}", target_file_abs_path, target_symlink_pointee, source_file_content);
                    tasks.push(ApplyTask::CreateOrUpdateSymlink(target_file_abs_path, source_file_content));
                    continue; // success
                }
            }
            // TODO check if the pointee of the symlink also is under management and needs to be
            //  applied.
            target_file_abs_path
        } else {
            target_abs_path
        };

        if target_abs_path.exists() {
            if target_abs_path.is_symlink() {
                let target_symlink_followed_abs_path = fs::canonicalize(&target_abs_path).unwrap();

                let source_file_abs_path = filepath_in_source_dir(&config, &target_dir_abs_path, &source_dir_abs_path, &target_abs_path, None);
                if target_symlink_followed_abs_path == source_file_abs_path {
                    println!("target symlink {:?} points to the source file {:?}, skipping...", target_abs_path, source_file_abs_path);
                    continue;
                }

                // TODO ".symlink" postfix is hardcoded
                let source_symlink_file_abs_path = filepath_in_source_dir(&config, &target_dir_abs_path, &source_dir_abs_path, &target_abs_path, Some(".symlink"));
                if source_symlink_file_abs_path.exists() {
                    let target_symlink_pointee_path = fs::read_link(&target_abs_path).unwrap();
                    let source_file_content = fs::read_to_string(&source_symlink_file_abs_path).unwrap();
                    if source_file_content.trim().eq(target_symlink_pointee_path.to_str().unwrap()) {
                        println!("target symlink {:?} points to {:?}, skipping...", target_abs_path, target_symlink_pointee_path.to_str().unwrap());
                        continue;
                    } else {
                        println!("target symlink {:?} points to {:?}, must point to {:?}", target_abs_path, target_symlink_pointee_path.to_str().unwrap(), source_file_content);
                        tasks.push(ApplyTask::CreateOrUpdateSymlink(target_abs_path.clone(), source_file_content));
                        continue;
                    }
                } else {
                    if !target_symlink_followed_abs_path.starts_with(&source_dir_abs_path) {
                        println!("target symlink {:?} does not point to the source directory, skipping...", target_abs_path);
                        // TODO remove the symlink?
                        continue;
                    }
                }

                // also the case is handled when the symlink pints inside the source directory but
                // to the wrong file
                tasks.push(ApplyTask::CreateOrUpdateSymlink(target_abs_path.clone(), source_file_abs_path.to_str().unwrap().to_string()));
                continue;
            }

            // existing target file is not a symlink
            let source_abs_path = filepath_in_source_dir(&config, &target_dir_abs_path, &source_dir_abs_path, &target_abs_path, None);
            if !source_abs_path.exists() {
                println!("target {:?} is unmanaged, no source {:?} found, skipping...", target_abs_path, source_abs_path);
                continue; // TODO error
            }

            let cmp = match compare_files_by_timestamps(&target_abs_path, &source_abs_path) {
                Ok(c) => c,
                Err(e) => {
                    println!("failed to compare target file and source file: {:?}", e);
                    continue;
                }
            };

            match cmp {
                CompareByTimestamp::BothModified => {
                    // TODO add merge
                    println!("both source and target was modified, merge needed");
                    if !overwrite {
                        continue; // TODO error
                    }
                },
                CompareByTimestamp::NonModified => {
                    println!("both source and target were not modified, no action needed, skipping...");
                    continue; // success
                },
                CompareByTimestamp::TargetModified => {
                    println!("target was modified, applying source will overwrite those changes");
                    if !overwrite {
                        continue; // TODO error
                    }
                },
                CompareByTimestamp::SourceModified => {
                    println!("only the source was modified")
                }
            }
            tasks.push(ApplyTask::Copy(target_abs_path.clone(), source_abs_path));
        } else {
            // target file does not exist
            println!("target {:?} does not exist", target_abs_path);

            let source_file_abs_path = filepath_in_source_dir(&config, &target_dir_abs_path, &source_dir_abs_path, &target_abs_path, None);
            if source_file_abs_path.exists() {
                println!("source {:?} will be copied to the target {:?}", source_file_abs_path, target_abs_path);
                tasks.push(ApplyTask::Copy(target_abs_path.clone(), source_file_abs_path));
                continue; // success
            } else {
                let source_symlink_file_abs_path = filepath_in_source_dir(&config, &target_dir_abs_path, &source_dir_abs_path, &target_abs_path, Some(".symlink"));
                if source_symlink_file_abs_path.exists() {
                    println!("source symlink file {:?} will be used to crate a target symlink", source_symlink_file_abs_path);
                    let source_file_content = fs::read_to_string(&source_symlink_file_abs_path).unwrap();
                    tasks.push(ApplyTask::CreateOrUpdateSymlink(target_abs_path.clone(), source_file_content));
                    continue; // success
                } else {
                    println!("for target {:?} no corresponding source file found", target_abs_path);
                }
            }
        }
    }

    // TODO add option "backup target file before overwrite", all backups must be stored in the specified
    //  directory, maybe not in the source directory.

    if *dry_run {
        println!("dry run specified, no changes will be made");
    }

    if tasks.is_empty() {
        println!("nothing to do");
        return;
    }

    println!("::copy procedure begins, {} tasks", tasks.len());

    for task in tasks.iter() {
        match task {
            ApplyTask::Copy(target_file, source_file) => {
                println!("copy source {:?} to target {:?}", source_file, target_file);
                if *dry_run {
                    continue;
                }
                match fs::copy(source_file, target_file) {
                    Err(e) => {
                        println!("failed to copy {}", e);
                        continue; // error
                    },
                    _ => {}
                }

                let permissions = source_file.metadata().unwrap().permissions();
                println!("copy permissions {:o}", permissions.mode());
                if let Err(e) = fs::set_permissions(target_file, permissions.clone()) {
                    println!("failed to set permissions {:?} to source {:?}: {}", permissions.mode(), target_file, e)
                }

                // TODO need some other algorithm do detect conflict
                //  current one forces to change the creation time of the source file
                //  Looks like the program needs to store some state,
                //  maybe use some kind of a database would be ok, like ~/.cache/dfm/synctime.db to
                //  store information for each file synchronization there?

                // recreate source file to update its creation time
                fs::remove_file(source_file).unwrap();
                fs::copy(target_file, source_file).unwrap();

                let source_file_creation_time = FileTime::from_system_time(source_file.metadata().unwrap().created().unwrap());
                set_file_mtime(target_file, source_file_creation_time).expect("TODO: panic message");
                set_file_mtime(source_file, source_file_creation_time).expect("TODO: panic message");

                let source_file_meta = source_file.metadata().unwrap();
                let target_file_meta = target_file.metadata().unwrap();

                let target_file_modified = target_file_meta.modified().unwrap();
                let source_file_created = source_file_meta.created().unwrap();
                let source_file_modified = source_file_meta.modified().unwrap();

                // TODO if verbose
                println!("final state:\n target: mtime={:?}\n source: btime={:?},\n         mtime={:?}",
                         target_file_modified, source_file_created, source_file_modified);
            },
            ApplyTask::CreateOrUpdateSymlink(target_symlink_file_path, points_to) => {
                println!("create symlink {:?} pointing to {:?}", target_symlink_file_path, points_to);
                if *dry_run {
                    continue;
                }

                if let Err(e) = symlink::remove_symlink_file(target_symlink_file_path) {
                    match e.kind() {
                        ErrorKind::NotFound => {
                            println!("target symlink {:?} does not exist", target_symlink_file_path);
                            // is ok
                        },
                        _ => {
                            println!("failed to remove symlink {:?}: {}", target_symlink_file_path, e);
                            continue; // TODO error
                        }
                    }
                }
                let points_to = if points_to.starts_with("./") {
                    &points_to[2..]
                } else {
                    points_to.as_str()
                };
                let pointee = PathBuf::from(points_to);

                if let Err(e) = symlink::symlink_file(pointee, target_symlink_file_path) {
                    println!("failed to crate a symlink {:?}: {}", target_symlink_file_path, e);
                    continue; // TODO error
                }
                println!("target symlink {:?} updated", target_symlink_file_path)
            }
        }
    }
}

// TODO fail the program execution if on of the check of any of the subcommands fails.

// TODO add an interactive mode, the application should ask user before each modification in
//  filesystem it wants to make.

fn main() {
    let args = Args::parse();

    if !envmnt::exists("HOME") {
        eprintln!("Environment variable $HOME is not set")
    }

    let default_config = create_default_config();

    // TODO to use XDS_CONFIG_HOME or not to use?
    let home_path = envmnt::get_or_panic("HOME");
    let path_to_config_in_xdg_dir = PathBuf::from_iter(vec![home_path.as_str(), &CONFIG_FILE_NAME_IN_XDG_CONFIG]);
    let path_to_config_file = if path_to_config_in_xdg_dir.exists() {
        path_to_config_in_xdg_dir
    } else {
        PathBuf::from_iter(vec![home_path.as_str(), &CONFIG_FILE_NAME_IN_HOME])
    };
    let path_to_config_file = PathBuf::from(path_to_config_file);

    let config_from_file = read_config(&path_to_config_file);
    let merged_config =  merge_configs(&default_config, &config_from_file);

    match args.command {
        Command::Init { .. } => {
            init_command(&merged_config, &args)
        },
        Command::Add { .. } => {
            add_command(&merged_config, &args)
        },
        Command::Apply { .. } => {
            apply_command(&merged_config, &args)
        },
        _ => {
            println!("subcommand {:?} is not implemented yet", args.command)
        }
    }
}
