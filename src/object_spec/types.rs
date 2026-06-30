use serde_json::{Map, Value};
use skenion_contracts::{MessageKeyPolicyV01, PackageChecksumV01};

use crate::nodes::CoreNodeConstructor;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ObjectSpecResolution {
    pub(crate) input: String,
    pub(crate) display_text: String,
    pub(crate) class_symbol: String,
    pub(crate) creation_args: Vec<ObjectSpecAtom>,
    pub(crate) resolved_kind: Option<String>,
    pub(crate) resolved_kind_version: Option<String>,
    pub(crate) params: Map<String, Value>,
    pub(crate) instance_ports: Vec<ObjectSpecPort>,
    pub(crate) candidates: Vec<ObjectSpecCandidateSummary>,
    pub(crate) diagnostics: Vec<ObjectSpecDiagnostic>,
}

impl ObjectSpecResolution {
    pub(crate) fn ok(&self) -> bool {
        self.diagnostics.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ObjectSpecCandidateSummary {
    pub(crate) id: String,
    pub(crate) source: String,
    pub(crate) kind: String,
    pub(crate) display_name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ObjectSpecAtom {
    Float(f64),
    Int(i64),
    Bool(bool),
    Symbol(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ObjectSpecDiagnostic {
    pub(crate) code: String,
    pub(crate) message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ObjectSpecPort {
    pub(crate) id: String,
    pub(crate) direction: ObjectSpecPortDirection,
    pub(crate) port_type: String,
    pub(crate) label: Option<String>,
    pub(crate) rate: ObjectSpecPortRate,
    pub(crate) accepts: Option<Vec<String>>,
    pub(crate) activation: Option<ObjectSpecPortActivation>,
    pub(crate) message_keys: Option<MessageKeyPolicyV01>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ObjectSpecPortDirection {
    Input,
    Output,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ObjectSpecPortRate {
    Event,
    Control,
    Audio,
    Render,
    Gpu,
    Resource,
    Io,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ObjectSpecPortActivation {
    Trigger,
    Latched,
    Passive,
}

#[derive(Debug, Clone)]
pub(crate) struct ObjectRegistry {
    pub(super) candidates: Vec<ObjectRegistryCandidate>,
    pub(super) allow_unchecked_project_patch_refs: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub(super) enum ObjectRegistrySource {
    FirstPartyCore,
    ProjectPatch,
    PackageProvider,
    NativeProvider,
}

#[derive(Debug, Clone)]
pub(super) struct ObjectRegistryCandidate {
    pub(super) id: String,
    pub(super) source: ObjectRegistrySource,
    pub(super) aliases: Vec<String>,
    pub(super) kind: String,
    pub(super) kind_version: String,
    pub(super) display_name: String,
    pub(super) constructor: Option<CoreNodeConstructor>,
    pub(super) catalog_category: Option<&'static str>,
    pub(super) project_patch: Option<ProjectPatchCandidate>,
}

#[derive(Debug, Clone)]
pub(super) struct ProjectPatchCandidate {
    pub(super) patch_id: String,
    pub(super) revision: String,
    pub(super) description: Option<String>,
    pub(super) interface_digest: PackageChecksumV01,
    pub(super) ports: Vec<ObjectSpecPort>,
}

#[derive(Debug, Clone)]
pub(super) struct ParsedObjectSpec {
    pub(super) input: String,
    pub(super) display_text: String,
    pub(super) class_symbol: String,
    pub(super) creation_args: Vec<ObjectSpecAtom>,
}
