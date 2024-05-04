use std::{path::PathBuf, vec};

use nu_plugin::{EvaluatedCall, Plugin};
use nu_protocol::{
    CustomValue, IntoPipelineData, LabeledError, Record, Signature, SyntaxShape, Type, Value,
};

use hezi::archive::{
    Archive, ArchiveCompression, ArchiveType, Archived, CreateOptions, DataSource, ExtractOptions,
    ListOptions, OpenOptions, SimpleLogger,
};


use crate::from::from_xx_archive;

pub struct ArchivePlugin;

fn archive_list_record_type() -> Type {
    Type::Table(vec![
        ("name".into(), Type::String),
        ("size".into(), Type::Filesize),
        ("compressed_size".into(), Type::Filesize),
        ("type".into(), Type::String),
        ("last_modified".into(), Type::Date),
        ("compression".into(), Type::String),
    ])
}

#[allow(clippy::unwrap_used)]
fn from_x_signature(name: &str) -> Signature {
    Signature::build(format!("from {}", name.trim()))
        .usage(format!("Lists a .{} archive.", name))
        .input_output_type(Type::String, archive_list_record_type())
        .category(nu_protocol::Category::Conversions)
}

// const ARCHIVE_EXTENSIONS: &[&str] = &[
//     "zip", "tar", "tar.gz", "tar.xz", "tar.bz2", "tar.zst", "7z", "7zip", "tar.lz", "tgz",
//     "tar.lzma", "tar.lzo", "tar.sz", "tar.z", "rar", "tar.lz4", "tar.gz2", "tar.bz", "tar.bz2",
// ];
const ARCHIVE_EXTENSIONS: &[&str] = &[
    "zip", // Zip
    "tar", // Tar (no compression)
    "tar.gz", "tgz", // Tar (gzip)
    "tar.xz", "txz", // Tar (xz)
    "tar.bz2", "tbz2", "tbz", // Tar (bzip2)
    "tar.zst", "tzst", "tzs", "tar.zstd", // Tar (zstd)
    "tar.lzma", "tlzma", "tlz", // Tar (lzma)
    "7z", "7zip", // 7z
];

fn archive_create_record_type() -> Type {
    Type::Table(vec![
        ("path".into(), Type::String),
        ("total_size".into(), Type::Filesize),
        ("compressed_size".into(), Type::Filesize),
    ])
}

impl Plugin for ArchivePlugin {
    fn commands(&self) -> Vec<Box<dyn nu_plugin::PluginCommand<Plugin = Self>>> {
        let mut commands: Vec<Box<dyn nu_plugin::PluginCommand<Plugin = Self>>> = vec![
            Box::new(ArchiveList),
            Box::new(ArchiveMetadata),
            Box::new(ArchiveCreate),
            Box::new(ArchiveExtract),
            Box::new(ArchiveOpen),
        ];
        commands.extend(ARCHIVE_EXTENSIONS.iter().map(|ext| {
            Box::new(FromArchive::new(ext)) as Box<dyn nu_plugin::PluginCommand<Plugin = Self>>
        }));

        commands
    }
}

struct FromArchive {
    ext: String,
    name: String,
    usage: String,
}

impl FromArchive {
    pub fn new<T: ToString>(ext: T) -> Self {
        let ext = ext.to_string();
        Self {
            name: format!("from {}", ext),
            usage: format!("List a .{} archive", ext),
            ext,
        }
    }
}

impl nu_plugin::PluginCommand for FromArchive {
    fn name(&self) -> &str {
        &self.name
    }

    fn usage(&self) -> &str {
        &self.usage
    }

    type Plugin = ArchivePlugin;

    fn signature(&self) -> Signature {
        from_x_signature(&self.ext)
    }

    fn run(
        &self,
        _plugin: &Self::Plugin,
        _engine: &nu_plugin::EngineInterface,
        call: &EvaluatedCall,
        input: nu_protocol::PipelineData,
    ) -> Result<nu_protocol::PipelineData, nu_protocol::LabeledError> {
        from_xx_archive(&self.ext, call, &input.into_value(call.head))
            .map(|v| v.into_pipeline_data())
    }
}

struct ArchiveOpen;

impl nu_plugin::PluginCommand for ArchiveOpen {
    fn name(&self) -> &str {
        "archive open"
    }

    fn usage(&self) -> &str {
        "Open an archive"
    }

    type Plugin = ArchivePlugin;

    fn signature(&self) -> nu_protocol::Signature {
        Signature::build("archive open")
            .usage("Open an archive")
            .input_output_types(vec![(Type::String, Type::Nothing)])
            .required("path", SyntaxShape::String, "path to archive to open")
            .named(
                "password",
                SyntaxShape::String,
                "password to use for extraction",
                Some('p'),
            )
    }

    fn run(
        &self,
        _plugin: &Self::Plugin,
        _engine: &nu_plugin::EngineInterface,
        call: &EvaluatedCall,
        input: nu_protocol::PipelineData,
    ) -> Result<nu_protocol::PipelineData, nu_protocol::LabeledError> {
        let archive_path = input.into_value(call.head).coerce_into_string()?;
        let path = call
            .nth(0)
            .map(|v| v.coerce_into_string())
            .unwrap_or(Ok(archive_path.clone()))
            .map(PathBuf::from)?;
        // make it relative to the cwd
        let current_dir = std::env::current_dir()
            .map_err(|e| LabeledError::new(format!("could not get current directory: {}", e)))?;

        let path = path
            .strip_prefix(&current_dir)
            .map_err(|_e| LabeledError::new("invalid path"))?;

        let password = call.get_flag::<String>("password")?;

        let datasource = DataSource::file(&archive_path)
            .map_err(|_e| LabeledError::new("could not open file"))?;

        let archive =
            Archive::of(datasource).map_err(|_e| LabeledError::new("could not open archive"))?;

        eprintln!(
            "Opening file {} in archive {}",
            path.display(),
            archive_path
        );

        archive
            .open(OpenOptions {
                path: path.into(),
                dest: Box::new(std::io::stderr()),
                password,
            })
            .map_err(|_e| LabeledError::new("could not open archive"))?;

        Ok(Value::nothing(call.head).into_pipeline_data())
    }
}

struct ArchiveExtract;

impl nu_plugin::PluginCommand for ArchiveExtract {
    fn name(&self) -> &str {
        "archive extract"
    }

    fn usage(&self) -> &str {
        "Extract an archive"
    }

    type Plugin = ArchivePlugin;

    fn run(
        &self,
        _plugin: &Self::Plugin,
        _engine: &nu_plugin::EngineInterface,
        call: &EvaluatedCall,
        input: nu_protocol::PipelineData,
    ) -> Result<nu_protocol::PipelineData, nu_protocol::LabeledError> {
        let path = input.into_value(call.head).coerce_into_string()?;
        let dest = call
            .nth(0)
            .map(|v| v.coerce_into_string())
            .unwrap_or(Ok(".".to_string()))?;

        let datasource =
            DataSource::file(&path).map_err(|_e| LabeledError::new("could not open file"))?;

        let archive =
            Archive::of(datasource).map_err(|_e| LabeledError::new("could not open archive"))?;

        archive
            .extract(ExtractOptions {
                destination: dest.into(),
                password: call.get_flag::<String>("password")?,
                files: call.get_flag::<Vec<String>>("files")?,
                overwrite: call.has_flag("overwrite")?,
                show_hidden: true,
                event_handler: Box::new(SimpleLogger),
            })
            .map_err(|_e| LabeledError::new("could not extract archive"))?;

        Ok(Value::nothing(call.head).into_pipeline_data())
    }

    fn signature(&self) -> Signature {
        Signature::build("archive extract")
            .usage("Extract an archive")
            .input_output_types(vec![
                (Type::String, Type::Nothing),
                (Type::Nothing, Type::Nothing),
            ])
            .optional("archive", SyntaxShape::String, "archive to extract")
            .required(
                "destination",
                SyntaxShape::String,
                "destination to extract to",
            )
            .named(
                "password",
                SyntaxShape::String,
                "password to use for extraction",
                Some('p'),
            )
            .named(
                "files",
                SyntaxShape::List(Box::new(SyntaxShape::String)),
                "files to extract",
                Some('F'),
            )
            .switch("silent", "do not print anything", Some('s'))
            .switch("overwrite", "overwrite existing files", Some('f'))
    }
}

struct ArchiveCreate;

impl nu_plugin::PluginCommand for ArchiveCreate {
    fn name(&self) -> &str {
        "archive create"
    }

    fn usage(&self) -> &str {
        "Create an archive"
    }

    type Plugin = ArchivePlugin;

    fn signature(&self) -> nu_protocol::Signature {
        Signature::build("archive create")
            .usage("Create an archive")
            .input_output_types(vec![
                (
                    Type::List(Box::new(Type::String)),
                    archive_create_record_type(),
                ),
                (Type::Nothing, archive_create_record_type()),
            ])
            .required(
                "destination",
                SyntaxShape::String,
                "destination to create archive at",
            )
            .optional(
                "files",
                SyntaxShape::OneOf(vec![
                    SyntaxShape::List(Box::new(SyntaxShape::String)),
                    SyntaxShape::String,
                ]),
                "files to add to archive",
            )
            .named(
                "password",
                SyntaxShape::String,
                "password to use for extraction",
                Some('p'),
            )
            .named(
                "source",
                SyntaxShape::String,
                "source directory to create archive from",
                Some('s'),
            )
            .named(
                "compression",
                SyntaxShape::String,
                "compression method to use",
                Some('c'),
            )
            .switch("overwrite", "overwrite existing files", Some('f'))
    }

    fn run(
        &self,
        _plugin: &Self::Plugin,
        _engine: &nu_plugin::EngineInterface,
        call: &EvaluatedCall,
        input: nu_protocol::PipelineData,
    ) -> Result<nu_protocol::PipelineData, nu_protocol::LabeledError> {
        let files = if let Some(files) = call.positional.get(1) {
            files.clone()
        } else {
            input.into_value(call.head)
        };
        let files_list = match files {
            Value::List { vals, .. } => vals
                .iter()
                .map(|v| v.coerce_string())
                .collect::<Result<_, _>>()
                .map_err(|_e| LabeledError::new("invalid input"))?,
            Value::String { val, .. } => vec![val.to_string()],
            _ => {
                return Err(LabeledError::new("invalid input"));
            }
        };

        let resolved_files = files_list
            .iter()
            .flat_map(|f| glob::glob_with(f, glob::MatchOptions::new()))
            .flatten()
            .flatten()
            .flat_map(|f| f.canonicalize())
            .collect::<Vec<_>>();

        let dest = if let Some(p) = call.positional.first() {
            p.coerce_string()?
        } else {
            // get deepest common directory
            compute_deepest_common_directory(&resolved_files)
                .and_then(|c| c.last().cloned())
                .map(|l| PathBuf::from(".").join(l).with_extension("zip"))
                .unwrap_or_else(|| PathBuf::from("archive.zip"))
                .to_string_lossy()
                .to_string()
        };

        let password = call.get_flag::<String>("password")?;

        let overwrite = call.has_flag("overwrite")?;

        let source_path = if let Some(source) = call.get_flag::<String>("source")? {
            PathBuf::from(source)
                .canonicalize()
                .map_err(|_e| LabeledError::new("invalid source path"))?
                .to_string_lossy()
                .to_string()
        } else {
            std::env::current_dir()
                .and_then(|p| p.canonicalize())
                .map_err(|_e| LabeledError::new("could not get current directory"))?
                .to_string_lossy()
                .to_string()
        };

        let compression_arg = call.get_flag::<ArchiveCompression>("compression")?;

        let (archive_type, guessed_compression) = ArchiveType::guess_from_filename(&dest)
            .map_err(|_e| LabeledError::new("could not guess archive type"))?;

        let options = CreateOptions {
            destination: PathBuf::from(dest),
            password,
            files: resolved_files,
            overwrite,
            source: PathBuf::from(source_path),
            archive_type,
            archive_compression: compression_arg.or(guessed_compression),
            include_hidden: true,
            event_handler: Box::new(SimpleLogger),
        };

        let res =
            Archive::create(options).map_err(|_e| LabeledError::new("could not create archive"))?;

        Ok(Value::Record {
            val: Record::from_iter(vec![
                (
                    "path".to_string(),
                    Value::string(res.path.to_string_lossy().to_string(), call.head),
                ),
                (
                    "total_size".to_string(),
                    Value::filesize(res.total_size as i64, call.head),
                ),
                (
                    "compressed_size".to_string(),
                    Value::filesize(res.compressed_size as i64, call.head),
                ),
            ])
            .into(),
            internal_span: call.head,
        }
        .into_pipeline_data())
    }
}

struct ArchiveMetadata;

impl nu_plugin::PluginCommand for ArchiveMetadata {
    fn name(&self) -> &str {
        "archive metadata"
    }

    fn usage(&self) -> &str {
        "Get metadata of an archive"
    }

    type Plugin = ArchivePlugin;

    fn signature(&self) -> nu_protocol::Signature {
        Signature::build("archive metadata")
            .usage("Get metadata of an archive")
            .input_output_types(vec![
                (Type::String, Type::Custom("archive_metadata".to_string())),
                (Type::Nothing, Type::Custom("archive_metadata".to_string())),
            ])
            .optional(
                "archive",
                SyntaxShape::String,
                "archive to get metadata from",
            )
    }

    fn run(
        &self,
        _plugin: &Self::Plugin,
        _engine: &nu_plugin::EngineInterface,
        call: &EvaluatedCall,
        input: nu_protocol::PipelineData,
    ) -> Result<nu_protocol::PipelineData, nu_protocol::LabeledError> {
        let path = if let Some(path) = call.positional.first() {
            path.coerce_string()?
        } else {
            input.into_value(call.head).coerce_into_string()?
        };
        let datasource =
            DataSource::file(&path).map_err(|_e| LabeledError::new("could not open file"))?;

        let archive =
            Archive::of(datasource).map_err(|_e| LabeledError::new("could not open archive"))?;

        let metadata = archive
            .metadata()
            .map_err(|_e| LabeledError::new("could not get metadata"))?;

        Ok(Value::custom(Box::new(metadata), call.head).into_pipeline_data())
    }
}

struct ArchiveList;

impl nu_plugin::PluginCommand for ArchiveList {
    fn name(&self) -> &str {
        "archive list"
    }

    fn usage(&self) -> &str {
        "List the contents of an archive"
    }

    type Plugin = ArchivePlugin;

    fn signature(&self) -> nu_protocol::Signature {
        Signature::build("archive list")
            .usage("List the contents of an archive")
            .input_output_types(vec![
                (Type::String, archive_list_record_type()),
                (Type::Nothing, archive_list_record_type()),
            ])
            .optional("archive", SyntaxShape::String, "archive to list")
    }

    fn run(
        &self,
        _plugin: &Self::Plugin,
        _engine: &nu_plugin::EngineInterface,
        call: &EvaluatedCall,
        input: nu_protocol::PipelineData,
    ) -> Result<nu_protocol::PipelineData, nu_protocol::LabeledError> {
        let path = if let Some(path) = call.positional.first() {
            path.coerce_string()?
        } else {
            input.into_value(call.head).coerce_into_string()?
        };
        let datasource =
            DataSource::file(&path).map_err(|_e| LabeledError::new("could not open file"))?;

        let archive =
            Archive::of(datasource).map_err(|_e| LabeledError::new("could not open archive"))?;

        let list = archive.list(ListOptions::default());

        Ok(Value::List {
            vals: list
                .map_err(|_e| LabeledError::new("could not list archive"))
                .and_then(|f| {
                    f.iter()
                        .map(|f| f.to_base_value(call.head))
                        .collect::<Result<Vec<_>, _>>()
                        .map_err(|_e| LabeledError::new("could not convert archive entry"))
                })?,
            internal_span: call.head,
        }
        .into_pipeline_data())
    }
}

fn compute_deepest_common_directory(paths: &[PathBuf]) -> Option<Vec<std::path::Component<'_>>> {
    paths
        .iter()
        .map(|f| f.components().collect::<Vec<_>>())
        .reduce(|a, b| {
            a.iter()
                .zip(b.iter())
                .take_while(|(a, b)| a == b)
                .map(|(a, _)| *a)
                .collect::<Vec<_>>()
        })
}

impl ArchivePlugin {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ArchivePlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn test_deeper_common_directory1() {
        let paths = vec![
            std::path::PathBuf::from("/home/user/file1"),
            std::path::PathBuf::from("/home/user/file2"),
            std::path::PathBuf::from("/home/user/file3"),
        ];

        let res = super::compute_deepest_common_directory(&paths);

        assert_eq!(
            res,
            Some(vec![
                std::path::Component::RootDir,
                std::path::Component::Normal("home".as_ref()),
                std::path::Component::Normal("user".as_ref())
            ])
        );
    }

    #[test]
    fn test_deeper_common_directory2() {
        let paths = vec![
            std::path::PathBuf::from("/home/download/file1"),
            std::path::PathBuf::from("/home/documents/file2"),
            std::path::PathBuf::from("/home/file3"),
        ];

        let res = super::compute_deepest_common_directory(&paths);

        assert_eq!(
            res,
            Some(vec![
                std::path::Component::RootDir,
                std::path::Component::Normal("home".as_ref()),
            ])
        );
    }
}
