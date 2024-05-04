use std::io::Write;

use byte_unit::{Byte, UnitType};
use hezi::archive::{ArchiveError, ArchiveEvent, EventHandler, SkipReason};
/// Search for a pattern in a file and display the lines that contain it.
use nu_color_config::StyleComputer;

use nu_protocol::{
    engine::{EngineState, Stack},
    Config, CustomValue, Span, TableIndexMode, Value,
};
use nu_table::{JustTable, TableOpts, TableTheme, UnstructuredTable};

use crate::{
    styling::{main_theme, no_color_theme},
    App, Color,
};

#[derive(Clone)]
pub struct NuSetup {
    engine_state: EngineState,
    stack: Stack,
    #[allow(dead_code)]
    app: App,
}

impl NuSetup {
    pub fn new(app: App) -> NuSetup {
        let mut nu_cfg = Config::default();
        match app.global_opts.color {
            Color::Always | Color::Auto => {
                nu_cfg.color_config = main_theme();
            }
            Color::Never => {
                nu_cfg.color_config = no_color_theme();
            }
        }
        nu_cfg.table_index_mode = TableIndexMode::Auto;
        nu_cfg.table_show_empty = true;
        let mut engine_state = EngineState::default();
        engine_state.set_config(nu_cfg);
        let stack = Stack::default();

        Self {
            engine_state,
            stack,
            app,
        }
    }

    #[inline]
    pub fn style_computer(&self) -> StyleComputer {
        StyleComputer::from_config(&self.engine_state, &self.stack)
    }

    #[inline]
    pub fn cfg(&self) -> &Config {
        &self.engine_state.config
    }

    #[inline]
    pub fn term_size(&self) -> (usize, usize) {
        terminal_size::terminal_size()
            .map(|(w, h)| (w.0 as usize, h.0 as usize))
            .unwrap_or((80, 24))
    }

    pub fn draw_list_table(&self, list: Vec<Value>) {
        let (w, _) = self.term_size();
        let drawn = JustTable::table(
            &list,
            TableOpts::new(
                self.cfg(),
                &self.style_computer(),
                None,
                Span::unknown(),
                w,
                (self.cfg().table_indent.left, self.cfg().table_indent.right),
                self.cfg().table_mode,
                0,
                false,
            ),
        );
        match drawn {
            Ok(Some(s)) => {
                _ = std::io::stdout().write_all(s.as_bytes());
            }
            Ok(None) => {
                // nothing to draw
            }
            Err(e) => {
                eprintln!(
                    "Failed to draw table, falling back to unstructured table: {}",
                    e
                );
                let table = UnstructuredTable::new(Value::list(list, Span::unknown()), self.cfg());
                _ = std::io::stdout().write_all(
                    table
                        .draw(&self.style_computer(), &TableTheme::rounded(), (0, 0))
                        .as_bytes(),
                );
            }
        }
    }

    pub fn display_list<V: CustomValue + serde::Serialize>(
        &self,
        list: Vec<V>,
    ) -> Result<(), ArchiveError> {
        if self.app.global_opts.json {
            println!("{}", serde_json::to_string(&list)?);
            return Ok(());
        }

        let list = list
            .into_iter()
            .map(|v| v.to_base_value(Span::unknown()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| ArchiveError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;
        self.draw_list_table(list);

        Ok(())
    }

    pub(crate) fn event_handler<'a>(&'a self) -> Box<dyn EventHandler + 'a> {
        Box::new(self)
    }
}

impl AsRef<NuSetup> for NuSetup {
    fn as_ref(&self) -> &NuSetup {
        self
    }
}

impl<'a> EventHandler for &'a NuSetup {
    fn handle(&self, event: ArchiveEvent) {
        match event {
            ArchiveEvent::Extracting(name, size) => {
                if let Some(size) = size {
                    println!(
                        "Extracting {} ({})",
                        name,
                        Byte::from(size).get_appropriate_unit(UnitType::Both)
                    );
                } else {
                    println!("Extracting {}", name);
                }
            }
            ArchiveEvent::DoneExtracting(name, path) => {
                println!("Done extracting {} to {}", name, path);
            }
            ArchiveEvent::FailedToReadEntry(name, e) => {
                println!("Failed to read entry {}: {}", name, e);
            }
            ArchiveEvent::Created(name, fstype) => {
                println!("Created {}: {}", fstype, name);
            }
            ArchiveEvent::Skipped(name, reason) => match reason {
                SkipReason::Hidden => println!("Skipped hidden file {}", name),
                SkipReason::NotInFiles => println!("Skipped file {} not in files", name),
                SkipReason::AlreadyExists => println!("Skipped file {} already exists", name),
                SkipReason::UnknownType => println!("Skipped file {} with unknown type", name),
            },
            ArchiveEvent::Log(msg) => println!("{}", msg),
        }
    }
}

impl From<App> for NuSetup {
    fn from(app: App) -> Self {
        Self::new(app)
    }
}
