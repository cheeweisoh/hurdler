use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "hurdler", version, about)]
pub struct Cli {
    #[arg(
        long,
        short,
        global = true,
        value_name = "FILE",
        help = "Config file to use (default: $XDG_CONFIG_HOME/hurdler/config.toml)"
    )]
    pub config: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand)]
pub enum Command {
    #[command(about = "Create or update the config file")]
    Init,
}
