#![deny(clippy::unwrap_used)]
mod from;
mod plugin;

use crate::plugin::ArchivePlugin;
use nu_plugin::{serve_plugin, MsgPackSerializer};

fn main() {
    serve_plugin(&ArchivePlugin::new(), MsgPackSerializer)
}
