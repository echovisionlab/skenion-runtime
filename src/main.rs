use std::path::PathBuf;

use clap::{Parser, Subcommand};
use skenion_runtime::{load_graph_document, load_node_definition};

#[derive(Debug, Parser)]
#[command(author, version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Validate a Skenion Node Definition Manifest v0.1 JSON file.
    ValidateNode {
        /// Path to the node definition manifest.
        path: PathBuf,
    },
    /// Validate a Skenion Graph Document v0.1 JSON file.
    ValidateGraph {
        /// Path to the graph document.
        path: PathBuf,
    },
}

fn main() {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::ValidateNode { path } => load_node_definition(&path).map(|definition| {
            println!(
                "valid node definition: {} {}",
                definition.id, definition.version
            );
        }),
        Command::ValidateGraph { path } => load_graph_document(&path).map(|graph| {
            println!("valid graph: {} {}", graph.id, graph.revision);
        }),
    };

    if let Err(error) = result {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
