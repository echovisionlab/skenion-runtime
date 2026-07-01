use serde_json::Value;
use skenion_contracts::{NodeCatalogSnapshotV01, PackageChecksumV01};

use super::super::node_catalog::node_catalog_snapshot_for_session;
use super::control::GraphControlEmission;
use crate::{PasteGraphFragmentResponse, RuntimePatchResponse, RuntimeSession};

#[derive(Debug)]
pub(in crate::realtime) struct GraphCommandOutcome {
    pub(in crate::realtime) response: RuntimePatchResponse,
    pub(in crate::realtime) node_result: Option<Value>,
    pub(super) operation_result: Option<PasteGraphFragmentResponse>,
    pub(super) control_emission: Option<GraphControlEmission>,
    pub(super) catalog_snapshot: Option<NodeCatalogSnapshotV01>,
}

impl GraphCommandOutcome {
    pub(super) fn from_response(response: RuntimePatchResponse) -> Self {
        Self {
            response,
            node_result: None,
            operation_result: None,
            control_emission: None,
            catalog_snapshot: None,
        }
    }

    pub(super) fn with_operation_result(
        response: RuntimePatchResponse,
        operation_result: PasteGraphFragmentResponse,
    ) -> Self {
        Self {
            response,
            node_result: None,
            operation_result: Some(operation_result),
            control_emission: None,
            catalog_snapshot: None,
        }
    }

    pub(super) fn with_node_result(response: RuntimePatchResponse, node_result: Value) -> Self {
        Self {
            response,
            node_result: Some(node_result),
            operation_result: None,
            control_emission: None,
            catalog_snapshot: None,
        }
    }

    pub(super) fn with_node_result_and_control_emission(
        response: RuntimePatchResponse,
        node_result: Value,
        control_emission: Option<GraphControlEmission>,
    ) -> Self {
        Self {
            response,
            node_result: Some(node_result),
            operation_result: None,
            control_emission,
            catalog_snapshot: None,
        }
    }

    pub(super) fn with_catalog_change(
        mut self,
        before_catalog_revision: PackageChecksumV01,
        session: &RuntimeSession,
    ) -> Self {
        if self.response.applied {
            let snapshot = node_catalog_snapshot_for_session(session);
            if snapshot.catalog_revision != before_catalog_revision {
                self.catalog_snapshot = Some(snapshot);
            }
        }
        self
    }
}
