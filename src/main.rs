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

use dfm::*;

// opts https://docs.rs/clap/latest/clap/_derive/_cookbook/git_derive/index.html
// toml https://docs.rs/toml/latest/toml/
// env https://docs.rs/envmnt/latest/envmnt/
// xdg https://wiki.archlinux.org/title/XDG_Base_Directory

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
}

#[derive(Debug, Subcommand)]
enum Command {

    // ./config/dfm/config.toml must be crated only with `init` no other command is allowed to do this
    // because otherwise it will create an empty config file with no source dir and no target dir
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

    /// Remove state file.
    Purge,

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
        // TODO rename to force
        overwrite: bool,

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

        /// Source files that was not pulled.
        #[arg(long, short, num_args = 0, default_value_t = false)]
        never_pulled: bool,

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
    },
}

fn init_command(config: &Config, args: &Args) -> Result<(), Error> {
    let Command::Init {
        path_to_source,
        path_to_target: path_to_target_opt,
        dry_run,
        ..
    } = &args.command else {
        return Err(Error::new(ErrorKind::Unsupported, format!("unreachable code reached: command {:?} is not `init`", args.command)));
    };

    let dry_run = if !dry_run { args.dry_run } else { true };

    debug!("init with path {:?}", path_to_source);

    if !path_to_source.exists() {
        return Err(Error::new(ErrorKind::NotFound, format!("directory {:?} was not found", path_to_source)));
    }

    // TODO search for the config file in source directory and pull it
    //  to the config file in the ~/.config/dfm or ~/.dfm.toml,
    //  but do not replace `source_dir` and `target_dir`.
    //  Maybe create a separate config file for them? The unaplyable file,
    //  those paths should not be overwritten with a config file source and target
    //  from the other machine.
    enum InitTask {
        CreateSourceRootFile(PathBuf),
        UpdateConfigFile(PathBuf, Option<PathBuf>, PathBuf),
        CreateStateFile(PathBuf),
    }

    let mut tasks = vec![];

    let mut source_directory_pointer = PathBuf::from_iter(vec![path_to_source.to_str().unwrap(), ".dfm_root"]);
    let source_dir_path = if source_directory_pointer.exists() {
        loop {
            let pointer_content = fs::read_to_string(&source_directory_pointer)?;
            if pointer_content == "." {
                break;
            } else {
                source_directory_pointer = PathBuf::from_iter(vec![source_directory_pointer.to_str().unwrap(), &pointer_content]);
            }
        }
        source_directory_pointer
    } else {
        tasks.push(InitTask::CreateSourceRootFile(PathBuf::from_iter(vec![path_to_source.to_str().unwrap(), ".dfm_root"])));
        path_to_source.clone()
    };

    debug!("using source directory {:?}", source_dir_path);

    if dry_run {
        info!("dry run specified, no changes will be made");
    }

    if tasks.is_empty() {
        info!("nothing to do");
        return Ok(());
    }

    debug!("::init procedure begins, {} tasks", tasks.len());

    let state_file_path = calc_state_file_path()?;
    tasks.push(InitTask::CreateStateFile(state_file_path.clone()));

    let home_dir = envmnt::get_or_panic("HOME");

    let source_dir_abs_path = fs::canonicalize(&source_dir_path)?;
    let source_dir_rel_to_home = file_path_relative_to(&source_dir_abs_path, &PathBuf::from(&home_dir));
    let mut source_dir_ags_path_with_home = String::from("$HOME/");
    source_dir_ags_path_with_home.push_str(source_dir_rel_to_home.to_str().unwrap());

    if let Some(path_to_target) = path_to_target_opt {
        let target_dir_abs_path = fs::canonicalize(path_to_target)?;
        let target_dir_rel_path = file_path_relative_to(&target_dir_abs_path, &PathBuf::from(&home_dir));
        let target_dir_rel_path = PathBuf::from_iter(vec!["$HOME/", target_dir_rel_path.to_str().unwrap()]);
        tasks.push(InitTask::UpdateConfigFile(calc_config_file_path().unwrap(), Some(target_dir_rel_path), PathBuf::from(source_dir_ags_path_with_home)));
    } else {
        tasks.push(InitTask::UpdateConfigFile(calc_config_file_path().unwrap(), None, PathBuf::from(source_dir_ags_path_with_home)));
    }

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
            InitTask::UpdateConfigFile(config_file_path, target_dir_path_opt, source_dir_path) => {
                info!("update config file {:?}", config_file_path);
                info!("\twith target directory {:?}", target_dir_path_opt);
                info!("\twith source directory {:?}", source_dir_path);
                if dry_run {
                    continue;
                }

                let status_message;
                let mut config_file = if config_file_path.exists() {
                    let config = read_config(&config_file_path);
                    if config.is_some() {
                        status_message = "updated";
                        config.unwrap()
                    } else {
                        status_message = "created";
                        ConfigFile::new()
                    }
                } else {
                    status_message = "created";
                    ConfigFile::new()
                };

                insert_config(&mut config_file, config);

                config_file.source_dir = source_dir_path.to_str().unwrap().to_string();
                if let Some(target_dir_path) = target_dir_path_opt {
                    config_file.target_dir = Some(target_dir_path.to_str().unwrap().to_string());
                }
                write_config(&config_file_path, &config_file)?;
                info!("config file {:?} {}", config_file_path, status_message);
            },
            InitTask::CreateStateFile(path) => {
                info!("create state file {:?}", path);
                if dry_run {
                    continue;
                }

                let empty_state = StateObject::new();
                write_state(&path, &empty_state)?;
            },
        }
    }
    Ok(())
}

fn add_command(config: &Config, args: &Args, state: &mut StateObject) -> Result<(), Error> {
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

    let (target_dir_abs_path, source_dir_abs_path) = calc_working_dir_paths(&config)?;

    let paths = match paths {
        Some(p) => p.clone(),
        None => vec![target_dir_abs_path.clone()]
    };

    let ListDirectories {
        found: traversed_paths,
        errors: error_messages,
        ..
    } = list_directory(&paths)?;
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
        debug!("checking {:?}", target_path);

        let target_path = if target_path.is_symlink() {
            debug!("target {:?} is a symlink", target_path);
            let current_dir = env::current_dir()?;

            let target_symlink_abs_path_raw = PathBuf::from_iter(vec![current_dir, target_path.clone()]);
            let root = PathBuf::from("/");
            let mut target_symlink_abs_path = fs::canonicalize(target_symlink_abs_path_raw.parent().get_or_insert(&root))?;
            target_symlink_abs_path.push(target_symlink_abs_path_raw.file_name().unwrap());
            let target_symlink_abs_path = target_symlink_abs_path;

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

        let target_abs_path = fs::canonicalize(&target_path)?;

        if target_abs_path.starts_with(&source_dir_abs_path) {
            info!("target {:?} resides in source directory, ignoring", target_abs_path);
            continue;
        }

        if !target_abs_path.starts_with(&target_dir_abs_path) {
            info!("target {:?} does not reside in target directory {:?}, skipping...", target_abs_path, target_dir_abs_path);
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
                    if !overwrite {
                        continue;
                    }
                },
                CompareByTimestamp::SourceModified => {
                    info!("source {:?} was modified, `add`ing the target {:?} will overwrite changes in source.",
                              source_abs_path, target_abs_path);
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
                CompareByTimestamp::NeverSynchronized => {
                    if !overwrite {
                        warn!("target {:?}\n\tand source {:?}\n\twere not synchronized.", target_abs_path, source_abs_path);
                        warn!("Use --overwrite to replace source with target");
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

                set_file_mtime(target_file.clone(), sync_creation)?;
                set_file_mtime(source_file.clone(), sync_creation)?;

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
        overwrite,
        dry_run,
        ..
    } = &args.command else {
        return Err(Error::new(ErrorKind::Unsupported, format!("unreachable code reached: command {:?} is not `pull`", args.command)));
    };

    let dry_run = if !dry_run { args.dry_run } else { true };

    debug!("pull paths {:?}, merge {}, overwrite {}, dry-run {}", paths, merge, overwrite, dry_run);

    let (target_dir_abs_path, source_dir_abs_path) = calc_working_dir_paths(&config)?;

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

    enum PullTask {
        Copy(PathBuf, PathBuf),
        CreateOrUpdateSymlink(PathBuf, String),
    }

    let mut tasks: Vec<PullTask> = vec![];

    for target_path in traversed_paths.iter() {
        debug!("checking {:?}", target_path);

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
                    if !overwrite {
                        return Err(Error::new(ErrorKind::InvalidData, "target and source have conflicting modifications"));
                    }
                },
                CompareByTimestamp::NonModified => {
                    info!("both source and target were not modified, no action needed, skipping...");
                    continue; // success
                },
                CompareByTimestamp::TargetModified => {
                    warn!("target was modified, pulling source will overwrite those changes");
                    if !overwrite {
                        return Err(Error::new(ErrorKind::InvalidData, "target was modified"));
                    }
                },
                CompareByTimestamp::SourceModified => {
                    info!("only the source was modified")
                },
                CompareByTimestamp::NeverSynchronized => {
                    if !overwrite {
                        warn!("target {:?}\n\tand source {:?}\n\twere not synchronized.", target_abs_path, source_abs_path);
                        warn!("Use --overwrite to replace target with source");
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

                    trace!("final state:\n target: mtime={:?}\n source: btime={:?},\n         mtime={:?}",
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
        overwrite,
        dry_run,
        ..
    } = &args.command else {
        return Err(Error::new(ErrorKind::Unsupported, format!("unreachable code reached: command {:?} is not `add`", args.command)));
    };

    let dry_run = if !dry_run { args.dry_run } else { true };

    debug!("add paths {:?}, overwrite {}, dry-run {}", paths, overwrite, dry_run);

    let (target_dir_abs_path, source_dir_abs_path) = calc_working_dir_paths(&config)?;

    let paths = match paths {
        Some(p) => p.clone(),
        None => vec![target_dir_abs_path.clone()]
    };

    let ListDirectories {
        found: traversed_paths,
        errors: error_messages,
        ..
    } = list_directory(&paths)?;
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
                    let source_file_rel_path = file_path_relative_to(&source_abs_path, &source_dir_abs_path);
                    let source_file_rel_path = remove_dots_from_path(&source_file_rel_path);
                    let sync_time_opt = state.syncs.get(source_file_rel_path.to_str().unwrap());

                    let cmp = compare_files_by_timestamps(&target_abs_path, &source_abs_path, sync_time_opt)?;
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

// TODO add option "backup target file before overwrite", all backups must be stored in the specified
//  directory, maybe not in the source directory.

// TODO Implement management of foreign files. Must check if their paths contain the home path
//  and ask for --force if so. Because at the other machine those files could be located in a
//  directory with some other username. Substitute that path with $HOME?

// TODO If verbosity = 1 was specified then write to stdout only one line per each target/source,
//  no matter if that was an error or a necessity to use flags --overwrite or --merge.

// TODO add an interactive mode, the application should ask user before each modification in
//  filesystem it wants to make.

// TODO the pull subcommand must not overwrite the value of the source_dir variable of
//  the programs config file. Actually the source_dir value must not be managed somehow.

fn main() -> Result<(), Error> {
    let args = Args::parse();

    if let Err(e) = stderrlog::new()
        .module(module_path!())
        .verbosity(args.verbosity)
        .show_level(args.verbosity > 2)
        .init() {
        return Err(Error::other(e));
    }

    let default_config = create_default_config();
    let path_to_config_file = calc_config_file_path()?;
    let config_from_file = read_config(&path_to_config_file);
    let config =  merge_configs(&default_config, &config_from_file);

    let path_to_state_file = calc_state_file_path()?;
    let state_opt = read_state(&path_to_state_file);

    return match args.command {
        Command::Init { .. } => {
            init_command(&config, &args)
        },
        Command::Add { .. } => {
            if state_opt.is_none() {
                return Err(Error::new(ErrorKind::NotFound, format!("state file is not found {:?}", path_to_state_file)));
            }
            let mut state = state_opt.unwrap();
            if let Err(e) = add_command(&config, &args, &mut state) { return Err(e) }
            write_state(&path_to_state_file, &state)
        },
        Command::Pull { .. } => {
            if state_opt.is_none() {
                return Err(Error::new(ErrorKind::NotFound, format!("state file is not found {:?}", path_to_state_file)));
            }
            let mut state = state_opt.unwrap();
            if let Err(e) = pull_command(&config, &args, &mut state) { return Err(e) }
            write_state(&path_to_state_file, &state)
        },
        Command::Forget { .. } => {
            if state_opt.is_none() {
                return Err(Error::new(ErrorKind::NotFound, format!("state file is not found {:?}", path_to_state_file)));
            }
            let mut state = state_opt.unwrap();
            if let Err(e) = forget_command(&config, &args, &mut state) { return Err(e) }
            write_state(&path_to_state_file, &state)
        },
        _ => {
            Err(Error::new(ErrorKind::Unsupported, format!("subcommand {:?} is not implemented yet", args)))
        }
    };
}
