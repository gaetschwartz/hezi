#![deny(clippy::unwrap_used)]
mod nu;
mod styling;

use std::env;
use std::{io::Error, path::PathBuf};

use clap::Parser;

/// Search for a pattern in a file and display the lines that contain it.
use clap::{Args, Subcommand, ValueEnum};
use hezi::archive::{
    Archive, ArchiveCompression, ArchiveError, ArchiveType, Archived, CreateOptions, DataSource,
    ExtractOptions, ListOptions, SimpleLogger,
};
use nu::NuSetup;
use rayon::iter::{ParallelBridge, ParallelIterator};

use nu_protocol::Span;
use styling::{get_default_color, get_styles};

#[derive(Debug, Parser, Clone)]
#[command(name = "hezi", version, about = "A command line archive tool.", styles=get_styles())]
pub struct App {
    #[clap(flatten)]
    global_opts: GlobalOpts,

    #[clap(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand, Clone)]
enum Command {
    /// List the contents of an archive
    #[clap(alias = "l")]
    List {
        /// Path to the archive to list
        path: String,

        /// Detailed output
        #[clap(short, long)]
        long: bool,

        /// Password of the archive
        #[clap(short, long)]
        password: Option<String>,
    },
    /// Create an archive
    #[clap(alias = "c")]
    Create(CreateArgs),
    /// Extract an archive
    #[clap(alias = "x")]
    Extract {
        /// The path of the archive to extract
        path: String,

        /// The path to write to
        #[clap(short)]
        out: Option<String>,

        /// Overwrite existing files
        #[clap(short, long)]
        force: bool,

        /// A password to use
        #[clap(short, long)]
        password: Option<String>,
    },
}

#[derive(Debug, Args, Clone)]
struct CreateArgs {
    /// The path of the archive to create
    archive_path: String,

    /// Directory to use as the root of the archive
    #[clap(long, short)]
    directory: Option<PathBuf>,

    /// Files to add to the archive
    #[clap(name = "FILE", trailing_var_arg = true)]
    files: Option<Vec<PathBuf>>,

    /// Compression level
    #[clap(long, short)]
    level: Option<i32>,

    /// Force overwrite
    #[clap(long, short)]
    overwrite: bool,

    /// Compression algorithm
    #[clap(long, short)]
    compression: Option<ArchiveCompression>,

    /// Password
    #[clap(long, short)]
    password: Option<String>,
}

#[derive(Debug, Args, Clone)]
struct GlobalOpts {
    /// Color
    #[clap(long, value_enum, global = true, default_value_t = get_default_color())]
    color: Color,

    /// Verbosity level
    #[clap(long, short, global = true)]
    verbose: bool,

    /// Json output
    // #[clap(long, global = true)]
    #[clap(long, global = true)]
    json: bool,
}

#[derive(Clone, Debug, ValueEnum)]
pub enum Color {
    Always,
    Auto,
    Never,
}

fn main() {
    env_logger::init();
    let res = App::parse();

    // if res.global_opts.help {
    //     println!("help requested");
    //     // println!("{}", <App as clap::CommandFactory>::header(&res));
    //     _ = App::command().print_help();
    //     return;
    // }

    let nu = NuSetup::new(res.clone());
    match run(res, nu) {
        Ok(_) => {}
        Err(e) => {
            const RED: &str = "\x1b[31m";
            const RESET: &str = "\x1b[0m";
            const BOLD: &str = "\x1b[1m";
            eprintln!("{}An error occurred: \n\n{}{:?}{}", RED, BOLD, e, RESET);
            std::process::exit(1);
        }
    }
}

fn run(app: App, nu: NuSetup) -> Result<(), ShellError> {
    if app.global_opts.verbose {
        println!("command: {:#?}", app.command);
    }

    match app.command {
        Command::List { path, password, .. } => {
            let source = DataSource::file(path)?;

            let archive = Archive::of(source)?;

            let entries = archive.list(ListOptions {
                password,
                event_handler: nu.event_handler(),
            })?;

            nu.display_list(entries)?;

            Ok(())
        }
        Command::Create(create) => {
            let (archive_type, guessed_compression) =
                ArchiveType::guess_from_filename(&create.archive_path)?;
            let archive_compression =
                create
                    .compression
                    .or(guessed_compression)
                    .ok_or(ShellError::InvalidOption(
                        "could not determine compression algorithm".to_string(),
                    ))?;

            if let (Some(level), Some(range)) =
                (create.level, archive_compression.valid_level_range())
            {
                if !range.contains(&level) {
                    return Err(ShellError::InvalidArgument(format!(
                        "compression level must be between {} and {} but was {}",
                        range.start(),
                        range.end(),
                        level
                    )));
                }
            }

            if create.files.is_none() && create.directory.is_none() {
                return Err(ShellError::InvalidArgument(
                    "no files or directory specified".to_string(),
                ));
            }

            // let cwd = env::current_dir().expect("could not get current working directory");
            let source = create
                .directory
                .map_or_else(env::current_dir, |p| p.canonicalize())?;

            println!("Creating archive from {}", source.display());

            let files = if let Some(files) = create.files {
                files
                    .iter()
                    .map(|p| p.canonicalize())
                    .collect::<Result<_, _>>()?
            } else {
                walkdir::WalkDir::new(&source)
                    .into_iter()
                    .par_bridge()
                    .filter_map(|e| e.ok())
                    .map(|e| e.into_path())
                    .collect::<Vec<_>>()
            };

            let destination = std::path::PathBuf::from(create.archive_path);

            let options = CreateOptions {
                destination,
                password: create.password,
                files,
                overwrite: create.overwrite,
                source,
                archive_type,
                archive_compression: Some(archive_compression),
                include_hidden: true,
                event_handler: Box::new(SimpleLogger),
            };

            Archive::create(options)?;

            Ok(())
        }
        Command::Extract {
            path,
            out,
            force,
            password,
        } => {
            let path = PathBuf::from(path).canonicalize()?;
            let dest: PathBuf = out
                .map(PathBuf::from)
                .or(env::current_dir()
                    .ok()
                    .and_then(|cwd| path.file_stem().map(|p| cwd.join(p))))
                .ok_or(Error::new(
                    std::io::ErrorKind::Other,
                    "could not determine output path",
                ))?;

            println!("Extracting {} to {}", path.display(), dest.display());

            let datasource = DataSource::file(&path)?;

            let archive = Archive::of(datasource)?;

            let handler = nu.event_handler();
            archive.extract(ExtractOptions {
                destination: dest,
                password,
                files: None,
                overwrite: force,
                show_hidden: true,
                event_handler: handler,
            })?;

            Ok(())
        }
    }
}

#[inline]
pub fn empty_span() -> Span {
    Span::unknown()
}

pub trait OptExt<L, R> {
    fn both(self) -> Option<(L, R)>;
}

impl<L, R> OptExt<L, R> for (Option<L>, Option<R>) {
    fn both(self) -> Option<(L, R)> {
        match self {
            (Some(l), Some(r)) => Some((l, r)),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub enum ShellError {
    InvalidArgument(String),
    InvalidOption(String),
    ArchiveError(ArchiveError),
    Io(std::io::Error),
}

impl std::error::Error for ShellError {}

impl std::fmt::Display for ShellError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ShellError::InvalidArgument(s) => write!(f, "invalid argument: {}", s),
            ShellError::InvalidOption(s) => write!(f, "invalid option: {}", s),
            ShellError::ArchiveError(e) => write!(f, "archive error: {}", e),
            ShellError::Io(e) => write!(f, "io error: {}", e),
        }
    }
}

impl From<ArchiveError> for ShellError {
    fn from(e: ArchiveError) -> Self {
        ShellError::ArchiveError(e)
    }
}

impl From<std::io::Error> for ShellError {
    fn from(e: Error) -> Self {
        ShellError::Io(e)
    }
}
