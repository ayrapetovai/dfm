use std::borrow::ToOwned;
use std::{env, fs};
use std::cmp::PartialEq;
use std::fs::File;
use std::io::{Error, ErrorKind, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::time::SystemTime;
use clap::{Parser, Subcommand};
use filetime_creation::set_file_mtime;
use filetime_creation::FileTime;
use log::{debug, error, info, log_enabled, trace, warn};
use log::Level::Trace;
use once_cell::unsync::Lazy;
use regex::Regex;

use dfm::*;

// opts https://docs.rs/clap/latest/clap/_derive/_cookbook/git_derive/index.html
// toml https://docs.rs/toml/latest/toml/
// env https://docs.rs/envmnt/latest/envmnt/
// xdg https://wiki.archlinux.org/title/XDG_Base_Directory
// aes https://rust.howtos.io/a-guide-to-symmetric-encryption-in-rust/

static LONG_ABOUT: &'static str = 
r#"This program is designed to manage dotfiles wich are usualy
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
        #[arg(long, short, num_args = 0, default_value_t = false)]
        symlink: bool,

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

        /// Invert pattern matching.
        #[arg(long, short = 'v', num_args = 0, default_value_t = false)]
        invert_match: bool,

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

    // TODO remove?
    // /// Get or set config properties.
    // Config {
    //     /// Print the specified config property.
    //     #[arg(long, short, num_args = 1, required = false, required_unless_present_any = ["set", "list"], value_name = "NAME")]
    //     get: Option<String>,
    //
    //     /// Set config property to a specified value.
    //     #[arg(long, short, num_args = 2, required = false, required_unless_present_any = ["get", "list"], value_names = ["NAME", "VALUE"])]
    //     set: Option<Vec<String>>,
    //
    //     /// List all config properties.
    //     #[arg(long, short, num_args = 0, required = false, required_unless_present_any = ["get", "set"])]
    //     list: bool,
    // },

    /// Print paths
    #[command(arg_required_else_help = false)]
    Paths
}

fn init_command(args: &Args, config: &Config) -> Result<(), Error> {
    let Command::Init {
        path_to_source,
        path_to_target: path_to_target_opt,
        dry_run,
        ..
    } = &args.command else {
        return Err(Error::new(ErrorKind::Unsupported, format!("unreachable code reached: command {:?} is not `init`", args.command)));
    };

    let dry_run = if !dry_run { args.dry_run } else { true };

    debug!("init with source path {:?}", path_to_source);
    debug!("init with target path {:?}", path_to_target_opt);

    enum InitTask {
        CreateSourceRootFile(PathBuf),
        CreateSourceIgnoreFile(),
        CreateStateFile(PathBuf, PathBuf, PathBuf),
        CreateDefaultConfigFile(PathBuf)
    }

    if !path_to_source.exists() {
        info!("source dir {:?} does not exist, creating", path_to_source);
        if dry_run {
            warn!("dry run specified but it will not prevent from creating source directory {:?}", path_to_source)
        }

        let actual_path = if path_to_source.is_absolute() {
            path_to_source.clone()
        } else {
            let current_dir = env::current_dir().unwrap();
            PathBuf::from_iter(vec![current_dir, path_to_source.clone()])
        };
        fs::create_dir_all(actual_path)?;
    }

    let mut tasks = vec![];

    let mut source_directory_pointer = PathBuf::from_iter(vec![path_to_source.to_str().unwrap(), ".dfm_root"]);
    let source_dir_path = if source_directory_pointer.exists() {
        loop {
            let pointer_content = fs::read_to_string(&source_directory_pointer)?.trim().to_owned();
            if pointer_content == "." {
                break;
            } else {
                source_directory_pointer = PathBuf::from_iter(vec![source_directory_pointer.to_str().unwrap(), &pointer_content]);
            }
            trace!("searching .dfm_root in {:?}", source_directory_pointer);
        }
        fs::canonicalize(source_directory_pointer.parent().unwrap())?
    } else {
        tasks.push(InitTask::CreateSourceRootFile(PathBuf::from_iter(vec![path_to_source.to_str().unwrap(), ".dfm_root"])));
        fs::canonicalize(path_to_source)?
    };

    debug!("using source directory {:?}", source_dir_path);

    let source_ignore_file_path = calc_source_ignore_file(&source_dir_path)?;
    let source_ignore_regex = load_ignore_regex(&source_ignore_file_path)?;

    if !source_ignore_regex.matches(".dfm_root").matched_any() {
        debug!("source ignore file will be extended with \\.dfm_root");
        tasks.push(InitTask::CreateSourceIgnoreFile());
    }

    let home_dir_path = match get_home_path() {
        Some(p) => p,
        None => return Err(Error::new(ErrorKind::InvalidData, "failed to define home directory"))
    };

    let target_abs_path = if let Some(path_to_target) = path_to_target_opt {
        fs::canonicalize(path_to_target)?
    } else {
        home_dir_path
    };

    debug!("using target directory {:?}", target_abs_path);
    let state_file_path = calc_state_file_path()?;
    if state_file_path.exists() {
        debug!("state file already exists, no need to create");
    } else {
        tasks.push(InitTask::CreateStateFile(state_file_path.clone(), target_abs_path, source_dir_path.clone()));
    }

    let target_config_file_path = calc_config_file_path();
    if let Ok(config_file) = target_config_file_path {
        if !config_file.exists() {
            tasks.push(InitTask::CreateDefaultConfigFile(config_file));
        }
    }

    if dry_run {
        info!("dry run specified, no changes will be made");
    }

    if tasks.is_empty() {
        info!("nothing to do");
        return Ok(());
    }

    debug!("::init procedure begins, {} tasks", tasks.len());

    for task in tasks {
        match task {
            InitTask::CreateSourceRootFile(path) => {
                info!("create source root file {:?}", path);
                if dry_run {
                    continue;
                }
                fs::create_dir_all(path.parent().unwrap())?;
                fs::write(&path, ".")?;
            },
            InitTask::CreateSourceIgnoreFile() => {
                let mut ignore_file_records = vec![];
                ignore_file_records.push(".dfm_root");
                ignore_file_records.push(".git");
                ignore_file_records.push(".dfm_ignore_source");
                ignore_file_records.push(".dfm_ignore_target");

                info!("add file names to source ignore file {:?}", source_ignore_file_path);
                if dry_run {
                    continue;
                }

                fs::create_dir_all(source_ignore_file_path.parent().unwrap())?;
                let mut source_ignore_file = open_or_create_file(&source_ignore_file_path);

                for ignore_file_record in ignore_file_records {
                    if let Err(e) = writeln!(source_ignore_file, "{}", regex::escape(ignore_file_record)) {
                        error!("failed write path to file: {}", e);
                        return Err(e);
                    } else {
                        debug!("source ignore file: added record {}", ignore_file_record);
                    }
                }
            },
            InitTask::CreateStateFile(path, target_dir, source_dir) => {
                info!("create state file {:?}", path);
                if dry_run {
                    continue;
                }

                fs::create_dir_all(path.parent().unwrap())?;

                let empty_state = StateObject::new(target_dir, source_dir);
                write_state(&path, &empty_state)?;
            },
            InitTask::CreateDefaultConfigFile(path) => {
                info!("create config file {:?}", path);
                if dry_run {
                    continue;
                }

                let config_file = ConfigFile::from_config(config);
                write_config(&path, &config_file)?;
            }
        }
    }

    Ok(())
}

fn purge_command(config: &Config, args: &Args, path_to_config_file: &PathBuf) -> Result<(), Error> {
    let Command::Purge {
        dry_run,
        keep_source,
        keep_config_file,
        force
    } = &args.command else {
        return Err(Error::new(ErrorKind::Unsupported, format!("unreachable code reached: command {:?} is not `purge`", args.command)));
    };

    let dry_run = if !dry_run { args.dry_run } else { true };

    let ref state_directory_path = match calc_state_directory_path() {
        Ok(path) => path,
        Err(e) => return Err(e)
    };
    let (_, ref source_dir_abs_path) = calc_working_dir_paths(&config)?;
    debug!("purge path to config {:?}, state {:?}, source {:?} keep_source {}, keep_config_file {}, force {}",
        path_to_config_file, state_directory_path, source_dir_abs_path, keep_source, keep_config_file, force);

    if dry_run {
        info!("dry run specified, no changes will be made");
    }

    if !keep_config_file {
        if !path_to_config_file.exists() {
            info!("config file does not exist");
        } else {
            if !dry_run {
                fs::remove_file(path_to_config_file)?;
            }
            info!("config removed {:?}", path_to_config_file);
        }
    }

    if !keep_source {
        if !source_dir_abs_path.exists() {
            info!("source does not exits");
        } else {
            if !dry_run {
                fs::remove_dir_all(source_dir_abs_path.clone())?;
            }
            info!("source removed {:?}", source_dir_abs_path);
        }
    }

    if !state_directory_path.exists() {
        info!("state directory does not exist");
    } else {
        if !dry_run {
            fs::remove_dir_all(state_directory_path)?;
        }
        info!("state removed {:?}", state_directory_path);
    }
    Ok(())
}

fn add_command(config: &Config, args: &Args, state: &mut StateObject) -> Result<(), Error> {
    let Command::Add {
        paths,
        merge,
        allow_foreign: foreign,
        force,
        symlink,
        dry_run,
        ..
    } = &args.command else {
        return Err(Error::new(ErrorKind::Unsupported, format!("unreachable code reached: command {:?} is not `add`", args.command)));
    };

    let dry_run = if !dry_run { args.dry_run } else { true };

    debug!("add paths {:?}, merge {}, foreign {}, force {}, symlink {}", paths, merge, foreign, force, symlink);

    let (target_dir_abs_path, source_dir_abs_path) = calc_working_dir_paths(&config)?;

    let paths = match paths {
        Some(p) => p.clone(),
        None => vec![target_dir_abs_path.clone()]
    };

    let ListDirectories {
        found: traversed_paths,
        errors: error_messages,
        ..
    } = list_directory(&paths, None)?;
    debug!("traversing result is {:?}", traversed_paths);

    if !error_messages.is_empty() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!("failed to process some subdirectories or files in targets {:?}", error_messages)
        ));
    }

    let target_ignore_file_path = calc_local_ignore_file()?;
    let target_ignore_regex = load_ignore_regex(&target_ignore_file_path)?;

    #[derive(Debug)]
    enum AddTask {
        Copy(PathBuf, PathBuf),
        CreateSymlinkFilePointer(PathBuf, String),
    }

    let mut tasks: Vec<AddTask> = Vec::new();

    debug!("::check state procedure begins");

    for target_path in traversed_paths.iter() {
        debug!("checking {:?}", target_path);

        let target_path = if target_path.is_symlink() {
            debug!("target {:?} is a symlink", target_path);
            let current_dir = env::current_dir()?;

            let target_symlink_abs_path_raw = PathBuf::from_iter(vec![current_dir, target_path.clone()]);
            let root = PathBuf::from("/");
            let mut target_symlink_abs_path = fs::canonicalize(target_symlink_abs_path_raw.parent().get_or_insert(&root))?;
            target_symlink_abs_path.push(target_symlink_abs_path_raw.file_name().unwrap());
            let target_symlink_abs_path = target_symlink_abs_path;

            if let Some(pattern) = check_path_matches_regex(&target_ignore_regex, &target_symlink_abs_path) {
                info!("target symlink {:?} is ignored by regex /{}/ in file {:?}", target_symlink_abs_path, pattern, target_ignore_file_path);
                continue;
            }

            let target_symlink_pointee_rel_path = fs::read_link(&target_symlink_abs_path)?;
            let target_symlink_pointee_abs_path = fs::canonicalize(&target_symlink_pointee_rel_path)?;
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
            if *force || source_symlink_file_exists && !source_symlink_file_points_to_right_target {
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

        let target_abs_path = fs::canonicalize(&target_path)?;

        if target_abs_path.starts_with(&source_dir_abs_path) {
            info!("target {:?} resides in source directory, ignoring", target_abs_path);
            continue;
        }

        if !target_abs_path.starts_with(&target_dir_abs_path) {
            info!("target {:?} does not reside in target directory {:?}, skipping...", target_abs_path, target_dir_abs_path);
            continue;
        }

        if let Some(pattern) = check_path_matches_regex(&target_ignore_regex, &target_abs_path) {
            info!("target {:?} is ignored by regex /{}/ in file {:?}", target_abs_path, pattern, target_ignore_file_path);
            continue;
        }

        let source_abs_path = filepath_in_source_dir(&config.dot_prefix, &target_dir_abs_path, &source_dir_abs_path, &target_abs_path, None);

        // check if a conflict could take a place
        if source_abs_path.exists() {
            let source_file_rel_path = file_path_relative_to(&source_abs_path, &source_dir_abs_path);
            let source_file_rel_path = remove_dots_from_path(&source_file_rel_path);
            let sync_time_opt = state.syncs.get(source_file_rel_path.to_str().unwrap());

            let cmp = compare_files_by_timestamps(&target_abs_path, &source_abs_path, sync_time_opt)?;

            // conflict cases
            match cmp {
                CompareByTimestamp::BothModified => {
                    info!("both target {:?} and source {:?} were modified independently, `add` on this target will overwrite source",
                        target_abs_path, source_abs_path);
                    if !force {
                        continue;
                    }
                },
                CompareByTimestamp::SourceModified => {
                    info!("source {:?} was modified, `add`ing the target {:?} will overwrite changes in source.",
                              source_abs_path, target_abs_path);
                    if !force {
                        continue;
                    }
                },
                CompareByTimestamp::NonModified => {
                    info!("neither target nor source were modified");
                    if !force {
                        continue;
                    }
                },
                CompareByTimestamp::TargetModified => {
                    info!("only target {:?} was modified, no conflicts", target_abs_path);
                },
                CompareByTimestamp::NeverSynchronized => {
                    if !force {
                        warn!("target {:?}\n\tand source {:?}\n\twere not synchronized.", target_abs_path, source_abs_path);
                        warn!("Use --force to replace source with target");
                        continue; // TODO error?
                    }
                },
            }

            info!("no conflict detected for target {:?}", target_abs_path);
        } else {
            info!("source file {:?} does not exist", source_abs_path);
        }

        tasks.push(AddTask::Copy(target_abs_path, source_abs_path));
    }

    if dry_run {
        info!("dry run specified, no changes will be made");
    }

    if tasks.is_empty() {
        info!("nothing to do");
        return Ok(());
    }

    debug!("::copy procedure begins, {} tasks", tasks.len());

    for task in tasks {
        match task {
            AddTask::Copy(target_file, source_file) => {
                info!("copy target {:?} to source {:?}", target_file, source_file);
                if dry_run {
                    continue;
                }

                // This unwrap considered to be safe since source file resides in source dir,
                // thus it has a parent directory.
                fs::create_dir_all(source_file.parent().unwrap())?;
                fs::copy(&target_file, &source_file)?;

                let permissions = target_file.metadata()?.permissions();
                trace!("copy permissions {:o}", permissions.mode());
                if let Err(e) = fs::set_permissions(source_file.clone(), permissions.clone()) {
                    error!("failed to set permissions {:?} to source {:?}: {}", permissions.mode(), source_file, e)
                }

                trace!("set metadata to {:?}", source_file);
                let sync_creation = SystemTime::now();
                let source_rel_path = file_path_relative_to(&source_file, &source_dir_abs_path);
                let source_rel_path = remove_dots_from_path(&source_rel_path);
                state.syncs.insert(source_rel_path.to_str().unwrap().to_string(), sync_creation);

                let sync_creation = FileTime::from_system_time(sync_creation);

                set_file_mtime(&target_file, sync_creation)?;
                set_file_mtime(&source_file, sync_creation)?;

                if log_enabled!(Trace) {
                    let source_file_meta = source_file.metadata().unwrap();
                    let target_file_meta = target_file.metadata().unwrap();

                    let target_file_modified = target_file_meta.modified().unwrap();
                    let source_file_modified = source_file_meta.modified().unwrap();

                    trace!("final state:\n target: mtime={:?}\n source: sync={:?},\n         mtime={:?}",
                         target_file_modified, sync_creation, source_file_modified);
                }
            },
            AddTask::CreateSymlinkFilePointer(source_symlink, points_to) => {
                info!("directing source symlink file {:?} to the pointee of the target symlink {:?}", source_symlink, points_to);
                if dry_run {
                    continue;
                }

                // open if exists or create, if it doesn't
                let mut symlink_file = File::create(&source_symlink)?;
                symlink_file.write(points_to.as_bytes())?;
            },
        }
    }
    Ok(())
}

fn pull_command(config: &Config, args: &Args, state: &mut StateObject) -> Result<(), Error> {
    let Command::Pull {
        paths,
        merge,
        force,
        dry_run,
        ..
    } = &args.command else {
        return Err(Error::new(ErrorKind::Unsupported, format!("unreachable code reached: command {:?} is not `pull`", args.command)));
    };

    let dry_run = if !dry_run { args.dry_run } else { true };

    debug!("pull paths {:?}, merge {}, force {}, dry-run {}", paths, merge, force, dry_run);

    let (target_dir_abs_path, source_dir_abs_path) = calc_working_dir_paths(&config)?;

    let paths = match paths {
        Some(p) => p.clone(),
        None => vec![source_dir_abs_path.clone()]
    };

    let ListDirectories{
        found: traversed_paths,
        errors: error_messages,
        ..
    } = list_directory(&paths, None)?;
    debug!("traversing result is {:?}", traversed_paths);

    if !error_messages.is_empty() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!("failed to process some subdirectories or files in source {:?}", error_messages)
        ));
    }

    enum PullTask {
        Copy(PathBuf, PathBuf),
        CreateOrUpdateSymlink(PathBuf, String),
    }

    let target_ignore_file_path = calc_local_ignore_file()?;
    let target_ignore_regex = load_ignore_regex(&target_ignore_file_path)?;

    let mut tasks: Vec<PullTask> = vec![];

    for path in traversed_paths.iter() {
        debug!("checking {:?}", path);

        let target_abs_path = PathBuf::from_iter(vec!(&target_dir_abs_path, &path));
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
                    let source_file_content = fs::read_to_string(&source_file_abs_path)?;
                    debug!("source is a symlink file, pointing to {}", source_file_content);
                    tasks.push(PullTask::CreateOrUpdateSymlink(target_file_abs_path, source_file_content));
                    continue; // success
                } else {
                    debug!("regular file creating task");
                    tasks.push(PullTask::Copy(target_file_abs_path, source_file_abs_path));
                    continue; // success
                }
            } else if target_file_abs_path.is_symlink() && source_file_abs_path.exists() {
                let target_symlink_pointee = fs::read_link(&target_file_abs_path)?;
                let source_file_content: String = fs::read_to_string(&source_file_abs_path)?.trim().to_string();
                if !source_file_content.eq(target_symlink_pointee.to_str().unwrap()) {
                    info!("target symlink {:?} points to {:?},\n\tmust point to {:?}", target_file_abs_path, target_symlink_pointee, source_file_content);
                    tasks.push(PullTask::CreateOrUpdateSymlink(target_file_abs_path, source_file_content));
                    continue; // success
                }
            }
            // TODO check if the pointee of the symlink also is under management and needs to be pulled.
            target_file_abs_path
        } else {
            target_abs_path
        };

        if let Some(pattern) = check_path_matches_regex(&target_ignore_regex, &target_abs_path) {
            info!("target {:?} is ignored by regex /{}/ in file {:?}", target_abs_path, pattern, target_ignore_file_path);
            continue; // ok
        }

        if target_abs_path.exists() {
            if target_abs_path.is_symlink() {
                let target_symlink_followed_abs_path = fs::canonicalize(&target_abs_path).unwrap();

                let source_file_abs_path = filepath_in_source_dir(&config.dot_prefix, &target_dir_abs_path, &source_dir_abs_path, &target_abs_path, None);
                if target_symlink_followed_abs_path == source_file_abs_path {
                    info!("target symlink {:?}\t\npoints to the source file {:?}, skipping...", target_abs_path, source_file_abs_path);
                    continue; // success
                }

                let source_symlink_file_abs_path = filepath_in_source_dir(&config.dot_prefix, &target_dir_abs_path, &source_dir_abs_path, &target_abs_path, Some(&config.symlink_postfix));
                if source_symlink_file_abs_path.exists() {
                    let target_symlink_pointee_path = fs::read_link(&target_abs_path).unwrap();
                    let source_file_content = fs::read_to_string(&source_symlink_file_abs_path).unwrap();
                    if source_file_content.trim().eq(target_symlink_pointee_path.to_str().unwrap()) {
                        info!("target symlink {:?}\n\tpoints to {:?}, skipping...", target_abs_path, target_symlink_pointee_path.to_str().unwrap());
                        continue; // success
                    } else {
                        info!("target symlink {:?}\n\tpoints to {:?},\n\tmust point to {:?}", target_abs_path, target_symlink_pointee_path.to_str().unwrap(), source_file_content);
                        tasks.push(PullTask::CreateOrUpdateSymlink(target_abs_path.clone(), source_file_content));
                        continue; // success
                    }
                } else {
                    if !target_symlink_followed_abs_path.starts_with(&source_dir_abs_path) {
                        info!("target symlink {:?} does not point to the source directory, skipping...", target_abs_path);
                        // TODO remove the symlink?
                        continue; // success
                    }
                }

                // also the case is handled when the symlink pints inside the source directory but
                // to the wrong file
                tasks.push(PullTask::CreateOrUpdateSymlink(target_abs_path.clone(), source_file_abs_path.to_str().unwrap().to_string()));
                continue;
            }

            // existing target file is not a symlink
            let source_abs_path = filepath_in_source_dir(&config.dot_prefix, &target_dir_abs_path, &source_dir_abs_path, &target_abs_path, None);
            if !source_abs_path.exists() {
                info!("target {:?} is unmanaged,\n\tno source {:?} found, skipping...", target_abs_path, source_abs_path);
                continue; // TODO is this an error?
            }

            let source_file_rel_path = file_path_relative_to(&source_abs_path, &source_dir_abs_path);
            let source_file_rel_path = remove_dots_from_path(&source_file_rel_path);
            let sync_time_opt = state.syncs.get(source_file_rel_path.to_str().unwrap());

            let cmp = compare_files_by_timestamps(&target_abs_path, &source_abs_path, sync_time_opt)?;

            match cmp {
                CompareByTimestamp::BothModified => {
                    // TODO add merge
                    warn!("both source and target was modified, merge needed");
                    if !force {
                        return Err(Error::new(ErrorKind::InvalidData, "target and source have conflicting modifications"));
                    }
                },
                CompareByTimestamp::NonModified => {
                    info!("both source and target were not modified, no action needed, skipping...");
                    continue; // success
                },
                CompareByTimestamp::TargetModified => {
                    warn!("target was modified, pulling source will overwrite those changes");
                    if !force {
                        return Err(Error::new(ErrorKind::InvalidData, "target was modified"));
                    }
                },
                CompareByTimestamp::SourceModified => {
                    info!("only the source was modified")
                },
                CompareByTimestamp::NeverSynchronized => {
                    if !force {
                        warn!("target {:?}\n\tand source {:?}\n\twere not synchronized.", target_abs_path, source_abs_path);
                        warn!("Use --force to replace target with source");
                        continue; // TODO error?
                    }
                },
            }
            tasks.push(PullTask::Copy(target_abs_path.clone(), source_abs_path));
        } else {
            // target file does not exist
            debug!("target {:?} does not exist", target_abs_path);

            let source_file_abs_path = filepath_in_source_dir(&config.dot_prefix, &target_dir_abs_path, &source_dir_abs_path, &target_abs_path, None);
            if source_file_abs_path.exists() {
                info!("source {:?} will be copied\n\tto the target {:?}", source_file_abs_path, target_abs_path);
                tasks.push(PullTask::Copy(target_abs_path.clone(), source_file_abs_path));
                continue; // success
            } else {
                let source_symlink_file_abs_path = filepath_in_source_dir(&config.dot_prefix, &target_dir_abs_path, &source_dir_abs_path, &target_abs_path, Some(&config.symlink_postfix));
                if source_symlink_file_abs_path.exists() {
                    info!("source symlink file {:?} will be used to crate a target symlink", source_symlink_file_abs_path);
                    let source_file_content = fs::read_to_string(&source_symlink_file_abs_path).unwrap();
                    tasks.push(PullTask::CreateOrUpdateSymlink(target_abs_path.clone(), source_file_content));
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
            PullTask::Copy(target_file, source_file) => {
                info!("copy source {:?}\n\tto target {:?}", source_file, target_file);
                if dry_run {
                    continue;
                }

                fs::create_dir_all(target_file.parent().unwrap())?;
                fs::copy(source_file, target_file)?;

                let permissions = source_file.metadata()?.permissions();
                trace!("copy permissions {:o}", permissions.mode());
                fs::set_permissions(target_file, permissions.clone())?;

                let sync_creation = SystemTime::now();
                let source_rel_path = file_path_relative_to(&source_file, &source_dir_abs_path);
                let source_rel_path = remove_dots_from_path(&source_rel_path);
                state.syncs.insert(source_rel_path.to_str().unwrap().to_string(), sync_creation);

                let sync_creation = FileTime::from_system_time(sync_creation);

                set_file_mtime(target_file, sync_creation)?;
                set_file_mtime(source_file, sync_creation)?;

                if log_enabled!(Trace) {
                    let source_file_meta = source_file.metadata()?;
                    let target_file_meta = target_file.metadata()?;

                    let target_file_modified = target_file_meta.modified()?;
                    let source_file_modified = source_file_meta.modified()?;

                    trace!("final state:\n target: mtime={:?}\n source: sync={:?},\n         mtime={:?}",
                         target_file_modified, sync_creation, source_file_modified);
                }
            },
            PullTask::CreateOrUpdateSymlink(target_symlink_file_path, points_to) => {
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
                            return Err(e);
                        }
                    }
                }
                let points_to = if points_to.starts_with("./") {
                    &points_to[2..]
                } else {
                    points_to.as_str()
                };
                let pointee = PathBuf::from(points_to);

                symlink::symlink_file(pointee, target_symlink_file_path)?;
                debug!("target symlink {:?} updated", target_symlink_file_path)
            }
        }
    }
    Ok(())
}

fn forget_command(config: &Config, args: &Args, state: &mut StateObject) -> Result<(), Error> {
    let Command::Forget {
        paths,
        force,
        dry_run,
        ..
    } = &args.command else {
        return Err(Error::new(ErrorKind::Unsupported, format!("unreachable code reached: command {:?} is not `forget`", args.command)));
    };

    let dry_run = if !dry_run { args.dry_run } else { true };

    debug!("add paths {:?}, force {}, dry-run {}", paths, force, dry_run);

    let (target_dir_abs_path, source_dir_abs_path) = calc_working_dir_paths(&config)?;

    let paths = match paths {
        Some(p) => p.clone(),
        None => vec![target_dir_abs_path.clone()]
    };

    let ListDirectories {
        found: traversed_paths,
        errors: error_messages,
        ..
    } = list_directory(&paths, None)?;
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
        debug!("checking {:?}", target_path);

        if target_path.is_symlink() {
            let target_abs_path = PathBuf::from_iter(vec![&target_dir_abs_path, &target_path]);
            let target_abs_path = remove_dots_from_path(&target_abs_path);
            let target_symlink_pointee_path = fs::read_link(&target_abs_path)?;

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
                let source_file_content = fs::read_to_string(&source_symlink_file_abs_path)?;
                if source_file_content.trim().eq(target_symlink_pointee_path.to_str().unwrap()) {
                    info!("target symlink {:?}\n\tpoints to {:?}, skipping...", target_abs_path, target_symlink_pointee_path.to_str().unwrap());
                    tasks.push(ForgetTask::Delete(source_symlink_file_abs_path));
                    continue;
                } else {
                    info!("target symlink {:?}\n\tpoints to {:?},\n\tmust point to {:?}", target_abs_path, target_symlink_pointee_path.to_str().unwrap(), source_file_content);
                    if *force {
                        tasks.push(ForgetTask::Delete(source_symlink_file_abs_path));
                    } else {
                        info!("specify --force to delete source {:?}", source_symlink_file_abs_path);
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
            let target_abs_path = target_abs_path_res?; // safe
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
                            continue; // success
                        } else {
                            info!("target symlink {:?}\n\tpoints to {:?},\n\tmust point to {:?}", target_symlink_abs_path, target_symlink_pointee_path.to_str().unwrap(), source_file_content);
                            if *force {
                                tasks.push(ForgetTask::Delete(source_symlink_file_abs_path));
                            } else {
                                info!("specify --force to delete source {:?}", source_symlink_file_abs_path);
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
                    let source_file_rel_path = file_path_relative_to(&source_abs_path, &source_dir_abs_path);
                    let source_file_rel_path = remove_dots_from_path(&source_file_rel_path);
                    let sync_time_opt = state.syncs.get(source_file_rel_path.to_str().unwrap());

                    let cmp = compare_files_by_timestamps(&target_abs_path, &source_abs_path, sync_time_opt)?;
                    if CompareByTimestamp::SourceModified == cmp || CompareByTimestamp::BothModified == cmp {
                        // source was modified and if we remove it then we will lose the modifications
                        warn!("source {:?}, was modified, run with --force", source_abs_path);
                        if !force {
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
                let source_rel_path = file_path_relative_to(&source_file, &source_dir_abs_path);
                let source_rel_path = remove_dots_from_path(&source_rel_path);
                state.syncs.remove(source_rel_path.to_str().unwrap());

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

fn ignore_command(config: &Config, args: &Args) -> Result<(), Error> {
    let Command::Ignore {
        paths,
        patterns,
        dry_run,
        ..
    } = &args.command else {
        return Err(Error::new(ErrorKind::Unsupported, format!("unreachable code reached: command {:?} is not `ignore`", args.command)));
    };

    let dry_run = if !dry_run { args.dry_run } else { true };

    debug!("ignore paths {:?}, patterns {:?}, dry-run {}", paths, patterns, dry_run);

    let (target_dir_abs_path, source_dir_abs_path) = calc_working_dir_paths(&config)?;
    // TODO make load of the files lazy
    let target_ignore_file_path = calc_local_ignore_file().unwrap();
    let target_ignore_regex = load_ignore_regex(&target_ignore_file_path)?;

    let source_ignore_file_path = calc_source_ignore_file(&source_dir_abs_path)?;
    let source_ignore_regex = load_ignore_regex(&source_ignore_file_path)?;

    let traversed_paths = match paths {
        Some(p) => p,
        None if patterns.is_none() => {
            let ListDirectories {
                found: traversed_paths,
                errors: error_messages,
                ..
            } = list_directory(&vec![target_dir_abs_path.clone()], Some(&target_ignore_regex))?;

            if !error_messages.is_empty() {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    format!("failed to process some subdirectories or files in targets {:?}", error_messages)
                ));
            }

            &traversed_paths.clone()
        },
        None => &vec![]
    };

    debug!("traversing result is {:?}", traversed_paths);

    let mut target_ignore_paths = vec![];
    let mut source_ignore_paths = vec![];

    for path in traversed_paths {
        debug!("check path {:?}", path);
        let abs_path = fs::canonicalize(path)?;

        // TODO check if file to be ignored is already added to source then
        //  report error, ignore is failed.

        if abs_path.starts_with(&source_dir_abs_path) {
            let rel_path = file_path_relative_to(&abs_path, &source_ignore_file_path);
            if source_ignore_regex.matches(rel_path.to_str().unwrap()).matched_any() {
                info!("source path {:?} is ignored already", path);
                continue;
            } else {
                debug!("adding path {:?} to source ignore file {:?}", path, source_ignore_file_path);
                source_ignore_paths.push(path);
                continue;
            }
        }

        if abs_path.starts_with(&target_dir_abs_path) {
            let rel_path = file_path_relative_to(&abs_path, &target_ignore_file_path);
            if target_ignore_regex.matches(rel_path.to_str().unwrap()).matched_any() {
                info!("target path {:?} is ignored already", path);
                continue;
            } else {
                debug!("adding path {:?} to target ignore file {:?}", path, target_ignore_file_path);
                target_ignore_paths.push(path);
                continue;
            }
        }

        debug!("path {:?} was not processed", path);
    }

    let mut target_ignore_regexps = vec![];

    if let Some(patterns_args) = patterns  {
        for pattern in patterns_args {
            if let Err(e) = Regex::new(pattern) {
                error!("argument is invalid, {}", e);
                return Err(Error::other(e));
            }

            debug!("adding regex /{}/", pattern);
            target_ignore_regexps.push(pattern);
        }
    }

    if target_ignore_paths.is_empty() &&
        source_ignore_paths.is_empty() &&
        target_ignore_regexps.is_empty()
    {
        info!("nothing to do");
        return Ok(());
    }

    if dry_run {
        info!("dry run specified, no changes will be made");
    }

    debug!("::ignore procedure begins");

    let mut target_ignore_file : Lazy<File> = Lazy::new(open_or_create_target_ignore_file);

    if !target_ignore_paths.is_empty() {
        for ignore_path in target_ignore_paths {
            info!("add path {:?} to {:?}", ignore_path, target_ignore_file_path);
            if dry_run {
                continue;
            }

            let escaped_path_str = regex::escape(ignore_path.to_str().unwrap());
            if let Err(e) = writeln!(target_ignore_file, "{}", escaped_path_str) {
                error!("failed write path to file: {}", e);
                return Err(e);
            }
        }
    }

    if !target_ignore_regexps.is_empty() {
        for pattern in target_ignore_regexps {
            info!("add regex /{}/ to {:?}", pattern, target_ignore_file_path);
            if dry_run {
                continue;
            }

            if let Err(e) = writeln!(target_ignore_file, "{}", pattern) {
                error!("failed write regex to file: {}", e);
                return Err(e);
            }
        }
    }

    if !source_ignore_paths.is_empty() {
        let mut source_ignore_file = open_or_create_file(&source_ignore_file_path);
        for ignore_path in source_ignore_paths {
            info!("add path {:?} to {:?}", ignore_path, source_ignore_file_path);
            if dry_run {
                continue;
            }

            let escaped_path_str = regex::escape(ignore_path.to_str().unwrap());
            if let Err(e) = writeln!(source_ignore_file, "{}", escaped_path_str) {
                error!("failed write path to file: {}", e);
                return Err(e);
            }
        }
    }

    Ok(())
}


fn paths_command(config: &Config, path_to_config_file: &PathBuf, path_to_state_file: &PathBuf) -> Result<(), Error> {
    println!("config {:?}", path_to_config_file);
    println!("state  {:?}", path_to_state_file);

    let (target_dir_abs_apth, ref source_dir_abs_path) = calc_working_dir_paths(&config)?;
    println!("source {:?}", source_dir_abs_path);
    println!("target {:?}", target_dir_abs_apth);
    Ok(())
}

// TODO add option "backup target file before overwrite", all backups must be stored in the specified
//  directory, maybe not in the source directory. The restoring operation should look like 2-way merge.

// TODO Implement management of foreign files. Must check if their paths contain the home path
//  and ask for --force if so. Because at the other machine those files could be located in a
//  directory with some other username. Substitute that path with $HOME?

// TODO If verbosity = 1 was specified then write to stdout only one line per each target/source,
//  no matter if that was an error or a necessity to use flags --force or --merge.

// TODO add an interactive mode, the application should ask user for confirmation before each
//  modification in filesystem it wants to make. Also asking what to do instead to aborting
//  subcommand because of a check error appeared. Erase the line with that prompt and print
//  the user answer and the resulting action.

// TODO consider 3-way merge. This will require to store somewhere the synchronization source.

// TODO make error and success messages parsable by the third-party program. They must be formatted
//  and contain the sign of error or success, a filename and a sign of necessity of using --force.

// TODO add option to save the password's hash to the local file in ~/.local/state/dfm/password
//  with permission of readonly by owner only. The `init` subcommand must prompt for that? `purge`
//  subcommand must delete that file.

fn main() -> Result<(), Error> {
    let args = Args::parse();

    if let Err(e) = stderrlog::new()
        .module(module_path!())
        .verbosity(args.verbosity)
        .show_level(args.verbosity > 2)
        .init() {
        return Err(Error::other(e));
    }

    let path_to_state_file = calc_state_file_path()?;
    let state_opt = match read_state(&path_to_state_file) {
        Ok(s) => Some(s),
        Err(_) => None
    };

    let default_config = create_default_config();
    let path_to_config_file = calc_config_file_path()?;
    let config_from_file = match read_config(&path_to_config_file) {
        Ok(c) => Some(c),
        Err(_) => None
    };
    let config =  merge_configs(&default_config, &config_from_file, &state_opt);

    return match args.command {
        Command::Init { .. } => {
            init_command(&args, &config)
        },
        Command::Purge { .. } => {
            purge_command(&config, &args, &path_to_config_file)
        },
        Command::Add { .. } => {
            if state_opt.is_none() {
                return Err(Error::new(ErrorKind::NotFound, format!("state file is not found {:?}", path_to_state_file)));
            }
            let mut state = state_opt.unwrap();
            add_command(&config, &args, &mut state)?;
            write_state(&path_to_state_file, &state)
        },
        Command::Pull { .. } => {
            if state_opt.is_none() {
                return Err(Error::new(ErrorKind::NotFound, format!("state file is not found {:?}", path_to_state_file)));
            }
            let mut state = state_opt.unwrap();
            pull_command(&config, &args, &mut state)?;
            write_state(&path_to_state_file, &state)
        },
        Command::Forget { .. } => {
            if state_opt.is_none() {
                return Err(Error::new(ErrorKind::NotFound, format!("state file is not found {:?}", path_to_state_file)));
            }
            let mut state = state_opt.unwrap();
            forget_command(&config, &args, &mut state)?;
            write_state(&path_to_state_file, &state)
        },
        Command::Ignore { .. } => {
            ignore_command(&config, &args)
        },
        Command::Paths => {
            paths_command(&config, &path_to_config_file, &path_to_state_file)
        },
        _ => {
            Err(Error::new(ErrorKind::Unsupported, format!("subcommand {:?} is not implemented yet", args)))
        }
    };
}
