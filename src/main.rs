use std::path::PathBuf;
use clap::{Parser, Subcommand, ValueEnum};

// opts https://docs.rs/clap/latest/clap/_derive/_cookbook/git_derive/index.html 
// toml  https://docs.rs/toml_edit/latest/toml_edit/

// $ cat ~/.cellar/.dfm-root
// ./dotfiles

// $ cat ~/.cellar/dotfiles
// .

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

    Status, // --all, --managed, --unmanaged (default), --ignored, --list-ignore-patterns --list-vain-ignored-patterns, --unapplyed, --different

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
    // .ignored-paths
    // .ignored-patterns
    Ignore {
        paths: Option<Vec<PathBuf>>,
        pattern: String,
        what: IgnoreTargetType,
    },
}

#[derive(ValueEnum, Copy, Clone, Debug, PartialEq, Eq)]
enum IgnoreTargetType {
    Paths,
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
    ()
}
