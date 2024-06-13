use std::borrow::ToOwned;
use std::{env, fs};
use std::fs::File;
use std::io::{Error, ErrorKind, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use clap::{Parser, Subcommand};
use filetime_creation::set_file_mtime;
use filetime_creation::FileTime;
use log::{debug, error, info, trace, warn};

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

    /// Do not perform actions, only checks and reports.
    #[arg(long, short = 'n', num_args = 0, default_value_t = false)]
    dry_run: bool,

    /// Verbosity level: 0 - quite, 1 - brief, 2 - info, 3 - debug.
    #[arg(long, short = 'v', num_args = 1, default_value_t = 1, value_name = "LEVEL_NUMBER")]
    verbosity: usize, // 0 - don't output anything, 1 - brief info, 2 - info print action, 3 - print debug

    /// Use other config.
    #[arg(long, short = 'c', num_args = 1, required = false, value_name = "PATH")]
    config: Option<PathBuf>,

    /// Prefix for dotfiles in source directory.
    #[arg(long, num_args = 1, required = false, value_name = "PREFIX")]
    dot_prefix: Option<String>,
}

#[derive(Debug, Subcommand)]
enum Command {

    // ./config/dfm/config.toml must be crated only with `init` no other command is allowed to do this
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
        /// If omitted - apply all files in the source directory.
        #[arg(value_name = "PATH")]
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
        paths: Option<Vec<PathBuf>>,

        /// Delete source file on conflict.
        #[arg(long, short, num_args = 0, default_value_t = false)]
        // TODO rename to force?
        overwrite: bool,

        /// Run only checks, no changes will be made to filesystem.
        #[arg(long, short = 'n', num_args = 0, default_value_t = false)]
        dry_run: bool,
    },

    // .dfm_ignored_paths
    // .dfm_ignored_patterns
    /// Ignore a file when processing other subcommands.
    #[command(arg_required_else_help = true)]
    Ignore {
        #[arg(long, short, num_args = 0.., value_name = "PATH")]
        files: Option<Vec<PathBuf>>,

        #[arg(long, short, num_args = 0.., value_name = "REGEXP")]
        patterns: Option<Vec<String>>,
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

        // path: bool, // print the path to the config file that will be used
    },
}

fn init_command(_config: &Config, args: &Args) -> Result<(), Error> {
    let Command::Init { path, .. } = &args.command else {
        return Err(Error::new(ErrorKind::Unsupported, format!("unreachable code reached: command {:?} is not `init`", args.command)));
    };

    debug!("init with path {}", path.to_str().unwrap());

    // TODO the apply subcommand must not overwrite the value of the source_dir variable of
    //  the programs config file. Actually the source_dir value must not be managed somehow.
    Ok(())
}

fn add_command(config: &Config, args: &Args) -> Result<(), Error> {
    let Command::Add {
        paths,
        merge,
        allow_foreign: foreign,
        overwrite,
        symlink,
        dry_run,
        ..
    } = &args.command else {
        return Err(Error::new(ErrorKind::Unsupported, format!("unreachable code reached: command {:?} is not `add`", args.command)));
    };

    let dry_run = if !dry_run { args.dry_run } else { true };

    debug!("add paths {:?}, merge {}, foreign {}, overwrite {}, symlink {}", paths, merge, foreign, overwrite, symlink);

    let Ok((target_dir_abs_path, source_dir_abs_path)) = calc_working_dir_paths(&config) else {
        panic!("cannot obtain working directories paths");
    };

    let paths = match paths {
        Some(p) => p.clone(),
        None => vec![target_dir_abs_path.clone()]
    };

    let ListDirectories {
        found: traversed_paths,
        errors: error_messages,
        ..
    } = list_directory(&paths).unwrap();
    debug!("traversing result is {:?}", traversed_paths);

    if !error_messages.is_empty() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!("failed to process some subdirectories or files in targets {:?}", error_messages)
        ));
    }

    #[derive(Debug)]
    enum AddTask {
        Copy(PathBuf, PathBuf),
        CreateSymlinkFilePointer(PathBuf, String),
    }

    let mut tasks: Vec<AddTask> = Vec::new();

    debug!("::check state procedure begins");

    for target_path in traversed_paths.iter() {
        info!("checking {:?}", target_path);

        let target_path = if target_path.is_symlink() {
            debug!("target {:?} is a symlink", target_path);
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
                    error!("cannot obtain the absolute path for target symlink {:?}: {}", target_symlink_abs_path_raw, e);
                    continue;
                }
            };
            target_symlink_abs_path.push(target_symlink_abs_path_raw.file_name().unwrap());
            let target_symlink_abs_path = target_symlink_abs_path;

            let target_symlink_pointee_rel_path = match fs::read_link(&target_symlink_abs_path) {
                Ok(p) => p,
                Err(e) => {
                    error!("cannot read target symlink {:?}: {}", target_symlink_abs_path, e);
                    continue;
                }
            };
            let target_symlink_pointee_abs_path = match fs::canonicalize(&target_symlink_pointee_rel_path) {
                Ok(p) => p,
                Err(e) => {
                    error!("cannot obtain the absolute path of the pointee of the target symlink {:?}: {}", target_symlink_abs_path, e);
                    continue;
                }
            };
            debug!("target symlink {:?}\n\tpoints to {:?}", target_symlink_abs_path, target_symlink_pointee_abs_path);

            let source_symlink_file_abs_path = filepath_in_source_dir(
                &config.dot_prefix, &target_dir_abs_path, &source_dir_abs_path,
                &target_symlink_abs_path, Some(&config.symlink_postfix)
            );
            let source_symlink_file_exists = source_symlink_file_abs_path.exists();
            let source_symlink_file_points_to_right_target = if source_symlink_file_exists {
                 match fs::read_to_string(&source_symlink_file_abs_path) {
                    Ok(file_content) => {
                        debug!("source symlink file {:?}\n\tpoints to \"{}\"", source_symlink_file_abs_path, file_content);
                        file_content.trim().eq(target_symlink_pointee_rel_path.to_str().unwrap())
                    },
                    _ => false
                }
            } else {
                false
            };
            if *overwrite || source_symlink_file_exists && !source_symlink_file_points_to_right_target {
                if !source_symlink_file_points_to_right_target {
                    debug!("source symlink file points to the wrong file, must be {:?}", &target_symlink_pointee_rel_path);
                }
                tasks.push(AddTask::CreateSymlinkFilePointer(source_symlink_file_abs_path.clone(), target_symlink_pointee_rel_path.to_str().unwrap().to_owned()));
            } else if source_symlink_file_points_to_right_target {
                debug!("for target symlink {:?},\n\tsource symlink file {:?} already exists, skipping...", target_symlink_abs_path, source_symlink_file_abs_path);
            } else if !target_symlink_pointee_abs_path.starts_with(&source_dir_abs_path) {
                debug!("for target symlink {:?},\n\tdoes not have a source symlink file {:?}", target_symlink_abs_path, source_symlink_file_abs_path);
                tasks.push(AddTask::CreateSymlinkFilePointer(source_symlink_file_abs_path.clone(), target_symlink_pointee_rel_path.to_str().unwrap().to_owned()));
            } else {
                debug!("target symlink {:?}\n\tpointee is managed as {:?}", source_symlink_file_abs_path, target_symlink_pointee_abs_path);
            };
            target_symlink_pointee_abs_path
        } else {
            target_path.clone()
        };

        let target_abs_path = match fs::canonicalize(&target_path) {
            Ok(p) => p,
            Err(e) => {
                error!("cannot obtain the absolute path of the argument {:?}: {}, skipping...", target_path, e);
                continue;
            }
        };

        if target_abs_path.starts_with(&source_dir_abs_path) {
            info!("target {:?} resides in source directory, ignoring", target_abs_path);
            continue;
        }

        if !target_abs_path.starts_with(&target_dir_abs_path) {
            info!("target {:?} does not reside in target directory {:?}, skipping...", target_abs_path, target_dir_abs_path);
            continue;
        }

        let source_file_abs_path = filepath_in_source_dir(&config.dot_prefix, &target_dir_abs_path, &source_dir_abs_path, &target_abs_path, None);

        // check if a conflict could take a place
        if source_file_abs_path.exists() {
            let cmp = match compare_files_by_timestamps(&target_abs_path, &source_file_abs_path) {
                Ok(c) => c,
                Err(e) => {
                    debug!("failed to compare target file and source file: {:?}", e);
                    continue;
                }
            };

            // conflict cases
            match cmp {
                CompareByTimestamp::BothModified => {
                    info!("both target {:?} and source {:?} were modified independently, `add` on this target will overwrite source",
                        target_abs_path, source_file_abs_path);
                    if !overwrite {
                        continue;
                    }
                },
                CompareByTimestamp::SourceModified => {
                    info!("source {:?} was modified, `add`ing the target {:?} will overwrite changes in source.",
                              source_file_abs_path, target_abs_path);
                    if !overwrite {
                        continue;
                    }
                },
                CompareByTimestamp::NonModified => {
                    info!("neither target nor source were modified");
                    if !overwrite {
                        continue;
                    }
                },
                CompareByTimestamp::TargetModified => {
                    info!("only target {:?} was modified, no conflicts", target_abs_path);
                },
            }

            info!("no conflict detected for target {:?}", target_abs_path);
        } else {
            info!("source file {:?} does not exist", source_file_abs_path);
        }

        tasks.push(AddTask::Copy(target_abs_path, source_file_abs_path));
    }

    if tasks.is_empty() {
        info!("nothing to do");
        return Ok(());
    }

    if dry_run {
        info!("dry run specified, no changes will be made");
    }

    debug!("::copy procedure begins, {} tasks", tasks.len());

    for task in tasks {
        match task {
            AddTask::Copy(target_file, source_file) => {
                info!("copy target {:?} to source {:?}", target_file, source_file);
                if dry_run {
                    continue;
                }

                if source_file.exists() {
                    if let Err(e) = fs::remove_file(source_file.clone()) {
                        error!("failed to remove source {:?}: {}", source_file, e);
                    } else {
                        info!("source {:?} removed", source_file);
                    }
                }

                // This unwrap considered to be safe since source file resides in source dir,
                // thus it has a parent directory.
                if let Err(e) = fs::create_dir_all(source_file.parent().unwrap()) {
                    info!("cannot create source parent dir {:?}: {}", source_file.parent(), e)
                }

                if let Err(e) = fs::copy(&target_file, &source_file) {
                    error!("copy failed: {}", e);
                    continue;
                } else {
                    info!("target {:?} copied to source {:?}", target_file, source_file)
                }

                let permissions = target_file.metadata().unwrap().permissions();
                trace!("copy permissions {:o}", permissions.mode());
                if let Err(e) = fs::set_permissions(source_file.clone(), permissions.clone()) {
                    error!("failed to set permissions {:?} to source {:?}: {}", permissions.mode(), source_file, e)
                }

                trace!("set metadata to {:?}", source_file);
                let source_file_meta = source_file.metadata().unwrap();
                let source_creation_time = source_file_meta.created().unwrap();
                let source_creation = FileTime::from_system_time(source_creation_time);

                if let Err(e) = set_file_mtime(target_file.clone(), source_creation) {
                    error!("failed to set mtime for target {:?}: {}", target_file, e);
                }

                if let Err(e) = set_file_mtime(source_file.clone(), source_creation) {
                    error!("failed to set mtime for source {:?}: {}", target_file, e);
                }

                let source_file_meta = source_file.metadata().unwrap();
                let target_file_meta = target_file.metadata().unwrap();

                let target_file_modified = target_file_meta.modified().unwrap();
                let source_file_created = source_file_meta.created().unwrap();
                let source_file_modified = source_file_meta.modified().unwrap();

                debug!("final state:\n target: mtime={:?}\n source: btime={:?},\n         mtime={:?}",
                         target_file_modified, source_file_created, source_file_modified);
            },
            AddTask::CreateSymlinkFilePointer(source_symlink, points_to) => {
                info!("directing source symlink file {:?} to the pointee of the target symlink {:?}", source_symlink, points_to);
                if dry_run {
                    continue;
                }

                // open if exists or create, if it doesn't
                let mut symlink_file = match File::create(&source_symlink) {
                    Ok(f) => f,
                    Err(e) => {
                        error!("failed to create/open source symlink file {:?}: {}", source_symlink, e);
                        continue;
                    }
                };
                if let Err(e) = symlink_file.write(points_to.as_bytes()) {
                    error!("failed to write a path {} into the source symlink file {:?}: {}", points_to, source_symlink, e);
                    continue;
                }
            },
        }
    }
    Ok(())
}

fn apply_command(config: &Config, args: &Args) -> Result<(), Error> {
    let Command::Apply {
        paths,
        merge,
        overwrite,
        dry_run,
        ..
    } = &args.command else {
        return Err(Error::new(ErrorKind::Unsupported, format!("unreachable code reached: command {:?} is not `apply`", args.command)));
    };

    let dry_run = if !dry_run { args.dry_run } else { true };

    debug!("apply paths {:?}, merge {}, overwrite {}, dry-run {}", paths, merge, overwrite, dry_run);

    let Ok((target_dir_abs_path, source_dir_abs_path)) = calc_working_dir_paths(&config) else {
        panic!("cannot obtain working directories paths");
    };

    let paths = match paths {
        Some(p) => p.clone(),
        None => vec![source_dir_abs_path.clone()]
    };

    let ListDirectories{
        found: traversed_paths,
        errors: error_messages,
        ..
    } = list_directory(&paths).unwrap();
    debug!("traversing result is {:?}", traversed_paths);

    if !error_messages.is_empty() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!("failed to process some subdirectories or files in source {:?}", error_messages)
        ));
    }

    enum ApplyTask {
        Copy(PathBuf, PathBuf),
        CreateOrUpdateSymlink(PathBuf, String),
    }

    let mut tasks: Vec<ApplyTask> = vec![];

    for target_path in traversed_paths.iter() {
        info!("checking {:?}", target_path);

        let target_abs_path = PathBuf::from_iter(vec!(&target_dir_abs_path, &target_path));
        let target_abs_path = remove_dots_from_path(&target_abs_path);
        debug!("target absolute path {:?}", target_abs_path);

        let target_abs_path = if target_abs_path.starts_with(&source_dir_abs_path) {
            let source_file_abs_path = target_abs_path;
            debug!("provided path of a source {:?}", source_file_abs_path);

            let target_file_rel_to_target_dir = file_path_relative_to(&source_file_abs_path, &source_dir_abs_path);
            let dot_prefix = config.dot_prefix.clone();
            let target_file_rel_to_target_dir = target_file_rel_to_target_dir.to_str().unwrap().replace(&dot_prefix, ".");
            let target_file_rel_to_target_dir = if source_file_abs_path.to_str().unwrap().ends_with(&config.symlink_postfix) {
                target_file_rel_to_target_dir.replace(&config.symlink_postfix, "")
            } else {
                target_file_rel_to_target_dir
            };
            let target_file_abs_path = PathBuf::from_iter(vec![target_dir_abs_path.to_str().unwrap(), &target_file_rel_to_target_dir]);
            let target_file_abs_path = remove_dots_from_path(&target_file_abs_path);
            debug!("inferred target {:?}", target_file_abs_path);

            if !target_file_abs_path.exists() && source_file_abs_path.exists() {
                if source_file_abs_path.to_str().unwrap().ends_with(&config.symlink_postfix) {
                    let source_file_content = fs::read_to_string(&source_file_abs_path).unwrap();
                    debug!("source is a symlink file, pointing to {}", source_file_content);
                    tasks.push(ApplyTask::CreateOrUpdateSymlink(target_file_abs_path, source_file_content));
                    continue; // success
                } else {
                    debug!("regular file creating task");
                    tasks.push(ApplyTask::Copy(target_file_abs_path, source_file_abs_path));
                    continue; // success
                }
            } else if target_file_abs_path.is_symlink() && source_file_abs_path.exists() {
                let target_symlink_pointee = fs::read_link(&target_file_abs_path).unwrap();
                let source_file_content: String = fs::read_to_string(&source_file_abs_path).unwrap().trim().to_string();
                if !source_file_content.eq(target_symlink_pointee.to_str().unwrap()) {
                    info!("target symlink {:?} points to {:?},\n\tmust point to {:?}", target_file_abs_path, target_symlink_pointee, source_file_content);
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

                let source_file_abs_path = filepath_in_source_dir(&config.dot_prefix, &target_dir_abs_path, &source_dir_abs_path, &target_abs_path, None);
                if target_symlink_followed_abs_path == source_file_abs_path {
                    info!("target symlink {:?}\t\npoints to the source file {:?}, skipping...", target_abs_path, source_file_abs_path);
                    continue;
                }

                let source_symlink_file_abs_path = filepath_in_source_dir(&config.dot_prefix, &target_dir_abs_path, &source_dir_abs_path, &target_abs_path, Some(&config.symlink_postfix));
                if source_symlink_file_abs_path.exists() {
                    let target_symlink_pointee_path = fs::read_link(&target_abs_path).unwrap();
                    let source_file_content = fs::read_to_string(&source_symlink_file_abs_path).unwrap();
                    if source_file_content.trim().eq(target_symlink_pointee_path.to_str().unwrap()) {
                        info!("target symlink {:?}\n\tpoints to {:?}, skipping...", target_abs_path, target_symlink_pointee_path.to_str().unwrap());
                        continue;
                    } else {
                        info!("target symlink {:?}\n\tpoints to {:?},\n\tmust point to {:?}", target_abs_path, target_symlink_pointee_path.to_str().unwrap(), source_file_content);
                        tasks.push(ApplyTask::CreateOrUpdateSymlink(target_abs_path.clone(), source_file_content));
                        continue;
                    }
                } else {
                    if !target_symlink_followed_abs_path.starts_with(&source_dir_abs_path) {
                        info!("target symlink {:?} does not point to the source directory, skipping...", target_abs_path);
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
            let source_abs_path = filepath_in_source_dir(&config.dot_prefix, &target_dir_abs_path, &source_dir_abs_path, &target_abs_path, None);
            if !source_abs_path.exists() {
                info!("target {:?} is unmanaged,\n\tno source {:?} found, skipping...", target_abs_path, source_abs_path);
                continue; // TODO is this an error?
            }

            let cmp = match compare_files_by_timestamps(&target_abs_path, &source_abs_path) {
                Ok(c) => c,
                Err(e) => {
                    error!("failed to compare target file and source file: {:?}", e);
                    continue;
                }
            };

            match cmp {
                CompareByTimestamp::BothModified => {
                    // TODO add merge
                    warn!("both source and target was modified, merge needed");
                    if !overwrite {
                        return Err(Error::new(ErrorKind::InvalidData, "target and source have conflicting modifications"));
                    }
                },
                CompareByTimestamp::NonModified => {
                    info!("both source and target were not modified, no action needed, skipping...");
                    continue; // success
                },
                CompareByTimestamp::TargetModified => {
                    warn!("target was modified, applying source will overwrite those changes");
                    if !overwrite {
                        return Err(Error::new(ErrorKind::InvalidData, "target was modified"));
                    }
                },
                CompareByTimestamp::SourceModified => {
                    info!("only the source was modified")
                }
            }
            tasks.push(ApplyTask::Copy(target_abs_path.clone(), source_abs_path));
        } else {
            // target file does not exist
            debug!("target {:?} does not exist", target_abs_path);

            let source_file_abs_path = filepath_in_source_dir(&config.dot_prefix, &target_dir_abs_path, &source_dir_abs_path, &target_abs_path, None);
            if source_file_abs_path.exists() {
                info!("source {:?} will be copied\n\tto the target {:?}", source_file_abs_path, target_abs_path);
                tasks.push(ApplyTask::Copy(target_abs_path.clone(), source_file_abs_path));
                continue; // success
            } else {
                let source_symlink_file_abs_path = filepath_in_source_dir(&config.dot_prefix, &target_dir_abs_path, &source_dir_abs_path, &target_abs_path, Some(&config.symlink_postfix));
                if source_symlink_file_abs_path.exists() {
                    info!("source symlink file {:?} will be used to crate a target symlink", source_symlink_file_abs_path);
                    let source_file_content = fs::read_to_string(&source_symlink_file_abs_path).unwrap();
                    tasks.push(ApplyTask::CreateOrUpdateSymlink(target_abs_path.clone(), source_file_content));
                    continue; // success
                } else {
                    warn!("for target {:?} no corresponding source file found", target_abs_path);
                }
            }
        }
    }

    if tasks.is_empty() {
        info!("nothing to do");
        return Ok(());
    }

    if dry_run {
        info!("dry run specified, no changes will be made");
    }

    debug!("::copy procedure begins, {} tasks", tasks.len());

    for task in tasks.iter() {
        match task {
            ApplyTask::Copy(target_file, source_file) => {
                info!("copy source {:?}\n\tto target {:?}", source_file, target_file);
                if dry_run {
                    continue;
                }

                if let Err(e) = fs::create_dir_all(target_file.parent().unwrap()) {
                    error!("cannot create source parent dir {:?}: {}", target_file.parent(), e);
                    continue; // error
                }

                match fs::copy(source_file, target_file) {
                    Err(e) => {
                        error!("failed to copy {}", e);
                        continue; // error
                    },
                    _ => {}
                }

                let permissions = source_file.metadata().unwrap().permissions();
                trace!("copy permissions {:o}", permissions.mode());
                if let Err(e) = fs::set_permissions(target_file, permissions.clone()) {
                    error!("failed to set permissions {:?} to source {:?}: {}", permissions.mode(), target_file, e)
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
                debug!("final state:\n target: mtime={:?}\n source: btime={:?},\n         mtime={:?}",
                         target_file_modified, source_file_created, source_file_modified);
            },
            ApplyTask::CreateOrUpdateSymlink(target_symlink_file_path, points_to) => {
                info!("create symlink {:?} pointing\n\tto {:?}", target_symlink_file_path, points_to);
                if dry_run {
                    continue;
                }

                if let Err(e) = symlink::remove_symlink_file(target_symlink_file_path) {
                    match e.kind() {
                        ErrorKind::NotFound => {
                            info!("target symlink {:?} does not exist", target_symlink_file_path);
                            // is ok
                        },
                        _ => {
                            error!("failed to remove symlink {:?}: {}", target_symlink_file_path, e);
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
                    error!("failed to crate a symlink {:?}: {}", target_symlink_file_path, e);
                    continue; // TODO error
                }
                debug!("target symlink {:?} updated", target_symlink_file_path)
            }
        }
    }
    Ok(())
}

fn forget_command(config: &Config, args: &Args) -> Result<(), Error> {
    let Command::Forget {
        paths,
        overwrite,
        dry_run,
        ..
    } = &args.command else {
        return Err(Error::new(ErrorKind::Unsupported, format!("unreachable code reached: command {:?} is not `add`", args.command)));
    };

    let dry_run = if !dry_run { args.dry_run } else { true };

    debug!("add paths {:?}, overwrite {}, dry-run {}", paths, overwrite, dry_run);

    let Ok((target_dir_abs_path, source_dir_abs_path)) = calc_working_dir_paths(&config) else {
        panic!("cannot obtain working directories paths");
    };

    let paths = match paths {
        Some(p) => p.clone(),
        None => vec![target_dir_abs_path.clone()]
    };

    let ListDirectories {
        found: traversed_paths,
        errors: error_messages,
        ..
    } = list_directory(&paths).unwrap();
    debug!("traversing result is {:?}", traversed_paths);

    if !error_messages.is_empty() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!("failed to process some subdirectories or files in targets {:?}", error_messages)
        ));
    }

    #[derive(Debug)]
    enum ForgetTask {
        Delete(PathBuf),
    }

    let mut tasks: Vec<ForgetTask> = Vec::new();

    debug!("::check state procedure begins");

    for target_path in traversed_paths.iter() {
        info!("checking {:?}", target_path);

        if target_path.is_symlink() {
            let target_abs_path = PathBuf::from_iter(vec![&target_dir_abs_path, &target_path]);
            let target_abs_path = remove_dots_from_path(&target_abs_path);
            let target_symlink_pointee_path = match fs::read_link(&target_abs_path) {
                Ok(p) => p,
                Err(e) => {
                    error!("failed to follow target symlink {:?}: {}", target_abs_path, e);
                    return Err(e);
                }
            };

            debug!("target symlink {:?}\n\tpoints to {:?}", target_abs_path, target_symlink_pointee_path);
            if target_symlink_pointee_path.starts_with(&source_dir_abs_path) {
                info!("target symlink {:?}\n\tpoints into source directory, removing", target_abs_path);
                tasks.push(ForgetTask::Delete(target_abs_path.clone()));
            }

            let source_symlink_file_abs_path = filepath_in_source_dir(
                &config.dot_prefix, &target_dir_abs_path, &source_dir_abs_path,
                &target_abs_path, Some(&config.symlink_postfix)
            );
            if source_symlink_file_abs_path.exists() {
                let source_file_content = fs::read_to_string(&source_symlink_file_abs_path).unwrap();
                if source_file_content.trim().eq(target_symlink_pointee_path.to_str().unwrap()) {
                    info!("target symlink {:?}\n\tpoints to {:?}, skipping...", target_abs_path, target_symlink_pointee_path.to_str().unwrap());
                    tasks.push(ForgetTask::Delete(source_symlink_file_abs_path));
                    continue;
                } else {
                    info!("target symlink {:?}\n\tpoints to {:?},\n\tmust point to {:?}", target_abs_path, target_symlink_pointee_path.to_str().unwrap(), source_file_content);
                    if *overwrite {
                        tasks.push(ForgetTask::Delete(source_symlink_file_abs_path));
                    } else {
                        info!("specify --overwrite to delete source {:?}", source_symlink_file_abs_path);
                    }
                    continue; // success
                }
            } else {
                debug!("symlink {:?}\n\tdoes not have source symlink file {:?}, skipping...", target_abs_path, source_symlink_file_abs_path);
            }
        }

        let target_abs_path_res = fs::canonicalize(&target_path);
        if target_abs_path_res.is_err() {
            // we are given a path in source dir
            if target_path.is_symlink() {
                debug!("symlink {:?} is broken: {:?}", target_path, target_abs_path_res);
                continue; // error
            }

            let source_file_abs_path = PathBuf::from_iter(vec![&source_dir_abs_path, &target_path]);
            if source_file_abs_path.exists() {
                info!("source {:?} will be removed", source_file_abs_path);
                tasks.push(ForgetTask::Delete(source_file_abs_path));
                continue; // success
            } else {
                info!("source {:?} does not exist, skipping...", source_file_abs_path);
                continue; // success?
            }
        } else {
            let target_abs_path = target_abs_path_res.unwrap(); // safe
            if target_abs_path.starts_with(&source_dir_abs_path) {
                let source_abs_path = target_abs_path;
                debug!("target {:?} resides in source directory", source_abs_path);
                if source_abs_path.to_str().unwrap().ends_with(&config.symlink_postfix) {
                    let source_symlink_file_abs_path = source_abs_path;
                    let source_rel_path = file_path_relative_to(&source_symlink_file_abs_path, &source_dir_abs_path);
                    let source_rel_str = source_rel_path.to_str().unwrap()
                        .replace(&config.dot_prefix, ".")
                        .replace(&config.symlink_postfix, "");
                    let target_symlink_abs_path = PathBuf::from_iter(vec![target_dir_abs_path.to_str().unwrap(), &source_rel_str]);
                    if target_symlink_abs_path.exists() {
                        let target_symlink_pointee_path = match fs::read_link(&target_symlink_abs_path) {
                            Ok(p) => p,
                            Err(e) => {
                                error!("failed to read symlink {:?}: {}", target_symlink_abs_path, e);
                                return Err(e);
                            }
                        };
                        let source_file_content = fs::read_to_string(&source_symlink_file_abs_path).unwrap();
                        if source_file_content.trim().eq(target_symlink_pointee_path.to_str().unwrap()) {
                            info!("target symlink {:?}\n\tpoints to {:?}, skipping...", target_symlink_abs_path, target_symlink_pointee_path.to_str().unwrap());
                            tasks.push(ForgetTask::Delete(source_symlink_file_abs_path));
                            continue;
                        } else {
                            info!("target symlink {:?}\n\tpoints to {:?},\n\tmust point to {:?}", target_symlink_abs_path, target_symlink_pointee_path.to_str().unwrap(), source_file_content);
                            if *overwrite {
                                tasks.push(ForgetTask::Delete(source_symlink_file_abs_path));
                            } else {
                                info!("specify --overwrite to delete source {:?}", source_symlink_file_abs_path);
                            }
                            continue; // success
                        }
                    }
                } else {
                    info!("source {:?} will be removed", source_abs_path);
                    tasks.push(ForgetTask::Delete(source_abs_path));
                    continue; // success
                }
            } else if target_abs_path.starts_with(&target_dir_abs_path) {
                debug!("target {:?} resides in target directory", target_abs_path);
                let source_abs_path = filepath_in_source_dir(&config.dot_prefix, &target_dir_abs_path, &source_dir_abs_path, &target_abs_path, None);
                if source_abs_path.exists() {
                    let cmp = match compare_files_by_timestamps(&target_abs_path, &source_abs_path) {
                        Ok(c) => c,
                        Err(e) => {
                            error!("failed to compare target and source files: {}", e);
                            return Err(e);
                        }
                    };
                    if CompareByTimestamp::SourceModified == cmp || CompareByTimestamp::BothModified == cmp {
                        // source was modified and if we remove it then we will lose the modifications
                        warn!("source {:?}, was modified, run with --overwrite", source_abs_path);
                        if !overwrite {
                            continue; // error
                        }
                    }
                    info!("source {:?} will be removed", source_abs_path);
                    tasks.push(ForgetTask::Delete(source_abs_path));
                    continue; // success
                } else {
                    info!("source {:?} does not exist, skipping...", source_abs_path);
                    continue; // success
                }
            } else {
                warn!("target {:?}\n\tresides outside the target directory {:?}, skipping...", target_abs_path, target_dir_abs_path);
                continue;
            }
        }
    }

    if tasks.is_empty() {
        info!("nothing to do");
        return Ok(());
    }

    if dry_run {
        info!("dry run specified, no changes will be made");
    }

    debug!("::remove procedure begins, {} tasks", tasks.len());

    for task in tasks.iter() {
        match task {
            ForgetTask::Delete(source_file) => {
                info!("delete {:?}", source_file);
                if dry_run {
                    continue;
                }

                // fs::remove_file does not follow links, it deletes the specified file
                // even it is a symlink
                if let Err(e) = fs::remove_file(&source_file) {
                    error!("failed to remove file {:?}: {}", source_file, e);
                    return Err(e);
                }

                let mut parent_opt = source_file.parent();
                while let Some(dir) = parent_opt {
                    parent_opt = dir.parent();
                    if dir != source_dir_abs_path && dir.starts_with(&source_dir_abs_path) && dir.read_dir()?.next().is_none() {
                        info!("removing empty directory {:?}", dir);
                        if let Err(e) = fs::remove_dir(dir) {
                            error!("failed to remove parent directory {:?}: {}", dir, e);
                            return Err(e);
                        }
                    } else {
                        trace!("removing stopped at {:?}", dir);
                        break;
                    }
                }
            },
        }
    }
    Ok(())
}

// TODO add option "backup target file before overwrite", all backups must be stored in the specified
//  directory, maybe not in the source directory.

// TODO Implement management of foreign files. Must check if their paths contain the home path
//  and ask for --force if so. Because at the other machine those files could be located in a
//  directory with some other username. Substitute that path with $HOME?

// TODO If verbosity = 1 was specified then write to stdout only one line per each target/source,
//  no matter if that was an error or a necessity to use flags --overwrite or --merge.

// TODO add an interactive mode, the application should ask user before each modification in
//  filesystem it wants to make.

fn main() -> Result<(), Error> {
    let args = Args::parse();

    if let Err(e) = stderrlog::new()
        .module(module_path!())
        .verbosity(args.verbosity)
        .show_level(args.verbosity > 2)
        .init() {
        return Err(Error::other(e));
    }

    if !envmnt::exists("HOME") {
        return Err(Error::new(ErrorKind::Unsupported, "Environment variable $HOME is not set"));
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

    return match args.command {
        Command::Init { .. } => {
            init_command(&merged_config, &args)
        },
        Command::Add { .. } => {
            add_command(&merged_config, &args)
        },
        Command::Apply { .. } => {
            apply_command(&merged_config, &args)
        },
        Command::Forget { .. } => {
            forget_command(&merged_config, &args)
        },
        _ => {
            Err(Error::new(ErrorKind::Unsupported, format!("subcommand {:?} is not implemented yet", args)))
        }
    };
}
