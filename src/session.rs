use serde::{Deserialize, Serialize};

use crate::{
    DummyExecutionReport, ExecutionPlan, GraphDocument, NodeRegistry, ProjectRequest,
    RuntimeDiagnostic, build_execution_plan, run_dummy_execution,
    server::{registry_from_nodes, validate_graph_with_registry},
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeSessionSnapshot {
    pub loaded: bool,
    pub graph_id: Option<String>,
    pub graph_revision: Option<String>,
    pub session_revision: u64,
    pub diagnostics: Vec<RuntimeDiagnostic>,
    pub plan: Option<ExecutionPlan>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeSessionResponse {
    pub ok: bool,
    pub loaded: bool,
    pub graph_id: Option<String>,
    pub graph_revision: Option<String>,
    pub session_revision: u64,
    pub diagnostics: Vec<RuntimeDiagnostic>,
    pub plan: Option<ExecutionPlan>,
    pub report: Option<DummyExecutionReport>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRunRequest {
    pub frames: Option<usize>,
}

#[derive(Debug, Default)]
pub struct RuntimeSession {
    graph: Option<GraphDocument>,
    registry: Option<NodeRegistry>,
    plan: Option<ExecutionPlan>,
    diagnostics: Vec<RuntimeDiagnostic>,
    revision: u64,
}

impl RuntimeSession {
    pub fn snapshot(&self) -> RuntimeSessionSnapshot {
        RuntimeSessionSnapshot {
            loaded: self.graph.is_some(),
            graph_id: self.graph.as_ref().map(|graph| graph.id.clone()),
            graph_revision: self.graph.as_ref().map(|graph| graph.revision.clone()),
            session_revision: self.revision,
            diagnostics: self.diagnostics.clone(),
            plan: self.plan.clone(),
        }
    }

    pub fn load_project(&mut self, request: ProjectRequest) -> RuntimeSessionResponse {
        let ProjectRequest { graph, nodes } = request;
        let registry = match registry_from_nodes(nodes) {
            Ok(registry) => registry,
            Err(diagnostics) => return self.response(false, diagnostics, None),
        };

        if let Err(diagnostics) = validate_graph_with_registry(&graph, &registry) {
            return self.response(false, diagnostics, None);
        }

        let plan = build_execution_plan(&graph, &registry).expect("validated project should plan");
        self.graph = Some(graph);
        self.registry = Some(registry);
        self.plan = Some(plan);
        self.diagnostics = Vec::new();
        self.revision += 1;

        self.response(true, Vec::new(), None)
    }

    pub fn validate_current(&mut self) -> RuntimeSessionResponse {
        let diagnostics = match self.loaded_project() {
            Some((graph, registry)) => validate_graph_with_registry(graph, registry)
                .err()
                .unwrap_or_default(),
            None => vec![RuntimeDiagnostic::error(
                "no project loaded in runtime session",
            )],
        };
        let ok = diagnostics.is_empty();
        self.diagnostics = diagnostics.clone();
        self.response(ok, diagnostics, None)
    }

    pub fn plan_current(&mut self) -> RuntimeSessionResponse {
        let (graph, registry) = match self.loaded_project() {
            Some(project) => project,
            None => {
                let diagnostics = vec![RuntimeDiagnostic::error(
                    "no project loaded in runtime session",
                )];
                self.diagnostics = diagnostics.clone();
                return self.response(false, diagnostics, None);
            }
        };

        if let Err(diagnostics) = validate_graph_with_registry(graph, registry) {
            self.diagnostics = diagnostics.clone();
            self.plan = None;
            return self.response(false, diagnostics, None);
        }

        let plan = build_execution_plan(graph, registry).expect("validated project should plan");
        self.plan = Some(plan);
        self.diagnostics = Vec::new();
        self.response(true, Vec::new(), None)
    }

    pub fn run_current(&mut self, frames: usize) -> RuntimeSessionResponse {
        if self.loaded_project().is_none() {
            let diagnostics = vec![RuntimeDiagnostic::error(
                "no project loaded in runtime session",
            )];
            self.diagnostics = diagnostics.clone();
            return self.response(false, diagnostics, None);
        }

        if self.plan.is_none() {
            let response = self.plan_current();
            if !response.ok {
                return response;
            }
        }

        let report = self
            .plan
            .as_ref()
            .map(|plan| run_dummy_execution(plan, frames));
        self.response(true, self.diagnostics.clone(), report)
    }

    pub fn clear(&mut self) -> RuntimeSessionResponse {
        self.graph = None;
        self.registry = None;
        self.plan = None;
        self.diagnostics = Vec::new();
        self.revision += 1;
        self.response(true, Vec::new(), None)
    }

    pub fn response(
        &self,
        ok: bool,
        diagnostics: Vec<RuntimeDiagnostic>,
        report: Option<DummyExecutionReport>,
    ) -> RuntimeSessionResponse {
        let snapshot = self.snapshot();
        RuntimeSessionResponse {
            ok,
            loaded: snapshot.loaded,
            graph_id: snapshot.graph_id,
            graph_revision: snapshot.graph_revision,
            session_revision: snapshot.session_revision,
            diagnostics,
            plan: snapshot.plan,
            report,
        }
    }

    fn loaded_project(&self) -> Option<(&GraphDocument, &NodeRegistry)> {
        Some((self.graph.as_ref()?, self.registry.as_ref()?))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use crate::{NodeRegistry, ProjectRequest};

    use super::RuntimeSession;

    #[test]
    fn invalid_registry_load_returns_diagnostics_without_revision_change() {
        let mut session = RuntimeSession::default();
        let mut request = sample_project();
        request.nodes[0].schema_version = "9.9.9".to_owned();

        let response = session.load_project(request);

        assert!(!response.ok);
        assert!(!response.loaded);
        assert_eq!(response.session_revision, 0);
        assert!(
            response.diagnostics[0]
                .message
                .contains("invalid node definition")
        );
    }

    #[test]
    fn validate_and_plan_fail_without_loaded_project() {
        let mut session = RuntimeSession::default();

        let validation = session.validate_current();
        let plan = session.plan_current();

        assert!(!validation.ok);
        assert!(!plan.ok);
        assert!(
            plan.diagnostics[0]
                .message
                .contains("no project loaded in runtime session")
        );
    }

    #[test]
    fn plan_current_reports_invalid_stored_project() {
        let mut session = RuntimeSession {
            graph: Some(sample_project().graph),
            registry: Some(NodeRegistry::new()),
            plan: None,
            diagnostics: Vec::new(),
            revision: 1,
        };

        let response = session.plan_current();

        assert!(!response.ok);
        assert!(response.plan.is_none());
        assert!(
            response.diagnostics[0]
                .message
                .contains("missing node definition")
        );
    }

    #[test]
    fn run_current_rebuilds_missing_plan() {
        let mut session = RuntimeSession::default();
        let loaded = session.load_project(sample_project());
        assert!(loaded.ok);
        session.plan = None;

        let response = session.run_current(2);

        assert!(response.ok);
        assert!(response.plan.is_some());
        assert_eq!(response.report.unwrap().frame_count, 2);
    }

    #[test]
    fn run_current_returns_plan_failure_when_rebuild_fails() {
        let mut session = RuntimeSession {
            graph: Some(sample_project().graph),
            registry: Some(NodeRegistry::new()),
            plan: None,
            diagnostics: Vec::new(),
            revision: 1,
        };

        let response = session.run_current(2);

        assert!(!response.ok);
        assert!(response.report.is_none());
        assert!(
            response.diagnostics[0]
                .message
                .contains("missing node definition")
        );
    }

    fn sample_project() -> ProjectRequest {
        serde_json::from_value(sample_project_json()).expect("sample project should parse")
    }

    fn sample_project_json() -> Value {
        json!({
          "graph": {
            "schema": "skenion.graph",
            "schemaVersion": "0.1.0",
            "id": "minimal-value",
            "revision": "1",
            "nodes": [
              {
                "id": "value_1",
                "kind": "core.value-f32",
                "kindVersion": "0.1.0",
                "params": {},
                "ports": [
                  { "id": "value", "direction": "output", "type": { "flow": "value", "dataKind": "f32" } }
                ]
              },
              {
                "id": "target_1",
                "kind": "core.target",
                "kindVersion": "0.1.0",
                "params": {},
                "ports": [
                  { "id": "value", "direction": "input", "type": { "flow": "value", "dataKind": "f32" }, "activation": "latched" }
                ]
              }
            ],
            "edges": [
              { "from": { "node": "value_1", "port": "value" }, "to": { "node": "target_1", "port": "value" } }
            ]
          },
          "nodes": [
            {
              "schema": "skenion.node.definition",
              "schemaVersion": "0.1.0",
              "id": "core.value-f32",
              "version": "0.1.0",
              "displayName": "Float Value",
              "category": "Values",
              "ports": [
                { "id": "value", "direction": "output", "type": { "flow": "value", "dataKind": "f32" } }
              ],
              "execution": { "model": "value" },
              "state": { "persistent": false },
              "permissions": [],
              "capabilities": []
            },
            {
              "schema": "skenion.node.definition",
              "schemaVersion": "0.1.0",
              "id": "core.target",
              "version": "0.1.0",
              "displayName": "Target",
              "category": "Values",
              "ports": [
                { "id": "value", "direction": "input", "type": { "flow": "value", "dataKind": "f32" }, "activation": "latched" }
              ],
              "execution": { "model": "value" },
              "state": { "persistent": false },
              "permissions": [],
              "capabilities": []
            }
          ]
        })
    }
}
