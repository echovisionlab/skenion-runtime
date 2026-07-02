use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::{Value, json};

use super::RealtimeCommandDispatch;
use super::control_input::apply_control_input;
use super::protocol::{
    EVENT_CONTROL_EMITTED, FRAME_NODE_INPUT, RUNTIME_REALTIME_SCHEMA,
    RUNTIME_REALTIME_SCHEMA_VERSION,
};
use super::state::{RememberAckInput, sync_required_issue};
use super::wire::{
    RuntimeRealtimeConnectionIdentity, RuntimeRealtimeEnvelope, RuntimeRealtimeIssue,
};
use super::{command_ack_with_payload, mark_ack_payload_cached};
use crate::runtime_time::created_at_now;
use crate::{
    ControlValue, RuntimeControlEmission, RuntimeControlEventRequest, RuntimeControlEventResponse,
    RuntimeIssue, RuntimeSessionRecord,
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct NodeInputPayload {
    inputs: Vec<NodeInputRequest>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct NodeInputRequest {
    node_id: String,
    port_id: String,
    message: crate::ControlMessage,
}

struct AppliedNodeInput {
    request: RuntimeControlEventRequest,
    response: RuntimeControlEventResponse,
    changed_values: BTreeMap<String, ControlValue>,
}

pub(super) fn handle_node_input(
    record: &RuntimeSessionRecord,
    identity: &RuntimeRealtimeConnectionIdentity,
    frame: RuntimeRealtimeEnvelope,
) -> Result<RealtimeCommandDispatch, RuntimeRealtimeIssue> {
    if frame
        .command_id
        .as_ref()
        .is_none_or(|value| value.trim().is_empty())
    {
        return Err(sync_required_issue(
            "realtime.command.command-id-required",
            "node.input requires commandId",
            None,
        ));
    }
    let idempotency_key = frame.idempotency_key.clone().ok_or_else(|| {
        sync_required_issue(
            "realtime.command.idempotency-key-required",
            "node.input requires idempotencyKey",
            None,
        )
    })?;
    if let Some(cached) =
        record
            .realtime
            .cached_command_result(identity, &frame.message_type, &idempotency_key)
    {
        let mut payload = mark_ack_payload_cached(cached.ack_payload);
        if let Some(object) = payload.as_object_mut() {
            object.insert("eventCursor".to_owned(), Value::String(cached.event_cursor));
        }
        return Ok(RealtimeCommandDispatch {
            ack: command_ack_with_payload(record, identity, &frame, payload),
            sender_events: cached.emitted_results,
            broadcast_events: Vec::new(),
        });
    }

    let payload =
        serde_json::from_value::<NodeInputPayload>(frame.payload.clone()).map_err(|error| {
            sync_required_issue(
                "realtime.node-input.invalid-payload",
                format!("invalid node.input payload: {error}"),
                None,
            )
        })?;
    if payload.inputs.is_empty() {
        return Err(sync_required_issue(
            "realtime.node-input.inputs-required",
            "node.input payload inputs must not be empty",
            None,
        ));
    }

    let sequence = record.realtime.next_event_sequence();
    let cursor = record.realtime.cursor_for(sequence);
    let applied = payload
        .inputs
        .into_iter()
        .map(|input| {
            let request = RuntimeControlEventRequest {
                node_id: input.node_id,
                port_id: input.port_id,
                message: input.message,
            };
            let (response, changed_values, request) = apply_control_input(record, request);
            AppliedNodeInput {
                request,
                response,
                changed_values,
            }
        })
        .collect::<Vec<_>>();

    let accepted = applied.iter().all(|input| input.response.ok);
    let issues = realtime_issue_payloads(
        applied
            .iter()
            .flat_map(|input| input.response.issues.iter()),
    );
    let node_results = applied
        .iter()
        .enumerate()
        .map(|(index, input)| node_input_result(index, input))
        .collect::<Vec<_>>();
    let ack = command_ack_with_payload(
        record,
        identity,
        &frame,
        json!({
            "status": if accepted { "accepted" } else { "rejected" },
            "accepted": accepted,
            "applied": false,
            "conflict": false,
            "cached": false,
            "kind": FRAME_NODE_INPUT,
            "commandId": frame.command_id.clone().unwrap_or_else(|| frame.message_id.clone()),
            "correlationId": frame.correlation_id.clone().unwrap_or_else(|| frame.message_id.clone()),
            "idempotencyKey": frame.idempotency_key,
            "eventCursor": cursor,
            "node": { "inputs": node_results },
            "issues": issues,
        }),
    );
    let event = control_emitted_event(record, identity, &frame, &applied, sequence, &cursor);
    let emitted_results = event.iter().cloned().collect::<Vec<_>>();
    record.realtime.remember_ack(RememberAckInput {
        identity,
        message_type: &frame.message_type,
        idempotency_key: &idempotency_key,
        event_cursor: &cursor,
        event_sequence: sequence,
        ack_payload: ack.payload.clone(),
        emitted_results,
    });

    Ok(RealtimeCommandDispatch {
        ack,
        sender_events: Vec::new(),
        broadcast_events: event.into_iter().collect(),
    })
}

fn node_input_result(index: usize, input: &AppliedNodeInput) -> Value {
    json!({
        "index": index,
        "nodeId": input.request.node_id,
        "portId": input.request.port_id,
        "message": input.request.message,
        "accepted": input.response.ok,
        "changed": input.response.changed,
        "controlRevision": input.response.control_revision,
        "events": input.response.emitted,
        "issues": realtime_issue_payloads(input.response.issues.iter()),
    })
}

fn control_emitted_event(
    record: &RuntimeSessionRecord,
    identity: &RuntimeRealtimeConnectionIdentity,
    frame: &RuntimeRealtimeEnvelope,
    applied: &[AppliedNodeInput],
    sequence: u64,
    cursor: &str,
) -> Option<RuntimeRealtimeEnvelope> {
    let events = applied
        .iter()
        .flat_map(|input| input.response.emitted.iter().cloned())
        .collect::<Vec<RuntimeControlEmission>>();
    let values = applied
        .iter()
        .flat_map(|input| input.changed_values.iter())
        .map(|(node_id, value)| (node_id.clone(), value.clone()))
        .collect::<BTreeMap<_, _>>();

    if events.is_empty() && values.is_empty() {
        return None;
    }

    Some(RuntimeRealtimeEnvelope {
        schema: RUNTIME_REALTIME_SCHEMA.to_owned(),
        schema_version: RUNTIME_REALTIME_SCHEMA_VERSION.to_owned(),
        message_type: EVENT_CONTROL_EMITTED.to_owned(),
        message_id: format!("{}_control_{sequence:06}", record.id),
        session_id: record.id.clone(),
        connection_id: Some(identity.connection_id.clone()),
        client_id: Some(identity.client_id.clone()),
        window_id: Some(identity.window_id.clone()),
        command_id: frame
            .command_id
            .clone()
            .or_else(|| Some(frame.message_id.clone())),
        correlation_id: frame
            .correlation_id
            .clone()
            .or_else(|| Some(frame.message_id.clone())),
        idempotency_key: frame.idempotency_key.clone(),
        sequence: Some(sequence),
        cursor: Some(cursor.to_owned()),
        created_at: Some(created_at_now()),
        payload: json!({
            "commandId": frame.command_id.clone().unwrap_or_else(|| frame.message_id.clone()),
            "correlationId": frame.correlation_id.clone().unwrap_or_else(|| frame.message_id.clone()),
            "idempotencyKey": frame.idempotency_key,
            "controlSequence": sequence,
            "controlRevision": applied.iter().rev().find_map(|input| input.response.control_revision),
            "changed": applied.iter().any(|input| input.response.changed),
            "events": events,
            "values": if values.is_empty() { Value::Null } else { json!(values) },
            "issues": realtime_issue_payloads(
                applied
                    .iter()
                    .flat_map(|input| input.response.issues.iter())
            ),
            "replayed": false,
        }),
    })
}

fn realtime_issue_payloads<'a>(issues: impl IntoIterator<Item = &'a RuntimeIssue>) -> Vec<Value> {
    issues
        .into_iter()
        .map(|issue| {
            let mut value = json!({
                "severity": issue.severity,
                "code": issue
                    .code
                    .as_deref()
                    .unwrap_or("runtime.control.issue"),
                "message": issue.message,
            });
            if let Some(details) = issue.details.clone()
                && let Some(object) = value.as_object_mut()
            {
                object.insert("details".to_owned(), details);
            }
            value
        })
        .collect()
}
