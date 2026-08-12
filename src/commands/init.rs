use std::io::Write;
use std::path::PathBuf;
use std::{env, fs};

use log::{debug, info, trace, warn};

use super::{msg_dry_run, msg_nothing_to_do};
use crate::DfmError;
use dfm::*;
use microxdg::Xdg;

/// Typed, per-command arguments for `init` (built by the dispatcher).
pub struct InitArgs {
    pub path_to_source: PathBuf,
    pub path_to_target: Option<PathBuf>,
    pub dry_run: bool,
}

pub fn init_command(settings: &Settings, xdg: &Xdg, args: InitArgs) -> Result<(), DfmError> {
    let InitArgs {
        ref path_to_source,
        path_to_target: ref path_to_target_opt,
        dry_run,
    } = args;

    debug!("init with source path {:?}", path_to_source);
    debug!("init with target path {:?}", path_to_target_opt);

    #[allow(clippy::enum_variant_names)]
    enum InitTask {
        CreateSourceRootFile(PathBuf),
        CreateSourceIgnoreFile(),
        CreateStateFile(PathBuf, PathBuf, PathBuf),
        CreateDefaultConfigFile(PathBuf),
    }

    /// Human-readable description of a task, shown before it runs (and during
    /// --dry-run when it does not run).
    fn describe_init_task(task: &InitTask) -> String {
        match task {
            InitTask::CreateSourceRootFile(path) => format!("create source root file {:?}", path),
            InitTask::CreateSourceIgnoreFile() => "create source ignore file".to_string(),
            InitTask::CreateStateFile(path, _, _) => format!("create state file {:?}", path),
            InitTask::CreateDefaultConfigFile(path) => format!("create config file {:?}", path),
        }
    }

    if !path_to_source.exists() {
        info!("source dir {:?} does not exist, creating", path_to_source);
        if dry_run {
            warn!("dry-run — skipping source directory creation");
        } else {
            let actual_path = if path_to_source.is_absolute() {
                path_to_source.clone()
            } else {
                let current_dir = env::current_dir()?;
                PathBuf::from_iter(vec![current_dir, path_to_source.clone()])
            };
            fs::create_dir_all(actual_path)?;
        }
    }

    let mut tasks = vec![];

    let mut source_directory_pointer = path_to_source.join(".dfm_root");
    let source_dir_path = if source_directory_pointer.exists() {
        const MAX_POINTER_HOPS: u32 = 8;
        let mut hops: u32 = 0;
        loop {
            hops += 1;
            if hops > MAX_POINTER_HOPS {
                return Err(DfmError::InvalidData(format!(
                    "too many chained {} pointers (limit {}) starting from {:?}",
                    ".dfm_root", MAX_POINTER_HOPS, source_directory_pointer
                )));
            }
            let pointer_content = fs::read_to_string(&source_directory_pointer)
                .map_err(|e| io_err(&source_directory_pointer, e))?
                .trim()
                .to_owned();
            if pointer_content == "." {
                break;
            }
            // Only a plain directory name is a valid pointer target. Absolute
            // paths, `..`, `.`, empty strings and nested paths would escape
            // the source directory or recurse forever, so reject them.
            let mut components = std::path::Path::new(&pointer_content).components();
            let is_single_component = matches!(
                components.next(),
                Some(std::path::Component::Normal(_))
            ) && components.next().is_none();
            if !is_single_component {
                return Err(DfmError::InvalidData(format!(
                    "invalid {} pointer value {:?} in {:?}: expected a single directory name or \".\"",
                    ".dfm_root", pointer_content, source_directory_pointer
                )));
            }
            source_directory_pointer = source_directory_pointer.join(&pointer_content);
            trace!("searching .dfm_root in {:?}", source_directory_pointer);
        }
        fs::canonicalize(source_directory_pointer.parent().unwrap())?
    } else {
        tasks.push(InitTask::CreateSourceRootFile(
            path_to_source.join(".dfm_root"),
        ));
        if dry_run {
            // dry-run does not create the source dir, so it cannot be
            // canonicalized yet; build the absolute path instead.
            if path_to_source.is_absolute() {
                path_to_source.clone()
            } else {
                env::current_dir()?.join(path_to_source)
            }
        } else {
            fs::canonicalize(path_to_source)?
        }
    };

    debug!("using source directory {:?}", source_dir_path);

    let source_ignore_file_path = calc_source_ignore_file(&source_dir_path);
    let source_ignore_regex = load_ignore_regex(&source_ignore_file_path)?;

    if !source_ignore_regex.matches(".dfm_root").matched_any() {
        debug!("source ignore file will be extended with \\.dfm_root");
        tasks.push(InitTask::CreateSourceIgnoreFile());
    }

    let home_dir_path = match get_home_path() {
        Some(p) => p,
        None => {
            return Err(DfmError::InvalidData(
                "failed to define home directory".into(),
            ));
        }
    };

    let target_abs_path = if let Some(path_to_target) = path_to_target_opt {
        fs::canonicalize(path_to_target)?
    } else {
        home_dir_path
    };

    debug!("using target directory {:?}", target_abs_path);
    let state_file_path = calc_state_file_path(xdg)?;
    if state_file_path.exists() {
        debug!("state file already exists, no need to create");
    } else {
        tasks.push(InitTask::CreateStateFile(
            state_file_path.clone(),
            target_abs_path,
            source_dir_path.clone(),
        ));
    }

    let target_config_file_path = calc_config_file_path(xdg);
    if let Ok(config_file) = target_config_file_path
        && !config_file.exists()
    {
        tasks.push(InitTask::CreateDefaultConfigFile(config_file));
    }

    if tasks.is_empty() {
        info!("{}", msg_nothing_to_do());
        return Ok(());
    }

    if dry_run {
        info!("{}", msg_dry_run());
    }

    debug!("::init procedure begins, {} tasks", tasks.len());

    for task in tasks {
        // Print what each task would do even under --dry-run.
        info!("{}", describe_init_task(&task));
        if dry_run {
            continue;
        }
        match task {
            InitTask::CreateSourceRootFile(path) => {
                fs::create_dir_all(path.parent().unwrap())
                    .map_err(|e| io_err(path.parent().unwrap(), e))?;
                fs::write(&path, ".").map_err(|e| io_err(&path, e))?;
            }
            InitTask::CreateSourceIgnoreFile() => {
                let ignore_file_records = vec![
                    ".dfm_root",
                    ".git",
                    ".dfm_ignore_source",
                    ".dfm_ignore_target",
                    ".current_merge",
                    ".current_diff",
                ];

                fs::create_dir_all(source_ignore_file_path.parent().unwrap())
                    .map_err(|e| io_err(source_ignore_file_path.parent().unwrap(), e))?;
                let mut source_ignore_file = open_or_create_file(&source_ignore_file_path)?;

                for ignore_file_record in ignore_file_records {
                    if let Err(e) =
                        writeln!(source_ignore_file, "{}", regex::escape(ignore_file_record))
                    {
                        return Err(io_err(&source_ignore_file_path, e));
                    } else {
                        debug!("source ignore file: added record {}", ignore_file_record);
                    }
                }
            }
            InitTask::CreateStateFile(path, target_dir, source_dir) => {
                fs::create_dir_all(path.parent().unwrap())
                    .map_err(|e| io_err(path.parent().unwrap(), e))?;

                let empty_state = StateObject::new(target_dir, source_dir);
                write_state(&path, &empty_state)?;
            }
            InitTask::CreateDefaultConfigFile(path) => {
                let config_file = Config::from_settings(settings);
                write_config(&path, &config_file)?;
            }
        }
    }

    Ok(())
}
