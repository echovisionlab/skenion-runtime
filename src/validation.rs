use std::{collections::HashMap, error::Error, fmt};

use crate::{DataFlow, DataType, GraphDocument, NodeDefinition, Port, PortDirection};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
    pub message: String,
}

impl ValidationError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationReport {
    errors: Vec<ValidationError>,
}

impl ValidationReport {
    fn new(errors: Vec<ValidationError>) -> Self {
        Self { errors }
    }

    pub fn errors(&self) -> &[ValidationError] {
        &self.errors
    }
}

impl fmt::Display for ValidationReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let messages = self
            .errors
            .iter()
            .map(|error| error.message.as_str())
            .collect::<Vec<_>>()
            .join("; ");
        write!(f, "{messages}")
    }
}

impl Error for ValidationReport {}

fn duplicate_errors<'a>(
    values: impl Iterator<Item = &'a str>,
    label: &str,
) -> Vec<ValidationError> {
    let mut counts: HashMap<&str, usize> = HashMap::new();
    let mut errors = Vec::new();

    for value in values {
        let count = counts.entry(value).or_default();
        *count += 1;
        if *count == 2 {
            errors.push(ValidationError::new(format!("duplicate {label}: {value}")));
        }
    }

    errors
}

fn validate_ports(owner_id: &str, ports: &[Port]) -> Vec<ValidationError> {
    let mut errors = duplicate_errors(
        ports.iter().map(|port| port.id.as_str()),
        &format!("port id on {owner_id}"),
    );

    for port in ports {
        if port.direction != PortDirection::Input && port.activation.is_some() {
            errors.push(ValidationError::new(format!(
                "output port {owner_id}.{} must not declare activation",
                port.id
            )));
        }
    }

    errors
}

fn format_accepts(target_type: &DataType, source_type: &DataType) -> bool {
    let Some(target_format) = &target_type.format else {
        return true;
    };
    let Some(source_format) = &source_type.format else {
        return true;
    };

    let target_formats = target_format.values();
    source_format
        .values()
        .iter()
        .all(|format| target_formats.contains(format))
}

fn compatible_types(source_type: &DataType, target_type: &DataType) -> bool {
    source_type.flow == target_type.flow
        && source_type.data_kind == target_type.data_kind
        && format_accepts(target_type, source_type)
}

fn type_label(data_type: &DataType) -> String {
    let flow = match data_type.flow {
        DataFlow::Value => "value",
        DataFlow::Event => "event",
        DataFlow::Signal => "signal",
        DataFlow::Stream => "stream",
        DataFlow::Resource => "resource",
    };
    format!("{flow}<{}>", data_type.data_kind)
}

pub fn validate_node_definition(definition: &NodeDefinition) -> Result<(), ValidationReport> {
    let mut errors = Vec::new();

    if definition.schema != "skenion.node.definition" {
        errors.push(ValidationError::new(format!(
            "expected schema skenion.node.definition, found {}",
            definition.schema
        )));
    }
    if definition.schema_version != "0.1.0" {
        errors.push(ValidationError::new(format!(
            "expected schemaVersion 0.1.0, found {}",
            definition.schema_version
        )));
    }

    errors.extend(validate_ports(&definition.id, &definition.ports));

    for permission in &definition.permissions {
        errors.push(ValidationError::new(format!(
            "unsupported permission: {permission}"
        )));
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(ValidationReport::new(errors))
    }
}

pub fn validate_graph_document(graph: &GraphDocument) -> Result<(), ValidationReport> {
    let mut errors = Vec::new();

    if graph.schema != "skenion.graph" {
        errors.push(ValidationError::new(format!(
            "expected schema skenion.graph, found {}",
            graph.schema
        )));
    }
    if graph.schema_version != "0.1.0" {
        errors.push(ValidationError::new(format!(
            "expected schemaVersion 0.1.0, found {}",
            graph.schema_version
        )));
    }

    errors.extend(duplicate_errors(
        graph.nodes.iter().map(|node| node.id.as_str()),
        "node id",
    ));

    let mut ports: HashMap<(&str, &str), &Port> = HashMap::new();
    for node in &graph.nodes {
        errors.extend(validate_ports(&node.id, &node.ports));
        for port in &node.ports {
            ports.insert((node.id.as_str(), port.id.as_str()), port);
        }
    }

    for edge in &graph.edges {
        let from_key = (edge.from.node.as_str(), edge.from.port.as_str());
        let to_key = (edge.to.node.as_str(), edge.to.port.as_str());
        let from = ports.get(&from_key);
        let to = ports.get(&to_key);

        if from.is_none() {
            errors.push(ValidationError::new(format!(
                "edge references missing source port {}:{}",
                edge.from.node, edge.from.port
            )));
        }
        if to.is_none() {
            errors.push(ValidationError::new(format!(
                "edge references missing target port {}:{}",
                edge.to.node, edge.to.port
            )));
        }

        let (Some(from), Some(to)) = (from, to) else {
            continue;
        };

        if from.direction != PortDirection::Output {
            errors.push(ValidationError::new(format!(
                "edge source {}:{} is not an output port",
                edge.from.node, edge.from.port
            )));
        }
        if to.direction != PortDirection::Input {
            errors.push(ValidationError::new(format!(
                "edge target {}:{} is not an input port",
                edge.to.node, edge.to.port
            )));
        }
        if !compatible_types(&from.data_type, &to.data_type) {
            errors.push(ValidationError::new(format!(
                "incompatible edge {}:{} {} -> {}:{} {}",
                edge.from.node,
                edge.from.port,
                type_label(&from.data_type),
                edge.to.node,
                edge.to.port,
                type_label(&to.data_type)
            )));
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(ValidationReport::new(errors))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node_definition(json: &str) -> NodeDefinition {
        serde_json::from_str(json).expect("node definition fixture should parse")
    }

    fn graph_document(json: &str) -> GraphDocument {
        serde_json::from_str(json).expect("graph fixture should parse")
    }

    fn error_text(report: &ValidationReport) -> String {
        report.to_string()
    }

    #[test]
    fn accepts_valid_script_node_manifest() {
        let definition = node_definition(
            r#"{
              "schema": "skenion.node.definition",
              "schemaVersion": "0.1.0",
              "id": "script.brightness",
              "version": "0.1.0",
              "displayName": "Brightness",
              "category": "Script",
              "scriptApiVersion": "0.1.0",
              "bundleHash": "sha256:0000",
              "ports": [
                {
                  "id": "enabled",
                  "direction": "input",
                  "type": { "flow": "value", "dataKind": "boolean" },
                  "activation": "latched",
                  "default": true
                },
                {
                  "id": "pulse",
                  "direction": "output",
                  "type": { "flow": "event", "dataKind": "bang" }
                }
              ],
              "execution": { "model": "script_control" },
              "state": { "persistent": true },
              "permissions": [],
              "capabilities": ["script.api.v0.1"]
            }"#,
        );

        validate_node_definition(&definition).expect("manifest should be valid");
    }

    #[test]
    fn rejects_duplicate_ports() {
        let definition = node_definition(
            r#"{
              "schema": "skenion.node.definition",
              "schemaVersion": "0.1.0",
              "id": "script.dup",
              "version": "0.1.0",
              "displayName": "Duplicate",
              "category": "Script",
              "ports": [
                { "id": "value", "direction": "input", "type": { "flow": "value", "dataKind": "number.f32" } },
                { "id": "value", "direction": "output", "type": { "flow": "value", "dataKind": "number.f32" } }
              ],
              "execution": { "model": "script_control" },
              "state": { "persistent": false },
              "permissions": [],
              "capabilities": []
            }"#,
        );

        let error = validate_node_definition(&definition).expect_err("duplicate port should fail");
        assert!(error_text(&error).contains("duplicate port id on script.dup"));
    }

    #[test]
    fn rejects_output_activation() {
        let definition = node_definition(
            r#"{
              "schema": "skenion.node.definition",
              "schemaVersion": "0.1.0",
              "id": "script.output_activation",
              "version": "0.1.0",
              "displayName": "Output Activation",
              "category": "Script",
              "ports": [
                {
                  "id": "pulse",
                  "direction": "output",
                  "type": { "flow": "event", "dataKind": "bang" },
                  "activation": "trigger"
                }
              ],
              "execution": { "model": "script_control" },
              "state": { "persistent": false },
              "permissions": [],
              "capabilities": []
            }"#,
        );

        let error =
            validate_node_definition(&definition).expect_err("output activation should fail");
        assert!(error_text(&error).contains("must not declare activation"));
    }

    #[test]
    fn rejects_unsupported_permissions() {
        let definition = node_definition(
            r#"{
              "schema": "skenion.node.definition",
              "schemaVersion": "0.1.0",
              "id": "script.permission",
              "version": "0.1.0",
              "displayName": "Permission",
              "category": "Script",
              "ports": [],
              "execution": { "model": "script_control" },
              "state": { "persistent": false },
              "permissions": ["network"],
              "capabilities": []
            }"#,
        );

        let error =
            validate_node_definition(&definition).expect_err("permission should be unsupported");
        assert!(error_text(&error).contains("unsupported permission: network"));
    }

    #[test]
    fn accepts_valid_event_bang_graph() {
        let graph = graph_document(
            r#"{
              "schema": "skenion.graph",
              "schemaVersion": "0.1.0",
              "id": "bang-graph",
              "revision": "1",
              "nodes": [
                {
                  "id": "button",
                  "kind": "core.button",
                  "kindVersion": "0.1.0",
                  "params": {},
                  "ports": [
                    { "id": "out", "direction": "output", "type": { "flow": "event", "dataKind": "bang" } }
                  ]
                },
                {
                  "id": "target",
                  "kind": "core.target",
                  "kindVersion": "0.1.0",
                  "params": {},
                  "ports": [
                    { "id": "in", "direction": "input", "type": { "flow": "event", "dataKind": "bang" }, "activation": "trigger" }
                  ]
                }
              ],
              "edges": [
                { "from": { "node": "button", "port": "out" }, "to": { "node": "target", "port": "in" } }
              ]
            }"#,
        );

        validate_graph_document(&graph).expect("graph should be valid");
    }

    #[test]
    fn rejects_bool_to_bang_without_converter() {
        let graph = graph_document(
            r#"{
              "schema": "skenion.graph",
              "schemaVersion": "0.1.0",
              "id": "bool-to-bang",
              "revision": "1",
              "nodes": [
                {
                  "id": "toggle",
                  "kind": "core.toggle",
                  "kindVersion": "0.1.0",
                  "params": {},
                  "ports": [
                    { "id": "out", "direction": "output", "type": { "flow": "value", "dataKind": "boolean" } }
                  ]
                },
                {
                  "id": "target",
                  "kind": "core.target",
                  "kindVersion": "0.1.0",
                  "params": {},
                  "ports": [
                    { "id": "in", "direction": "input", "type": { "flow": "event", "dataKind": "bang" }, "activation": "trigger" }
                  ]
                }
              ],
              "edges": [
                { "from": { "node": "toggle", "port": "out" }, "to": { "node": "target", "port": "in" } }
              ]
            }"#,
        );

        let error = validate_graph_document(&graph).expect_err("bool to bang should fail");
        assert!(error_text(&error).contains("value<boolean>"));
        assert!(error_text(&error).contains("event<bang>"));
    }
}
