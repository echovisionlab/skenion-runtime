use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use thiserror::Error;

use crate::{
    DataFlow, Edge, ExecutionModel, GraphDocument, GraphNode, NodeRegistry, PlanError, Port,
    PortDirection, build_execution_plan, validate_project,
};

const AUDIO_SIGNAL_KIND: &str = "signal.audio";
const DEFAULT_BLOCK_SIZE: u32 = 64;
const DEFAULT_SAMPLE_RATE: u32 = 48_000;
const DEFAULT_SAMPLE_FORMAT: &str = "f32";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioDspPlanOptions {
    pub block_size: u32,
    pub sample_rate: u32,
}

impl Default for AudioDspPlanOptions {
    fn default() -> Self {
        Self {
            block_size: DEFAULT_BLOCK_SIZE,
            sample_rate: DEFAULT_SAMPLE_RATE,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioDspPlan {
    pub graph_id: String,
    pub graph_revision: String,
    pub block_size: u32,
    pub sample_rate: u32,
    pub nodes: Vec<AudioDspPlanNode>,
    pub edges: Vec<AudioDspPlanEdge>,
    pub buffers: Vec<AudioDspBuffer>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioDspPlanNode {
    pub node_id: String,
    pub kind: String,
    pub kind_version: String,
    pub order: usize,
    pub params: Map<String, Value>,
    pub signal_inputs: Vec<AudioDspSignalInput>,
    pub control_inputs: Vec<AudioDspControlInput>,
    pub signal_outputs: Vec<AudioDspSignalOutput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioDspSignalInput {
    pub port_id: String,
    pub source_node_id: String,
    pub source_port_id: String,
    pub buffer_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioDspControlInput {
    pub port_id: String,
    pub data_kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_node_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_port_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioDspSignalOutput {
    pub port_id: String,
    pub buffer_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioDspPlanEdge {
    pub from_node: String,
    pub from_port: String,
    pub to_node: String,
    pub to_port: String,
    pub buffer_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioDspBuffer {
    pub id: String,
    pub producer_node_id: String,
    pub producer_port_id: String,
    pub sample_format: String,
    pub channels: u32,
}

#[derive(Debug, Error)]
pub enum AudioDspPlanError {
    #[error("{0}")]
    InvalidProject(#[from] crate::ProjectValidationReport),
    #[error("{0}")]
    Plan(#[from] PlanError),
    #[error("audio dsp block size must be greater than zero")]
    InvalidBlockSize,
    #[error("audio dsp sample rate must be greater than zero")]
    InvalidSampleRate,
    #[error("audio signal port {node_id}.{port_id} is not an audio_block node")]
    SignalPortOutsideAudioBlock { node_id: String, port_id: String },
}

pub fn build_audio_dsp_plan(
    graph: &GraphDocument,
    registry: &NodeRegistry,
    options: AudioDspPlanOptions,
) -> Result<AudioDspPlan, AudioDspPlanError> {
    if options.block_size == 0 {
        return Err(AudioDspPlanError::InvalidBlockSize);
    }
    if options.sample_rate == 0 {
        return Err(AudioDspPlanError::InvalidSampleRate);
    }

    validate_project(graph, registry)?;
    let execution_plan = build_execution_plan(graph, registry)?;
    let nodes_by_id = graph
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect::<HashMap<_, _>>();
    let audio_node_ids = execution_plan
        .nodes
        .iter()
        .filter(|node| node.execution_model == ExecutionModel::AudioBlock)
        .map(|node| node.node_id.as_str())
        .collect::<Vec<_>>();
    let audio_node_set = audio_node_ids
        .iter()
        .copied()
        .collect::<std::collections::HashSet<_>>();

    reject_signal_ports_outside_audio_block(graph, &audio_node_set)?;

    let mut buffers = Vec::new();
    let mut buffer_by_output = BTreeMap::new();
    for node_id in &audio_node_ids {
        let node = nodes_by_id
            .get(node_id)
            .expect("execution plan should only contain graph nodes");
        for port in node
            .ports
            .iter()
            .filter(|port| is_audio_signal_output(port))
        {
            let buffer_id = format!("audio_buffer_{}", buffers.len());
            buffer_by_output.insert((node.id.as_str(), port.id.as_str()), buffer_id.clone());
            buffers.push(AudioDspBuffer {
                id: buffer_id,
                producer_node_id: node.id.clone(),
                producer_port_id: port.id.clone(),
                sample_format: DEFAULT_SAMPLE_FORMAT.to_owned(),
                channels: 1,
            });
        }
    }

    let mut signal_edges = Vec::new();
    for edge in graph
        .edges
        .iter()
        .filter(|edge| is_audio_signal_edge(edge, graph))
    {
        let buffer_id = buffer_by_output
            .get(&(edge.from.node.as_str(), edge.from.port.as_str()))
            .cloned()
            .expect("validated audio signal edge should have a producer output buffer");
        signal_edges.push(AudioDspPlanEdge {
            from_node: edge.from.node.clone(),
            from_port: edge.from.port.clone(),
            to_node: edge.to.node.clone(),
            to_port: edge.to.port.clone(),
            buffer_id,
        });
    }

    let nodes = audio_node_ids
        .iter()
        .enumerate()
        .map(|(order, node_id)| {
            let node = nodes_by_id
                .get(node_id)
                .expect("execution plan should only contain graph nodes");
            audio_plan_node(node, order, &signal_edges, &buffer_by_output, graph)
        })
        .collect();

    Ok(AudioDspPlan {
        graph_id: graph.id.clone(),
        graph_revision: graph.revision.clone(),
        block_size: options.block_size,
        sample_rate: options.sample_rate,
        nodes,
        edges: signal_edges,
        buffers,
    })
}

fn reject_signal_ports_outside_audio_block(
    graph: &GraphDocument,
    audio_node_set: &std::collections::HashSet<&str>,
) -> Result<(), AudioDspPlanError> {
    for node in &graph.nodes {
        if audio_node_set.contains(node.id.as_str()) {
            continue;
        }
        if let Some(port) = node.ports.iter().find(|port| is_audio_signal_port(port)) {
            return Err(AudioDspPlanError::SignalPortOutsideAudioBlock {
                node_id: node.id.clone(),
                port_id: port.id.clone(),
            });
        }
    }
    Ok(())
}

fn audio_plan_node(
    node: &GraphNode,
    order: usize,
    signal_edges: &[AudioDspPlanEdge],
    buffer_by_output: &BTreeMap<(&str, &str), String>,
    graph: &GraphDocument,
) -> AudioDspPlanNode {
    let signal_inputs = signal_edges
        .iter()
        .filter(|edge| edge.to_node == node.id)
        .map(|edge| AudioDspSignalInput {
            port_id: edge.to_port.clone(),
            source_node_id: edge.from_node.clone(),
            source_port_id: edge.from_port.clone(),
            buffer_id: edge.buffer_id.clone(),
        })
        .collect();
    let signal_outputs = node
        .ports
        .iter()
        .filter(|port| is_audio_signal_output(port))
        .map(|port| AudioDspSignalOutput {
            port_id: port.id.clone(),
            buffer_id: buffer_by_output
                .get(&(node.id.as_str(), port.id.as_str()))
                .expect("audio output should have an allocated buffer")
                .clone(),
        })
        .collect();
    let control_inputs = node
        .ports
        .iter()
        .filter(|port| port.direction == PortDirection::Input && !is_audio_signal_port(port))
        .map(|port| {
            let source = graph
                .edges
                .iter()
                .find(|edge| edge.to.node == node.id && edge.to.port == port.id);
            AudioDspControlInput {
                port_id: port.id.clone(),
                data_kind: port.data_type.data_kind.clone(),
                source_node_id: source.map(|edge| edge.from.node.clone()),
                source_port_id: source.map(|edge| edge.from.port.clone()),
            }
        })
        .collect();

    AudioDspPlanNode {
        node_id: node.id.clone(),
        kind: node.kind.clone(),
        kind_version: node.kind_version.clone(),
        order,
        params: node.params.clone(),
        signal_inputs,
        control_inputs,
        signal_outputs,
    }
}

fn is_audio_signal_edge(edge: &Edge, graph: &GraphDocument) -> bool {
    let Some(from) = find_port(graph, &edge.from.node, &edge.from.port) else {
        return false;
    };
    let Some(to) = find_port(graph, &edge.to.node, &edge.to.port) else {
        return false;
    };
    is_audio_signal_output(from) && is_audio_signal_input(to)
}

fn find_port<'a>(graph: &'a GraphDocument, node_id: &str, port_id: &str) -> Option<&'a Port> {
    graph
        .nodes
        .iter()
        .find(|node| node.id == node_id)
        .and_then(|node| node.ports.iter().find(|port| port.id == port_id))
}

fn is_audio_signal_port(port: &Port) -> bool {
    port.data_type.flow == DataFlow::Signal && port.data_type.data_kind == AUDIO_SIGNAL_KIND
}

fn is_audio_signal_input(port: &Port) -> bool {
    port.direction == PortDirection::Input && is_audio_signal_port(port)
}

fn is_audio_signal_output(port: &Port) -> bool {
    port.direction == PortDirection::Output && is_audio_signal_port(port)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::NodeDefinition;

    fn registry() -> NodeRegistry {
        let mut registry = NodeRegistry::new();
        for definition in [
            audio_source_definition("audio.osc", "frequency"),
            audio_binary_definition("audio.operator.mul"),
            audio_unary_definition("audio.operator.sqrt", "in"),
            audio_snapshot_definition(),
            float_definition(),
            bad_signal_definition(),
        ] {
            registry.insert(definition).unwrap();
        }
        registry
    }

    fn graph(value: serde_json::Value) -> GraphDocument {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn builds_stable_audio_block_plan_with_buffers_and_control_inputs() {
        let graph = graph(json!({
          "schema": "skenion.graph",
          "schemaVersion": "0.1.0",
          "id": "audio-dsp",
          "revision": "7",
          "nodes": [
            float_node("freq", 220.0),
            audio_osc_node("osc"),
            audio_binary_node("mul", "audio.operator.mul"),
            audio_snapshot_node("snap")
          ],
          "edges": [
            { "from": { "node": "freq", "port": "value" }, "to": { "node": "osc", "port": "frequency" } },
            { "from": { "node": "osc", "port": "out" }, "to": { "node": "mul", "port": "left" } },
            { "from": { "node": "mul", "port": "out" }, "to": { "node": "snap", "port": "signal" } }
          ]
        }));

        let plan = build_audio_dsp_plan(
            &graph,
            &registry(),
            AudioDspPlanOptions {
                block_size: 128,
                sample_rate: 44_100,
            },
        )
        .unwrap();

        assert_eq!(plan.graph_id, "audio-dsp");
        assert_eq!(plan.graph_revision, "7");
        assert_eq!(plan.block_size, 128);
        assert_eq!(plan.sample_rate, 44_100);
        assert_eq!(
            plan.nodes
                .iter()
                .map(|node| (node.order, node.node_id.as_str()))
                .collect::<Vec<_>>(),
            vec![(0, "osc"), (1, "mul"), (2, "snap")]
        );
        assert_eq!(
            plan.buffers
                .iter()
                .map(|buffer| {
                    (
                        buffer.id.as_str(),
                        buffer.producer_node_id.as_str(),
                        buffer.producer_port_id.as_str(),
                    )
                })
                .collect::<Vec<_>>(),
            vec![
                ("audio_buffer_0", "osc", "out"),
                ("audio_buffer_1", "mul", "out")
            ]
        );
        assert_eq!(
            plan.edges
                .iter()
                .map(|edge| {
                    (
                        edge.from_node.as_str(),
                        edge.from_port.as_str(),
                        edge.to_node.as_str(),
                        edge.to_port.as_str(),
                        edge.buffer_id.as_str(),
                    )
                })
                .collect::<Vec<_>>(),
            vec![
                ("osc", "out", "mul", "left", "audio_buffer_0"),
                ("mul", "out", "snap", "signal", "audio_buffer_1")
            ]
        );
        assert_eq!(
            plan.nodes[0].control_inputs,
            vec![AudioDspControlInput {
                port_id: "frequency".to_owned(),
                data_kind: "number.float".to_owned(),
                source_node_id: Some("freq".to_owned()),
                source_port_id: Some("value".to_owned()),
            }]
        );
        assert_eq!(
            plan.nodes[1].signal_inputs,
            vec![AudioDspSignalInput {
                port_id: "left".to_owned(),
                source_node_id: "osc".to_owned(),
                source_port_id: "out".to_owned(),
                buffer_id: "audio_buffer_0".to_owned(),
            }]
        );
        assert_eq!(plan.nodes[2].signal_outputs, Vec::new());
        assert_eq!(plan.nodes[2].control_inputs[0].port_id, "trigger");
    }

    #[test]
    fn builds_empty_plan_when_graph_has_no_audio_nodes() {
        let graph = graph(json!({
          "schema": "skenion.graph",
          "schemaVersion": "0.1.0",
          "id": "control-only",
          "revision": "1",
          "nodes": [float_node("value", 1.0)],
          "edges": []
        }));

        let plan =
            build_audio_dsp_plan(&graph, &registry(), AudioDspPlanOptions::default()).unwrap();

        assert_eq!(plan.nodes, Vec::new());
        assert_eq!(plan.edges, Vec::new());
        assert_eq!(plan.buffers, Vec::new());
        assert_eq!(plan.block_size, DEFAULT_BLOCK_SIZE);
        assert_eq!(plan.sample_rate, DEFAULT_SAMPLE_RATE);
    }

    #[test]
    fn rejects_invalid_options_and_projects() {
        let graph = graph(json!({
          "schema": "skenion.graph",
          "schemaVersion": "0.1.0",
          "id": "invalid-options",
          "revision": "1",
          "nodes": [float_node("value", 1.0)],
          "edges": []
        }));
        let registry = registry();

        let block_error = build_audio_dsp_plan(
            &graph,
            &registry,
            AudioDspPlanOptions {
                block_size: 0,
                sample_rate: 48_000,
            },
        )
        .unwrap_err();
        let rate_error = build_audio_dsp_plan(
            &graph,
            &registry,
            AudioDspPlanOptions {
                block_size: 64,
                sample_rate: 0,
            },
        )
        .unwrap_err();
        let project_error =
            build_audio_dsp_plan(&graph, &NodeRegistry::new(), AudioDspPlanOptions::default())
                .unwrap_err();

        assert!(matches!(block_error, AudioDspPlanError::InvalidBlockSize));
        assert!(matches!(rate_error, AudioDspPlanError::InvalidSampleRate));
        assert!(matches!(
            project_error,
            AudioDspPlanError::InvalidProject(_)
        ));
    }

    #[test]
    fn rejects_signal_ports_outside_audio_block_nodes() {
        let graph = graph(json!({
          "schema": "skenion.graph",
          "schemaVersion": "0.1.0",
          "id": "bad-signal",
          "revision": "1",
          "nodes": [bad_signal_node("bad")],
          "edges": []
        }));

        let error =
            build_audio_dsp_plan(&graph, &registry(), AudioDspPlanOptions::default()).unwrap_err();

        assert!(matches!(
            error,
            AudioDspPlanError::SignalPortOutsideAudioBlock { .. }
        ));
        assert!(error.to_string().contains("bad.out"));
    }

    #[test]
    fn signal_edge_helper_ignores_missing_endpoint_ports() {
        let graph = graph(json!({
          "schema": "skenion.graph",
          "schemaVersion": "0.1.0",
          "id": "missing-edge-ports",
          "revision": "1",
          "nodes": [audio_osc_node("osc"), audio_binary_node("mul", "audio.operator.mul")],
          "edges": []
        }));
        let missing_from = serde_json::from_value::<Edge>(json!({
          "from": { "node": "osc", "port": "missing" },
          "to": { "node": "mul", "port": "left" }
        }))
        .unwrap();
        let missing_to = serde_json::from_value::<Edge>(json!({
          "from": { "node": "osc", "port": "out" },
          "to": { "node": "mul", "port": "missing" }
        }))
        .unwrap();

        assert!(!is_audio_signal_edge(&missing_from, &graph));
        assert!(!is_audio_signal_edge(&missing_to, &graph));
    }

    fn audio_source_definition(id: &str, input_port: &str) -> NodeDefinition {
        node_definition(json!({
          "schema": "skenion.node.definition",
          "schemaVersion": "0.1.0",
          "id": id,
          "version": "0.1.0",
          "displayName": id,
          "category": "Audio",
          "ports": [
            { "id": input_port, "direction": "input", "type": { "flow": "value", "dataKind": "number.float" }, "activation": "latched" },
            { "id": "out", "direction": "output", "type": { "flow": "signal", "dataKind": "signal.audio" } }
          ],
          "execution": { "model": "audio_block" },
          "state": { "persistent": false },
          "permissions": [],
          "capabilities": []
        }))
    }

    fn audio_binary_definition(id: &str) -> NodeDefinition {
        node_definition(json!({
          "schema": "skenion.node.definition",
          "schemaVersion": "0.1.0",
          "id": id,
          "version": "0.1.0",
          "displayName": id,
          "category": "Audio",
          "ports": [
            { "id": "left", "direction": "input", "type": { "flow": "signal", "dataKind": "signal.audio" }, "activation": "latched" },
            { "id": "right", "direction": "input", "type": { "flow": "signal", "dataKind": "signal.audio" }, "activation": "latched" },
            { "id": "out", "direction": "output", "type": { "flow": "signal", "dataKind": "signal.audio" } }
          ],
          "execution": { "model": "audio_block" },
          "state": { "persistent": false },
          "permissions": [],
          "capabilities": []
        }))
    }

    fn audio_unary_definition(id: &str, input_port: &str) -> NodeDefinition {
        node_definition(json!({
          "schema": "skenion.node.definition",
          "schemaVersion": "0.1.0",
          "id": id,
          "version": "0.1.0",
          "displayName": id,
          "category": "Audio",
          "ports": [
            { "id": input_port, "direction": "input", "type": { "flow": "signal", "dataKind": "signal.audio" }, "activation": "latched" },
            { "id": "out", "direction": "output", "type": { "flow": "signal", "dataKind": "signal.audio" } }
          ],
          "execution": { "model": "audio_block" },
          "state": { "persistent": false },
          "permissions": [],
          "capabilities": []
        }))
    }

    fn audio_snapshot_definition() -> NodeDefinition {
        node_definition(json!({
          "schema": "skenion.node.definition",
          "schemaVersion": "0.1.0",
          "id": "audio.snapshot",
          "version": "0.1.0",
          "displayName": "snapshot~",
          "category": "Audio",
          "ports": [
            { "id": "signal", "direction": "input", "type": { "flow": "signal", "dataKind": "signal.audio" }, "activation": "latched" },
            { "id": "trigger", "direction": "input", "type": { "flow": "event", "dataKind": "message.any" }, "activation": "trigger" },
            { "id": "value", "direction": "output", "type": { "flow": "value", "dataKind": "number.float" } }
          ],
          "execution": { "model": "audio_block" },
          "state": { "persistent": false },
          "permissions": [],
          "capabilities": []
        }))
    }

    fn float_definition() -> NodeDefinition {
        node_definition(json!({
          "schema": "skenion.node.definition",
          "schemaVersion": "0.1.0",
          "id": "core.float",
          "version": "0.1.0",
          "displayName": "Float",
          "category": "Core",
          "ports": [
            { "id": "value", "direction": "output", "type": { "flow": "value", "dataKind": "number.float" } }
          ],
          "execution": { "model": "value" },
          "state": { "persistent": false },
          "permissions": [],
          "capabilities": []
        }))
    }

    fn bad_signal_definition() -> NodeDefinition {
        node_definition(json!({
          "schema": "skenion.node.definition",
          "schemaVersion": "0.1.0",
          "id": "test.bad-signal",
          "version": "0.1.0",
          "displayName": "Bad Signal",
          "category": "Test",
          "ports": [
            { "id": "out", "direction": "output", "type": { "flow": "signal", "dataKind": "signal.audio" } }
          ],
          "execution": { "model": "value" },
          "state": { "persistent": false },
          "permissions": [],
          "capabilities": []
        }))
    }

    fn node_definition(value: serde_json::Value) -> NodeDefinition {
        serde_json::from_value(value).unwrap()
    }

    fn float_node(id: &str, value: f64) -> serde_json::Value {
        json!({
          "id": id,
          "kind": "core.float",
          "kindVersion": "0.1.0",
          "params": { "value": value },
          "ports": [
            { "id": "value", "direction": "output", "type": { "flow": "value", "dataKind": "number.float" } }
          ]
        })
    }

    fn audio_osc_node(id: &str) -> serde_json::Value {
        json!({
          "id": id,
          "kind": "audio.osc",
          "kindVersion": "0.1.0",
          "params": { "frequency": 440.0 },
          "ports": [
            { "id": "frequency", "direction": "input", "type": { "flow": "value", "dataKind": "number.float" }, "activation": "latched" },
            { "id": "out", "direction": "output", "type": { "flow": "signal", "dataKind": "signal.audio" } }
          ]
        })
    }

    fn audio_binary_node(id: &str, kind: &str) -> serde_json::Value {
        json!({
          "id": id,
          "kind": kind,
          "kindVersion": "0.1.0",
          "params": {},
          "ports": [
            { "id": "left", "direction": "input", "type": { "flow": "signal", "dataKind": "signal.audio" }, "activation": "latched" },
            { "id": "right", "direction": "input", "type": { "flow": "signal", "dataKind": "signal.audio" }, "activation": "latched" },
            { "id": "out", "direction": "output", "type": { "flow": "signal", "dataKind": "signal.audio" } }
          ]
        })
    }

    fn audio_snapshot_node(id: &str) -> serde_json::Value {
        json!({
          "id": id,
          "kind": "audio.snapshot",
          "kindVersion": "0.1.0",
          "params": {},
          "ports": [
            { "id": "signal", "direction": "input", "type": { "flow": "signal", "dataKind": "signal.audio" }, "activation": "latched" },
            { "id": "trigger", "direction": "input", "type": { "flow": "event", "dataKind": "message.any" }, "activation": "trigger" },
            { "id": "value", "direction": "output", "type": { "flow": "value", "dataKind": "number.float" } }
          ]
        })
    }

    fn bad_signal_node(id: &str) -> serde_json::Value {
        json!({
          "id": id,
          "kind": "test.bad-signal",
          "kindVersion": "0.1.0",
          "params": {},
          "ports": [
            { "id": "out", "direction": "output", "type": { "flow": "signal", "dataKind": "signal.audio" } }
          ]
        })
    }
}
