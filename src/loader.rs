use std::{fs, path::Path};

use thiserror::Error;

use crate::{
    GraphDocument, NodeDefinition, ValidationReport, validate_graph_document,
    validate_node_definition,
};

#[derive(Debug, Error)]
pub enum LoadError {
    #[error("failed to read {path}: {source}")]
    Read {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse {path}: {source}")]
    Parse {
        path: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("invalid {path}: {source}")]
    Invalid {
        path: String,
        #[source]
        source: ValidationReport,
    },
}

pub fn load_node_definition(path: impl AsRef<Path>) -> Result<NodeDefinition, LoadError> {
    let path = path.as_ref();
    let display_path = path.display().to_string();
    let bytes = fs::read(path).map_err(|source| LoadError::Read {
        path: display_path.clone(),
        source,
    })?;
    let definition: NodeDefinition =
        serde_json::from_slice(&bytes).map_err(|source| LoadError::Parse {
            path: display_path.clone(),
            source,
        })?;

    validate_node_definition(&definition).map_err(|source| LoadError::Invalid {
        path: display_path,
        source,
    })?;

    Ok(definition)
}

pub fn load_graph_document(path: impl AsRef<Path>) -> Result<GraphDocument, LoadError> {
    let path = path.as_ref();
    let display_path = path.display().to_string();
    let bytes = fs::read(path).map_err(|source| LoadError::Read {
        path: display_path.clone(),
        source,
    })?;
    let graph: GraphDocument =
        serde_json::from_slice(&bytes).map_err(|source| LoadError::Parse {
            path: display_path.clone(),
            source,
        })?;

    validate_graph_document(&graph).map_err(|source| LoadError::Invalid {
        path: display_path,
        source,
    })?;

    Ok(graph)
}
