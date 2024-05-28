use std::fs::File;
use std::io::Read;
use std::ops::Add;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
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
    command: Commands,

    //arbitrary_command: String,

    #[arg(long, num_args = 0, default_value_t = false)]
    dry_run: bool,

    #[arg(long, num_args = 1, default_value_t = 1)]
    verbose: u8, // 0 - don't output anything, 1 - print action, 2 - print debug

    #[arg(long, num_args = 1, required = false)]
    config: Option<PathBuf>,
}

#[derive(Debug, Subcommand)]
enum Commands {

    // ./config/dfm/config.toml must be crated only with `init` no other command cannot do this
    // because otherwise it will create an empty config file with no source dir and no target dir
    Init {
        #[arg(required = true)]
        path: PathBuf,
    },

    #[command(arg_required_else_help = true)]
    Apply {
        // empty means all, alright?
        paths: Option<Vec<PathBuf>>,
        invert_match: bool, // -v
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

    #[command(arg_required_else_help = true)]
    Add {
        paths: Option<Vec<PathBuf>>,

        #[arg(long, short, num_args = 0, default_value_t = false)]
        merge: bool,
    },

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
    }
}

#[derive(ValueEnum, Copy, Clone, Debug, PartialEq, Eq)]
enum IgnoreTargetType {
    Path,
    Pattern,
}

fn main() {
    let args = Args::parse();

    println!("dry run {}", args.dry_run);
    println!("verbose {}", args.verbose);

    match args.command {
        Commands::Init { path} => {
            println!("init with path {}", path.to_str().unwrap())
        }
        Commands::Add { paths, merge, .. } => {
            println!("add paths {:?}, merge {}", paths.to_owned(), merge);
        }
        _ => {
            println!("is not implemented")
        }
    }

    if !envmnt::exists("HOME") {
        eprintln!("Environment variable $HOME is not set")
    }

    // regular run
    // try to read ~/.config/fdm/config.toml, if not found
    // try to read ~/.dfm.toml, if not found
    // error, print help

    // init run
    // if config file exists, check that specified source dir exists and contains .dfm-source file,
    // if not create dir and file, print "already initialized"
    // exit
    // take a path, look for .fdm-source, if exists read the source dir path from it, if not exists
    // search in the path recursively until find a file .fdm-source, if not found
    // error, print help
    // create ~/.config/dfm/config.toml, if exists
    // do not override values, write source dir path to it, if source path has expanded $HOME prefix
    // write source path with a $HOME prepended and relative path.
    // Write $HOME to target path variable of the config
    // if --config is given then error, print help

    let path_to_config_file = envmnt::get_or_panic("HOME")
        .add("/")
        .add(CONFIG_FILE_NAME_IN_HOME);

    eprintln!("config file path {}", path_to_config_file);

    let config_file = File::open(path_to_config_file);
    let mut config_file_content = String::new();

    config_file.unwrap().read_to_string(&mut config_file_content).expect("TODO: panic message");

    let config: Config = toml::from_str(config_file_content.as_str()).unwrap();

    println!("-S {} -T {}", envmnt::expand(config.source_dir.as_str(), None), envmnt::expand(config.target_dir.as_str(), None));
    ()
}
