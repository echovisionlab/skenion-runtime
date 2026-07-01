use serde_json::json;

use crate::{
    ExecutionPlan, GraphDocument, GraphDocumentCurrent, NodeRegistry,
    ObjectResolutionStatusCurrent, PlanError, RuntimeDiagnostic, build_execution_plan,
};

pub(super) fn unresolved_object_diagnostics_current(
    graph: &GraphDocumentCurrent,
) -> Vec<RuntimeDiagnostic> {
    graph
        .nodes
        .iter()
        .filter(|node| {
            node.object_resolution.as_ref().is_some_and(|resolution| {
                resolution.status != ObjectResolutionStatusCurrent::Resolved
            })
        })
        .map(|node| {
            let object_spec = node.object_spec.as_deref().unwrap_or(node.id.as_str());
            let diagnostic_message = node
                .object_resolution
                .as_ref()
                .and_then(|resolution| resolution.diagnostics.first())
                .map(|diagnostic| diagnostic.message.as_str())
                .unwrap_or("object spec could not be resolved");
            RuntimeDiagnostic::error(format!(
                "unresolved object {object_spec}: {diagnostic_message}"
            ))
        })
        .collect()
}

pub(super) fn build_session_execution_plan(
    graph: &GraphDocument,
    registry: &NodeRegistry,
    surface: &'static str,
) -> Result<ExecutionPlan, Vec<RuntimeDiagnostic>> {
    build_execution_plan(graph, registry)
        .map_err(|error| plan_error_diagnostics(error, surface, graph))
}

fn plan_error_diagnostics(
    error: PlanError,
    surface: &'static str,
    graph: &GraphDocument,
) -> Vec<RuntimeDiagnostic> {
    let details = || {
        json!({
            "surface": surface,
            "graphId": graph.id,
            "graphRevision": graph.revision,
        })
    };
    match error {
        PlanError::InvalidProject(report) => report
            .errors()
            .iter()
            .map(|error| {
                RuntimeDiagnostic::structured_error(
                    "session.plan.invalid-project",
                    error.message.clone(),
                    details(),
                )
            })
            .collect(),
        PlanError::Cycle { nodes } => vec![RuntimeDiagnostic::structured_error(
            "session.plan.cycle",
            format!("cycle detected: {nodes}"),
            json!({
                "surface": surface,
                "graphId": graph.id,
                "graphRevision": graph.revision,
                "nodes": nodes,
            }),
        )],
    }
}
