use serde_json::{Map, Value, json};

use super::ports::{
    audio_binary_ports, audio_input_ports, audio_osc_ports, audio_output_ports, audio_sig_ports,
    bang_ports, comment_ports, control_operator_ports, control_sqrt_ports, message_ports,
    stored_value_ports,
};
use super::{
    ObjectRegistryCandidate, ObjectSpecAtom, ObjectSpecCandidateSummary, ObjectSpecDiagnostic,
    ObjectSpecPort, ObjectSpecResolution, ParsedObjectSpec,
};
use crate::nodes::CoreNodeConstructor;

pub(super) fn parse_object_spec_input_v01(
    input: &str,
) -> Result<ParsedObjectSpec, Box<ObjectSpecResolution>> {
    let parsed = skenion_contracts::parse_object_spec_v01(input);
    let creation_args = parsed
        .creation_args
        .iter()
        .map(contract_object_spec_atom_to_runtime)
        .collect::<Vec<_>>();
    if parsed.ok {
        return Ok(ParsedObjectSpec {
            input: parsed.input,
            display_text: parsed.display_text,
            class_symbol: parsed.class_name,
            creation_args,
        });
    }

    let diagnostic = parsed.diagnostics.first();
    let code = diagnostic
        .map(|diagnostic| runtime_object_spec_diagnostic_code(&diagnostic.code))
        .unwrap_or_else(|| "object-spec.invalid-syntax".to_owned());
    let message = diagnostic
        .map(|diagnostic| diagnostic.message.clone())
        .unwrap_or_else(|| "object spec could not be parsed".to_owned());
    Err(Box::new(failure(
        &parsed.input,
        parsed.display_text,
        &parsed.class_name,
        creation_args,
        code,
        message,
    )))
}

fn runtime_object_spec_diagnostic_code(code: &str) -> String {
    match code {
        "empty-object-spec" => "object-spec.empty".to_owned(),
        "invalid-syntax" => "object-spec.invalid-syntax".to_owned(),
        value if value.starts_with("object-spec.") => value.to_owned(),
        value => format!("object-spec.{value}"),
    }
}

fn contract_object_spec_atom_to_runtime(
    atom: &skenion_contracts::ObjectSpecAtomV01,
) -> ObjectSpecAtom {
    match atom {
        skenion_contracts::ObjectSpecAtomV01::Float { value, .. } => ObjectSpecAtom::Float(*value),
        skenion_contracts::ObjectSpecAtomV01::Int { value, .. } => ObjectSpecAtom::Int(*value),
        skenion_contracts::ObjectSpecAtomV01::Uint { value, .. } => {
            if *value <= i64::MAX as u64 {
                ObjectSpecAtom::Int(*value as i64)
            } else {
                ObjectSpecAtom::Symbol(value.to_string())
            }
        }
        skenion_contracts::ObjectSpecAtomV01::Bool { value } => ObjectSpecAtom::Bool(*value),
        skenion_contracts::ObjectSpecAtomV01::Identifier { value }
        | skenion_contracts::ObjectSpecAtomV01::String { value } => {
            ObjectSpecAtom::Symbol(value.clone())
        }
    }
}

pub(super) fn construct_first_party_core(
    parsed: ParsedObjectSpec,
    candidate: &ObjectRegistryCandidate,
) -> ObjectSpecResolution {
    let ParsedObjectSpec {
        input,
        display_text,
        class_symbol,
        creation_args,
    } = parsed;

    match candidate.constructor {
        Some(CoreNodeConstructor::ControlOperator) => {
            return resolve_control_operator(
                &input,
                display_text,
                &class_symbol,
                creation_args,
                candidate,
            );
        }
        Some(CoreNodeConstructor::ControlValue) => {
            return resolve_control_value(
                &input,
                display_text,
                &class_symbol,
                creation_args,
                candidate,
            );
        }
        Some(CoreNodeConstructor::Audio) => {
            return resolve_audio_object(
                &input,
                display_text,
                &class_symbol,
                creation_args,
                candidate,
            );
        }
        Some(CoreNodeConstructor::Subpatch) => {
            return resolve_named_ref_object(
                &input,
                display_text,
                &class_symbol,
                creation_args,
                candidate,
                "patchRef",
                "subpatch object spec requires exactly one patch reference",
            );
        }
        Some(CoreNodeConstructor::BoundaryPort) => {
            return resolve_optional_named_ref_object(
                &input,
                display_text,
                &class_symbol,
                creation_args,
                candidate,
                "portId",
            );
        }
        None => {}
    }

    failure_with_candidates(
        &input,
        display_text,
        &class_symbol,
        creation_args,
        vec![candidate.summary()],
        "object-spec.unresolved",
        format!(
            "{} is registered but has no Runtime constructor",
            candidate.kind
        ),
    )
}

pub(super) fn construct_project_patch(
    parsed: ParsedObjectSpec,
    candidate: &ObjectRegistryCandidate,
) -> ObjectSpecResolution {
    let ParsedObjectSpec {
        input,
        display_text,
        class_symbol,
        creation_args,
    } = parsed;
    let Some(patch) = candidate.project_patch.as_ref() else {
        return failure_with_candidates(
            &input,
            display_text,
            &class_symbol,
            creation_args,
            vec![candidate.summary()],
            "object-spec.unresolved",
            "project patch candidate is missing patch metadata",
        );
    };

    if matches!(class_symbol.as_str(), "p" | "object.core.subpatch") {
        if creation_args.len() != 1 {
            return failure_with_candidates(
                &input,
                display_text,
                &class_symbol,
                creation_args,
                vec![candidate.summary()],
                "object-spec.invalid-arg-count",
                "subpatch object spec requires exactly one patch reference",
            );
        }
        let Some(reference) = symbol_value(&creation_args[0]) else {
            return failure_with_candidates(
                &input,
                display_text,
                &class_symbol,
                creation_args,
                vec![candidate.summary()],
                "object-spec.invalid-arg-type",
                format!("{class_symbol} reference argument must be a symbol"),
            );
        };
        if reference != patch.patch_id {
            return failure_with_candidates(
                &input,
                display_text,
                &class_symbol,
                creation_args,
                vec![candidate.summary()],
                "object-spec.unresolved",
                format!("project patch {reference} is not available in the active project"),
            );
        }
    } else if !creation_args.is_empty() {
        return failure_with_candidates(
            &input,
            display_text,
            &class_symbol,
            creation_args,
            vec![candidate.summary()],
            "object-spec.invalid-arg-count",
            format!("{class_symbol} project patch shortcut accepts no creation arguments"),
        );
    }

    let mut params = Map::new();
    params.insert("patchRef".to_owned(), Value::String(patch.patch_id.clone()));
    params.insert(
        "patchRevision".to_owned(),
        Value::String(patch.revision.clone()),
    );
    success(
        &input,
        display_text,
        &class_symbol,
        creation_args,
        candidate,
        params,
        patch.ports.clone(),
    )
}

pub(super) fn explicit_project_patch_ref(parsed: &ParsedObjectSpec) -> Option<String> {
    if parsed.creation_args.len() != 1 {
        return None;
    }
    symbol_value(&parsed.creation_args[0])
}

pub(super) fn unresolved_resolution(parsed: ParsedObjectSpec) -> ObjectSpecResolution {
    failure(
        &parsed.input,
        parsed.display_text,
        &parsed.class_symbol,
        parsed.creation_args,
        "object-spec.unresolved",
        format!(
            "{} is not available in the local Runtime object registry",
            parsed.class_symbol
        ),
    )
}

pub(super) fn ambiguous_resolution(
    parsed: ParsedObjectSpec,
    candidates: Vec<ObjectRegistryCandidate>,
) -> ObjectSpecResolution {
    let summaries = candidates
        .iter()
        .map(ObjectRegistryCandidate::summary)
        .collect::<Vec<_>>();
    let candidate_list = summaries
        .iter()
        .map(|candidate| format!("{} ({})", candidate.id, candidate.source))
        .collect::<Vec<_>>()
        .join(", ");
    failure_with_candidates(
        &parsed.input,
        parsed.display_text,
        &parsed.class_symbol,
        parsed.creation_args,
        summaries,
        "object-spec.ambiguous",
        format!(
            "{} matches multiple Runtime object candidates: {candidate_list}",
            parsed.class_symbol
        ),
    )
}

fn resolve_control_operator(
    input: &str,
    display_text: String,
    class_symbol: &str,
    creation_args: Vec<ObjectSpecAtom>,
    candidate: &ObjectRegistryCandidate,
) -> ObjectSpecResolution {
    let kind = candidate.kind.as_str();
    if kind == "object.core.operator.sqrt" {
        if !creation_args.is_empty() {
            return failure(
                input,
                display_text,
                class_symbol,
                creation_args,
                "object-spec.invalid-arg-count",
                "sqrt accepts no creation arguments",
            );
        }
        return success(
            input,
            display_text,
            class_symbol,
            creation_args,
            candidate,
            Map::new(),
            control_sqrt_ports(),
        );
    }

    if creation_args.len() > 1 {
        return failure(
            input,
            display_text,
            class_symbol,
            creation_args,
            "object-spec.invalid-arg-count",
            format!("{class_symbol} accepts at most one creation argument"),
        );
    }

    let right = match creation_args.first() {
        Some(arg) => match numeric_value(arg) {
            Some(value) => value,
            None => {
                return failure(
                    input,
                    display_text,
                    class_symbol,
                    creation_args,
                    "object-spec.invalid-arg-type",
                    format!("{class_symbol} creation argument must be numeric"),
                );
            }
        },
        None => 0.0,
    };
    let mut params = Map::new();
    insert_number(&mut params, "right", right);
    success(
        input,
        display_text,
        class_symbol,
        creation_args,
        candidate,
        params,
        control_operator_ports(),
    )
}

fn resolve_control_value(
    input: &str,
    display_text: String,
    class_symbol: &str,
    creation_args: Vec<ObjectSpecAtom>,
    candidate: &ObjectRegistryCandidate,
) -> ObjectSpecResolution {
    let kind = candidate.kind.as_str();
    match kind {
        "object.core.bang" => {
            if !creation_args.is_empty() {
                return failure(
                    input,
                    display_text,
                    class_symbol,
                    creation_args,
                    "object-spec.invalid-arg-count",
                    format!("{class_symbol} accepts no creation arguments"),
                );
            }
            success(
                input,
                display_text,
                class_symbol,
                creation_args,
                candidate,
                Map::new(),
                bang_ports(),
            )
        }
        "object.core.message" | "object.core.comment" => {
            let text = creation_args
                .iter()
                .map(atom_display_text)
                .collect::<Vec<_>>()
                .join(" ");
            let mut params = Map::new();
            params.insert("text".to_owned(), Value::String(text));
            let ports = if kind == "object.core.message" {
                message_ports()
            } else {
                comment_ports()
            };
            success(
                input,
                display_text,
                class_symbol,
                creation_args,
                candidate,
                params,
                ports,
            )
        }
        "object.core.float" => resolve_number_value(
            input,
            display_text,
            class_symbol,
            creation_args,
            candidate,
            NumberValueSpec {
                port_type: "value.core.float32",
                coerce: numeric_value,
                to_json: |value| json!(value),
            },
        ),
        "object.core.int" => resolve_number_value(
            input,
            display_text,
            class_symbol,
            creation_args,
            candidate,
            NumberValueSpec {
                port_type: "value.core.int32",
                coerce: integer_value,
                to_json: |value| json!(value),
            },
        ),
        "object.core.uint" => resolve_number_value(
            input,
            display_text,
            class_symbol,
            creation_args,
            candidate,
            NumberValueSpec {
                port_type: "value.core.uint32",
                coerce: unsigned_value,
                to_json: |value| json!(value),
            },
        ),
        _ => unreachable!("control value resolver received unknown kind"),
    }
}

struct NumberValueSpec<T> {
    port_type: &'static str,
    coerce: fn(&ObjectSpecAtom) -> Option<T>,
    to_json: fn(T) -> Value,
}

fn resolve_number_value<T>(
    input: &str,
    display_text: String,
    class_symbol: &str,
    creation_args: Vec<ObjectSpecAtom>,
    candidate: &ObjectRegistryCandidate,
    spec: NumberValueSpec<T>,
) -> ObjectSpecResolution {
    if creation_args.len() > 1 {
        return failure(
            input,
            display_text,
            class_symbol,
            creation_args,
            "object-spec.invalid-arg-count",
            format!("{class_symbol} accepts at most one creation argument"),
        );
    }

    let value = match creation_args.first() {
        Some(arg) => match (spec.coerce)(arg) {
            Some(value) => (spec.to_json)(value),
            None => {
                return failure(
                    input,
                    display_text,
                    class_symbol,
                    creation_args,
                    "object-spec.invalid-arg-type",
                    format!("{class_symbol} creation argument has the wrong numeric type"),
                );
            }
        },
        None => json!(0),
    };
    let mut params = Map::new();
    params.insert("value".to_owned(), value);
    success(
        input,
        display_text,
        class_symbol,
        creation_args,
        candidate,
        params,
        stored_value_ports(spec.port_type),
    )
}

fn resolve_audio_object(
    input: &str,
    display_text: String,
    class_symbol: &str,
    creation_args: Vec<ObjectSpecAtom>,
    candidate: &ObjectRegistryCandidate,
) -> ObjectSpecResolution {
    let kind = candidate.kind.as_str();
    match kind {
        "object.core.audio.sig" => resolve_audio_number_param(
            input,
            display_text,
            class_symbol,
            creation_args,
            candidate,
            AudioNumberParamSpec {
                param_key: "value",
                default_value: 0.0,
                ports: audio_sig_ports(),
            },
        ),
        "object.core.audio.osc" => resolve_audio_number_param(
            input,
            display_text,
            class_symbol,
            creation_args,
            candidate,
            AudioNumberParamSpec {
                param_key: "frequency",
                default_value: 440.0,
                ports: audio_osc_ports(),
            },
        ),
        "object.core.audio.operator.mul" => {
            if !creation_args.is_empty() {
                return failure(
                    input,
                    display_text,
                    class_symbol,
                    creation_args,
                    "object-spec.invalid-arg-count",
                    "*~ accepts no creation arguments in the current Runtime audio substrate",
                );
            }
            success(
                input,
                display_text,
                class_symbol,
                creation_args,
                candidate,
                Map::new(),
                audio_binary_ports(),
            )
        }
        "object.core.audio.input" | "object.core.audio.output" => {
            if !creation_args.is_empty() {
                return failure(
                    input,
                    display_text,
                    class_symbol,
                    creation_args,
                    "object-spec.invalid-arg-count",
                    format!("{class_symbol} accepts no creation arguments"),
                );
            }
            let ports = if kind == "object.core.audio.input" {
                audio_input_ports()
            } else {
                audio_output_ports()
            };
            success(
                input,
                display_text,
                class_symbol,
                creation_args,
                candidate,
                Map::new(),
                ports,
            )
        }
        _ => unreachable!("audio object resolver received unknown kind"),
    }
}

struct AudioNumberParamSpec {
    param_key: &'static str,
    default_value: f64,
    ports: Vec<ObjectSpecPort>,
}

fn resolve_audio_number_param(
    input: &str,
    display_text: String,
    class_symbol: &str,
    creation_args: Vec<ObjectSpecAtom>,
    candidate: &ObjectRegistryCandidate,
    spec: AudioNumberParamSpec,
) -> ObjectSpecResolution {
    if creation_args.len() > 1 {
        return failure(
            input,
            display_text,
            class_symbol,
            creation_args,
            "object-spec.invalid-arg-count",
            format!("{class_symbol} accepts at most one creation argument"),
        );
    }
    let value = match creation_args.first() {
        Some(arg) => match numeric_value(arg) {
            Some(value) => value,
            None => {
                return failure(
                    input,
                    display_text,
                    class_symbol,
                    creation_args,
                    "object-spec.invalid-arg-type",
                    format!("{class_symbol} creation argument must be numeric"),
                );
            }
        },
        None => spec.default_value,
    };
    let mut params = Map::new();
    insert_number(&mut params, spec.param_key, value);
    success(
        input,
        display_text,
        class_symbol,
        creation_args,
        candidate,
        params,
        spec.ports,
    )
}

fn resolve_named_ref_object(
    input: &str,
    display_text: String,
    class_symbol: &str,
    creation_args: Vec<ObjectSpecAtom>,
    candidate: &ObjectRegistryCandidate,
    param_key: &'static str,
    count_message: &'static str,
) -> ObjectSpecResolution {
    if creation_args.len() != 1 {
        return failure(
            input,
            display_text,
            class_symbol,
            creation_args,
            "object-spec.invalid-arg-count",
            count_message,
        );
    }
    let Some(reference) = symbol_value(&creation_args[0]) else {
        return failure(
            input,
            display_text,
            class_symbol,
            creation_args,
            "object-spec.invalid-arg-type",
            format!("{class_symbol} reference argument must be a symbol"),
        );
    };
    let mut params = Map::new();
    params.insert(param_key.to_owned(), Value::String(reference));
    success(
        input,
        display_text,
        class_symbol,
        creation_args,
        candidate,
        params,
        Vec::new(),
    )
}

fn resolve_optional_named_ref_object(
    input: &str,
    display_text: String,
    class_symbol: &str,
    creation_args: Vec<ObjectSpecAtom>,
    candidate: &ObjectRegistryCandidate,
    param_key: &'static str,
) -> ObjectSpecResolution {
    if creation_args.len() > 1 {
        return failure(
            input,
            display_text,
            class_symbol,
            creation_args,
            "object-spec.invalid-arg-count",
            format!("{class_symbol} accepts at most one creation argument"),
        );
    }
    let mut params = Map::new();
    if let Some(arg) = creation_args.first() {
        let Some(reference) = symbol_value(arg) else {
            return failure(
                input,
                display_text,
                class_symbol,
                creation_args,
                "object-spec.invalid-arg-type",
                format!("{class_symbol} reference argument must be a symbol"),
            );
        };
        params.insert(param_key.to_owned(), Value::String(reference));
    }
    success(
        input,
        display_text,
        class_symbol,
        creation_args,
        candidate,
        params,
        Vec::new(),
    )
}

fn success(
    input: &str,
    display_text: String,
    class_symbol: &str,
    creation_args: Vec<ObjectSpecAtom>,
    candidate: &ObjectRegistryCandidate,
    params: Map<String, Value>,
    instance_ports: Vec<ObjectSpecPort>,
) -> ObjectSpecResolution {
    let summary = candidate.summary();
    ObjectSpecResolution {
        input: input.to_owned(),
        display_text,
        class_symbol: class_symbol.to_owned(),
        creation_args,
        resolved_kind: Some(candidate.kind.clone()),
        resolved_kind_version: Some(candidate.kind_version.clone()),
        params,
        instance_ports,
        candidates: vec![summary],
        diagnostics: Vec::new(),
    }
}

pub(super) fn failure(
    input: &str,
    display_text: String,
    class_symbol: &str,
    creation_args: Vec<ObjectSpecAtom>,
    code: impl Into<String>,
    message: impl Into<String>,
) -> ObjectSpecResolution {
    failure_with_candidates(
        input,
        display_text,
        class_symbol,
        creation_args,
        Vec::new(),
        code,
        message,
    )
}

fn failure_with_candidates(
    input: &str,
    display_text: String,
    class_symbol: &str,
    creation_args: Vec<ObjectSpecAtom>,
    candidates: Vec<ObjectSpecCandidateSummary>,
    code: impl Into<String>,
    message: impl Into<String>,
) -> ObjectSpecResolution {
    ObjectSpecResolution {
        input: input.to_owned(),
        display_text,
        class_symbol: class_symbol.to_owned(),
        creation_args,
        resolved_kind: None,
        resolved_kind_version: None,
        params: Map::new(),
        instance_ports: Vec::new(),
        candidates,
        diagnostics: vec![ObjectSpecDiagnostic {
            code: code.into(),
            message: message.into(),
        }],
    }
}

fn numeric_value(atom: &ObjectSpecAtom) -> Option<f64> {
    match atom {
        ObjectSpecAtom::Float(value) => Some(*value),
        ObjectSpecAtom::Int(value) => Some(*value as f64),
        ObjectSpecAtom::Bool(_) | ObjectSpecAtom::Symbol(_) => None,
    }
}

fn integer_value(atom: &ObjectSpecAtom) -> Option<i64> {
    match atom {
        ObjectSpecAtom::Int(value) => Some(*value),
        ObjectSpecAtom::Float(_) | ObjectSpecAtom::Bool(_) | ObjectSpecAtom::Symbol(_) => None,
    }
}

fn unsigned_value(atom: &ObjectSpecAtom) -> Option<u64> {
    match atom {
        ObjectSpecAtom::Int(value) if *value >= 0 => Some(*value as u64),
        ObjectSpecAtom::Float(_) | ObjectSpecAtom::Bool(_) | ObjectSpecAtom::Symbol(_) => None,
        ObjectSpecAtom::Int(_) => None,
    }
}

fn symbol_value(atom: &ObjectSpecAtom) -> Option<String> {
    match atom {
        ObjectSpecAtom::Symbol(value) if !value.is_empty() => Some(value.clone()),
        ObjectSpecAtom::Float(_) | ObjectSpecAtom::Int(_) | ObjectSpecAtom::Bool(_) => None,
        ObjectSpecAtom::Symbol(_) => None,
    }
}

fn atom_display_text(atom: &ObjectSpecAtom) -> String {
    match atom {
        ObjectSpecAtom::Float(value) => value.to_string(),
        ObjectSpecAtom::Int(value) => value.to_string(),
        ObjectSpecAtom::Bool(value) => value.to_string(),
        ObjectSpecAtom::Symbol(value) => value.clone(),
    }
}

fn insert_number(params: &mut Map<String, Value>, key: &str, value: f64) {
    params.insert(key.to_owned(), json!(value));
}

pub(super) fn unsupported_first_party_audio_message(class_symbol: &str) -> Option<&'static str> {
    match class_symbol {
        "+~"
        | "-~"
        | "/~"
        | "object.core.audio.operator.add"
        | "object.core.audio.operator.sub"
        | "object.core.audio.operator.div" => {
            Some("audio add/sub/div aliases are not executable in the current Runtime substrate")
        }
        "sqrt~" | "object.core.audio.operator.sqrt" => {
            Some("audio sqrt is not executable in the current Runtime substrate")
        }
        "phasor~" | "object.core.audio.phasor" => {
            Some("audio phasor is not executable in the current Runtime substrate")
        }
        _ => None,
    }
}
