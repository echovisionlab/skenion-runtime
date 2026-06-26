use serde_json::{Map, Value, json};

const CURRENT_KIND_VERSION: &str = "0.1.0";

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ObjectTextResolution {
    pub(crate) input: String,
    pub(crate) display_text: String,
    pub(crate) class_symbol: String,
    pub(crate) creation_args: Vec<ObjectTextAtom>,
    pub(crate) resolved_kind: Option<String>,
    pub(crate) resolved_kind_version: Option<String>,
    pub(crate) params: Map<String, Value>,
    pub(crate) instance_ports: Vec<ObjectTextPort>,
    pub(crate) diagnostics: Vec<ObjectTextDiagnostic>,
}

impl ObjectTextResolution {
    pub(crate) fn ok(&self) -> bool {
        self.diagnostics.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ObjectTextAtom {
    Float(f64),
    Int(i64),
    Bool(bool),
    Symbol(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ObjectTextDiagnostic {
    pub(crate) code: String,
    pub(crate) message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ObjectTextPort {
    pub(crate) id: String,
    pub(crate) direction: ObjectTextPortDirection,
    pub(crate) port_type: String,
    pub(crate) rate: ObjectTextPortRate,
    pub(crate) activation: Option<ObjectTextPortActivation>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ObjectTextPortDirection {
    Input,
    Output,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ObjectTextPortRate {
    Event,
    Control,
    Audio,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ObjectTextPortActivation {
    Trigger,
    Latched,
}

pub(crate) fn resolve_object_text_v01(input: &str) -> ObjectTextResolution {
    let display_text = match normalize_input(input) {
        Ok(display_text) => display_text,
        Err((display_text, message)) => {
            return failure(
                input,
                display_text,
                "<invalid>",
                Vec::new(),
                "object-text.invalid-syntax",
                message,
            );
        }
    };
    let tokens = tokenize(&display_text);
    let Some((class_symbol, arg_tokens)) = tokens.split_first() else {
        return failure(
            input,
            "<empty>".to_owned(),
            "<empty>",
            Vec::new(),
            "object-text.empty",
            "object text must contain a class symbol",
        );
    };
    let creation_args = arg_tokens
        .iter()
        .map(|token| parse_atom(token))
        .collect::<Vec<_>>();

    if is_payload_identity_kind(class_symbol) {
        return failure(
            input,
            display_text,
            class_symbol,
            creation_args,
            "object-text.payload-identity",
            format!("{class_symbol} is a payload identity, not an executable object"),
        );
    }

    if let Some(message) = unsupported_first_party_audio_message(class_symbol) {
        return failure(
            input,
            display_text,
            class_symbol,
            creation_args,
            "object-text.unsupported-first-party",
            message,
        );
    }

    if let Some(kind) = control_operator_kind(class_symbol) {
        return resolve_control_operator(input, display_text, class_symbol, creation_args, kind);
    }

    if let Some(kind) = control_value_kind(class_symbol) {
        return resolve_control_value(input, display_text, class_symbol, creation_args, kind);
    }

    if let Some(kind) = audio_object_kind(class_symbol) {
        return resolve_audio_object(input, display_text, class_symbol, creation_args, kind);
    }

    if matches!(class_symbol.as_str(), "p" | "core.subpatch") {
        return resolve_named_ref_object(
            input,
            display_text,
            class_symbol,
            creation_args,
            "core.subpatch",
            "patchRef",
            "subpatch object text requires exactly one patch reference",
        );
    }

    if matches!(class_symbol.as_str(), "inlet" | "core.inlet") {
        return resolve_optional_named_ref_object(
            input,
            display_text,
            class_symbol,
            creation_args,
            "core.inlet",
            "portId",
        );
    }

    if matches!(class_symbol.as_str(), "outlet" | "core.outlet") {
        return resolve_optional_named_ref_object(
            input,
            display_text,
            class_symbol,
            creation_args,
            "core.outlet",
            "portId",
        );
    }

    failure(
        input,
        display_text,
        class_symbol,
        creation_args,
        "object-text.unresolved",
        format!("{class_symbol} is not available in the local Runtime object resolver"),
    )
}

pub(crate) fn is_payload_identity_kind(kind: &str) -> bool {
    matches!(
        kind,
        "value"
            | "data"
            | "payload"
            | "bool"
            | "string"
            | "core.bool"
            | "core.string"
            | "control.message.any"
            | "event.bang"
            | "asset.video"
            | "asset.image"
            | "asset.audio"
            | "gpu.texture2d"
    ) || kind.starts_with("value.")
        || kind.starts_with("data.")
        || kind.starts_with("payload.")
        || kind.starts_with("control.")
}

fn resolve_control_operator(
    input: &str,
    display_text: String,
    class_symbol: &str,
    creation_args: Vec<ObjectTextAtom>,
    kind: &'static str,
) -> ObjectTextResolution {
    if kind == "core.operator.sqrt" {
        if !creation_args.is_empty() {
            return failure(
                input,
                display_text,
                class_symbol,
                creation_args,
                "object-text.invalid-arg-count",
                "sqrt accepts no creation arguments",
            );
        }
        return success(
            input,
            display_text,
            class_symbol,
            creation_args,
            kind,
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
            "object-text.invalid-arg-count",
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
                    "object-text.invalid-arg-type",
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
        kind,
        params,
        control_operator_ports(),
    )
}

fn resolve_control_value(
    input: &str,
    display_text: String,
    class_symbol: &str,
    creation_args: Vec<ObjectTextAtom>,
    kind: &'static str,
) -> ObjectTextResolution {
    match kind {
        "core.bang" => {
            if !creation_args.is_empty() {
                return failure(
                    input,
                    display_text,
                    class_symbol,
                    creation_args,
                    "object-text.invalid-arg-count",
                    format!("{class_symbol} accepts no creation arguments"),
                );
            }
            success(
                input,
                display_text,
                class_symbol,
                creation_args,
                kind,
                Map::new(),
                bang_ports(),
            )
        }
        "core.message" | "core.comment" => {
            let text = creation_args
                .iter()
                .map(atom_display_text)
                .collect::<Vec<_>>()
                .join(" ");
            let mut params = Map::new();
            params.insert("text".to_owned(), Value::String(text));
            let ports = if kind == "core.message" {
                message_ports()
            } else {
                comment_ports()
            };
            success(
                input,
                display_text,
                class_symbol,
                creation_args,
                kind,
                params,
                ports,
            )
        }
        "core.float" => resolve_number_value(
            input,
            display_text,
            class_symbol,
            creation_args,
            kind,
            "control.number.float",
            numeric_value,
            |value| json!(value),
        ),
        "core.int" => resolve_number_value(
            input,
            display_text,
            class_symbol,
            creation_args,
            kind,
            "control.number.int",
            integer_value,
            |value| json!(value),
        ),
        "core.uint" => resolve_number_value(
            input,
            display_text,
            class_symbol,
            creation_args,
            kind,
            "control.number.uint",
            unsigned_value,
            |value| json!(value),
        ),
        _ => unreachable!("control value resolver received unknown kind"),
    }
}

fn resolve_number_value<T>(
    input: &str,
    display_text: String,
    class_symbol: &str,
    creation_args: Vec<ObjectTextAtom>,
    kind: &'static str,
    port_type: &'static str,
    coerce: fn(&ObjectTextAtom) -> Option<T>,
    to_json: fn(T) -> Value,
) -> ObjectTextResolution {
    if creation_args.len() > 1 {
        return failure(
            input,
            display_text,
            class_symbol,
            creation_args,
            "object-text.invalid-arg-count",
            format!("{class_symbol} accepts at most one creation argument"),
        );
    }

    let value = match creation_args.first() {
        Some(arg) => match coerce(arg) {
            Some(value) => to_json(value),
            None => {
                return failure(
                    input,
                    display_text,
                    class_symbol,
                    creation_args,
                    "object-text.invalid-arg-type",
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
        kind,
        params,
        stored_value_ports(port_type),
    )
}

fn resolve_audio_object(
    input: &str,
    display_text: String,
    class_symbol: &str,
    creation_args: Vec<ObjectTextAtom>,
    kind: &'static str,
) -> ObjectTextResolution {
    match kind {
        "audio.sig" => resolve_audio_number_param(
            input,
            display_text,
            class_symbol,
            creation_args,
            kind,
            "value",
            0.0,
            audio_sig_ports(),
        ),
        "audio.osc" => resolve_audio_number_param(
            input,
            display_text,
            class_symbol,
            creation_args,
            kind,
            "frequency",
            440.0,
            audio_osc_ports(),
        ),
        "audio.operator.mul" => {
            if !creation_args.is_empty() {
                return failure(
                    input,
                    display_text,
                    class_symbol,
                    creation_args,
                    "object-text.invalid-arg-count",
                    "*~ accepts no creation arguments in the current Runtime audio substrate",
                );
            }
            success(
                input,
                display_text,
                class_symbol,
                creation_args,
                kind,
                Map::new(),
                audio_binary_ports(),
            )
        }
        "audio.input" | "audio.output" => {
            if !creation_args.is_empty() {
                return failure(
                    input,
                    display_text,
                    class_symbol,
                    creation_args,
                    "object-text.invalid-arg-count",
                    format!("{class_symbol} accepts no creation arguments"),
                );
            }
            let ports = if kind == "audio.input" {
                audio_input_ports()
            } else {
                audio_output_ports()
            };
            success(
                input,
                display_text,
                class_symbol,
                creation_args,
                kind,
                Map::new(),
                ports,
            )
        }
        _ => unreachable!("audio object resolver received unknown kind"),
    }
}

fn resolve_audio_number_param(
    input: &str,
    display_text: String,
    class_symbol: &str,
    creation_args: Vec<ObjectTextAtom>,
    kind: &'static str,
    param_key: &'static str,
    default_value: f64,
    ports: Vec<ObjectTextPort>,
) -> ObjectTextResolution {
    if creation_args.len() > 1 {
        return failure(
            input,
            display_text,
            class_symbol,
            creation_args,
            "object-text.invalid-arg-count",
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
                    "object-text.invalid-arg-type",
                    format!("{class_symbol} creation argument must be numeric"),
                );
            }
        },
        None => default_value,
    };
    let mut params = Map::new();
    insert_number(&mut params, param_key, value);
    success(
        input,
        display_text,
        class_symbol,
        creation_args,
        kind,
        params,
        ports,
    )
}

fn resolve_named_ref_object(
    input: &str,
    display_text: String,
    class_symbol: &str,
    creation_args: Vec<ObjectTextAtom>,
    kind: &'static str,
    param_key: &'static str,
    count_message: &'static str,
) -> ObjectTextResolution {
    if creation_args.len() != 1 {
        return failure(
            input,
            display_text,
            class_symbol,
            creation_args,
            "object-text.invalid-arg-count",
            count_message,
        );
    }
    let Some(reference) = symbol_value(&creation_args[0]) else {
        return failure(
            input,
            display_text,
            class_symbol,
            creation_args,
            "object-text.invalid-arg-type",
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
        kind,
        params,
        Vec::new(),
    )
}

fn resolve_optional_named_ref_object(
    input: &str,
    display_text: String,
    class_symbol: &str,
    creation_args: Vec<ObjectTextAtom>,
    kind: &'static str,
    param_key: &'static str,
) -> ObjectTextResolution {
    if creation_args.len() > 1 {
        return failure(
            input,
            display_text,
            class_symbol,
            creation_args,
            "object-text.invalid-arg-count",
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
                "object-text.invalid-arg-type",
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
        kind,
        params,
        Vec::new(),
    )
}

fn success(
    input: &str,
    display_text: String,
    class_symbol: &str,
    creation_args: Vec<ObjectTextAtom>,
    resolved_kind: &str,
    params: Map<String, Value>,
    instance_ports: Vec<ObjectTextPort>,
) -> ObjectTextResolution {
    ObjectTextResolution {
        input: input.to_owned(),
        display_text,
        class_symbol: class_symbol.to_owned(),
        creation_args,
        resolved_kind: Some(resolved_kind.to_owned()),
        resolved_kind_version: Some(CURRENT_KIND_VERSION.to_owned()),
        params,
        instance_ports,
        diagnostics: Vec::new(),
    }
}

fn failure(
    input: &str,
    display_text: String,
    class_symbol: &str,
    creation_args: Vec<ObjectTextAtom>,
    code: &str,
    message: impl Into<String>,
) -> ObjectTextResolution {
    ObjectTextResolution {
        input: input.to_owned(),
        display_text,
        class_symbol: class_symbol.to_owned(),
        creation_args,
        resolved_kind: None,
        resolved_kind_version: None,
        params: Map::new(),
        instance_ports: Vec::new(),
        diagnostics: vec![ObjectTextDiagnostic {
            code: code.to_owned(),
            message: message.into(),
        }],
    }
}

fn normalize_input(input: &str) -> Result<String, (String, String)> {
    let trimmed = input.trim();
    if trimmed.starts_with('[') || trimmed.ends_with(']') {
        if !(trimmed.starts_with('[') && trimmed.ends_with(']')) {
            return Err((
                trimmed.to_owned(),
                "object text brackets must be balanced".to_owned(),
            ));
        }
        return Ok(trimmed[1..trimmed.len() - 1].trim().to_owned());
    }
    Ok(trimmed.to_owned())
}

fn tokenize(display_text: &str) -> Vec<String> {
    display_text.split_whitespace().map(str::to_owned).collect()
}

fn parse_atom(token: &str) -> ObjectTextAtom {
    if token == "true" {
        return ObjectTextAtom::Bool(true);
    }
    if token == "false" {
        return ObjectTextAtom::Bool(false);
    }
    if is_integer_token(token)
        && let Ok(value) = token.parse::<i64>()
    {
        return ObjectTextAtom::Int(value);
    }
    if is_float_token(token)
        && let Ok(value) = token.parse::<f64>()
        && value.is_finite()
    {
        return ObjectTextAtom::Float(value);
    }
    ObjectTextAtom::Symbol(token.to_owned())
}

fn is_integer_token(token: &str) -> bool {
    let digits = token.strip_prefix(['+', '-']).unwrap_or(token);
    !digits.is_empty() && digits.chars().all(|character| character.is_ascii_digit())
}

fn is_float_token(token: &str) -> bool {
    token.contains('.') || token.contains('e') || token.contains('E')
}

fn numeric_value(atom: &ObjectTextAtom) -> Option<f64> {
    match atom {
        ObjectTextAtom::Float(value) => Some(*value),
        ObjectTextAtom::Int(value) => Some(*value as f64),
        ObjectTextAtom::Bool(_) | ObjectTextAtom::Symbol(_) => None,
    }
}

fn integer_value(atom: &ObjectTextAtom) -> Option<i64> {
    match atom {
        ObjectTextAtom::Int(value) => Some(*value),
        ObjectTextAtom::Float(_) | ObjectTextAtom::Bool(_) | ObjectTextAtom::Symbol(_) => None,
    }
}

fn unsigned_value(atom: &ObjectTextAtom) -> Option<u64> {
    match atom {
        ObjectTextAtom::Int(value) if *value >= 0 => Some(*value as u64),
        ObjectTextAtom::Float(_) | ObjectTextAtom::Bool(_) | ObjectTextAtom::Symbol(_) => None,
        ObjectTextAtom::Int(_) => None,
    }
}

fn symbol_value(atom: &ObjectTextAtom) -> Option<String> {
    match atom {
        ObjectTextAtom::Symbol(value) if !value.is_empty() => Some(value.clone()),
        ObjectTextAtom::Float(_) | ObjectTextAtom::Int(_) | ObjectTextAtom::Bool(_) => None,
        ObjectTextAtom::Symbol(_) => None,
    }
}

fn atom_display_text(atom: &ObjectTextAtom) -> String {
    match atom {
        ObjectTextAtom::Float(value) => value.to_string(),
        ObjectTextAtom::Int(value) => value.to_string(),
        ObjectTextAtom::Bool(value) => value.to_string(),
        ObjectTextAtom::Symbol(value) => value.clone(),
    }
}

fn insert_number(params: &mut Map<String, Value>, key: &str, value: f64) {
    params.insert(key.to_owned(), json!(value));
}

fn control_operator_kind(class_symbol: &str) -> Option<&'static str> {
    match class_symbol {
        "+" | "add" | "core.operator.add" => Some("core.operator.add"),
        "-" | "sub" | "core.operator.sub" => Some("core.operator.sub"),
        "*" | "mul" | "core.operator.mul" => Some("core.operator.mul"),
        "/" | "div" | "core.operator.div" => Some("core.operator.div"),
        "pow" | "core.operator.pow" => Some("core.operator.pow"),
        "min" | "core.operator.min" => Some("core.operator.min"),
        "max" | "core.operator.max" => Some("core.operator.max"),
        "sqrt" | "core.operator.sqrt" => Some("core.operator.sqrt"),
        _ => None,
    }
}

fn control_value_kind(class_symbol: &str) -> Option<&'static str> {
    match class_symbol {
        "f" | "float" | "number" | "core.float" => Some("core.float"),
        "i" | "int" | "core.int" => Some("core.int"),
        "u" | "uint" | "core.uint" => Some("core.uint"),
        "b" | "bang" | "core.bang" => Some("core.bang"),
        "msg" | "message" | "core.message" => Some("core.message"),
        "comment" | "core.comment" => Some("core.comment"),
        _ => None,
    }
}

fn audio_object_kind(class_symbol: &str) -> Option<&'static str> {
    match class_symbol {
        "sig~" | "audio.sig" => Some("audio.sig"),
        "osc~" | "audio.osc" => Some("audio.osc"),
        "*~" | "audio.operator.mul" => Some("audio.operator.mul"),
        "adc~" | "audio.input" => Some("audio.input"),
        "dac~" | "audio.output" => Some("audio.output"),
        _ => None,
    }
}

fn unsupported_first_party_audio_message(class_symbol: &str) -> Option<&'static str> {
    match class_symbol {
        "+~" | "-~" | "/~" | "audio.operator.add" | "audio.operator.sub" | "audio.operator.div" => {
            Some("audio add/sub/div aliases are not executable in the current Runtime substrate")
        }
        "sqrt~" | "audio.operator.sqrt" => {
            Some("audio sqrt is not executable in the current Runtime substrate")
        }
        "phasor~" | "audio.phasor" => {
            Some("audio phasor is not executable in the current Runtime substrate")
        }
        _ => None,
    }
}

fn input_port(
    id: &str,
    port_type: &str,
    rate: ObjectTextPortRate,
    activation: ObjectTextPortActivation,
) -> ObjectTextPort {
    ObjectTextPort {
        id: id.to_owned(),
        direction: ObjectTextPortDirection::Input,
        port_type: port_type.to_owned(),
        rate,
        activation: Some(activation),
    }
}

fn output_port(id: &str, port_type: &str, rate: ObjectTextPortRate) -> ObjectTextPort {
    ObjectTextPort {
        id: id.to_owned(),
        direction: ObjectTextPortDirection::Output,
        port_type: port_type.to_owned(),
        rate,
        activation: None,
    }
}

fn stored_value_ports(port_type: &str) -> Vec<ObjectTextPort> {
    vec![
        input_port(
            "in",
            "control.message.any",
            ObjectTextPortRate::Control,
            ObjectTextPortActivation::Trigger,
        ),
        input_port(
            "cold",
            port_type,
            ObjectTextPortRate::Control,
            ObjectTextPortActivation::Latched,
        ),
        output_port("value", port_type, ObjectTextPortRate::Control),
    ]
}

fn control_operator_ports() -> Vec<ObjectTextPort> {
    vec![
        input_port(
            "in",
            "control.number.float",
            ObjectTextPortRate::Control,
            ObjectTextPortActivation::Trigger,
        ),
        input_port(
            "right",
            "control.number.float",
            ObjectTextPortRate::Control,
            ObjectTextPortActivation::Latched,
        ),
        output_port("out", "control.number.float", ObjectTextPortRate::Control),
    ]
}

fn control_sqrt_ports() -> Vec<ObjectTextPort> {
    vec![
        input_port(
            "in",
            "control.number.float",
            ObjectTextPortRate::Control,
            ObjectTextPortActivation::Trigger,
        ),
        output_port("out", "control.number.float", ObjectTextPortRate::Control),
    ]
}

fn bang_ports() -> Vec<ObjectTextPort> {
    vec![
        input_port(
            "in",
            "control.message.any",
            ObjectTextPortRate::Control,
            ObjectTextPortActivation::Trigger,
        ),
        output_port("out", "event.bang", ObjectTextPortRate::Event),
    ]
}

fn message_ports() -> Vec<ObjectTextPort> {
    vec![
        input_port(
            "in",
            "control.message.any",
            ObjectTextPortRate::Control,
            ObjectTextPortActivation::Trigger,
        ),
        output_port("out", "control.message.any", ObjectTextPortRate::Control),
    ]
}

fn comment_ports() -> Vec<ObjectTextPort> {
    vec![input_port(
        "in",
        "control.message.any",
        ObjectTextPortRate::Control,
        ObjectTextPortActivation::Trigger,
    )]
}

fn audio_sig_ports() -> Vec<ObjectTextPort> {
    vec![
        input_port(
            "value",
            "control.number.float",
            ObjectTextPortRate::Control,
            ObjectTextPortActivation::Latched,
        ),
        output_port("out", "signal.audio", ObjectTextPortRate::Audio),
    ]
}

fn audio_osc_ports() -> Vec<ObjectTextPort> {
    vec![
        input_port(
            "frequency",
            "control.number.float",
            ObjectTextPortRate::Control,
            ObjectTextPortActivation::Latched,
        ),
        output_port("out", "signal.audio", ObjectTextPortRate::Audio),
    ]
}

fn audio_binary_ports() -> Vec<ObjectTextPort> {
    vec![
        input_port(
            "left",
            "signal.audio",
            ObjectTextPortRate::Audio,
            ObjectTextPortActivation::Latched,
        ),
        input_port(
            "right",
            "signal.audio",
            ObjectTextPortRate::Audio,
            ObjectTextPortActivation::Latched,
        ),
        output_port("out", "signal.audio", ObjectTextPortRate::Audio),
    ]
}

fn audio_input_ports() -> Vec<ObjectTextPort> {
    vec![
        output_port("left", "signal.audio", ObjectTextPortRate::Audio),
        output_port("right", "signal.audio", ObjectTextPortRate::Audio),
    ]
}

fn audio_output_ports() -> Vec<ObjectTextPort> {
    vec![
        input_port(
            "left",
            "signal.audio",
            ObjectTextPortRate::Audio,
            ObjectTextPortActivation::Latched,
        ),
        input_port(
            "right",
            "signal.audio",
            ObjectTextPortRate::Audio,
            ObjectTextPortActivation::Latched,
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_runtime_control_aliases_and_validates_args() {
        let add = resolve_object_text_v01("[+ 1e3]");
        assert!(add.ok());
        assert_eq!(add.display_text, "+ 1e3");
        assert_eq!(add.class_symbol, "+");
        assert_eq!(add.resolved_kind.as_deref(), Some("core.operator.add"));
        assert_eq!(add.resolved_kind_version.as_deref(), Some("0.1.0"));
        assert_eq!(add.params["right"], json!(1000.0));
        assert_eq!(add.instance_ports[0].id, "in");

        let sqrt = resolve_object_text_v01("sqrt 2");
        assert_eq!(sqrt.diagnostics[0].code, "object-text.invalid-arg-count");

        let invalid = resolve_object_text_v01("+ true");
        assert_eq!(invalid.diagnostics[0].code, "object-text.invalid-arg-type");
    }

    #[test]
    fn resolves_runtime_value_audio_and_subpatch_aliases() {
        let float = resolve_object_text_v01("f 0.25");
        assert!(float.ok());
        assert_eq!(float.resolved_kind.as_deref(), Some("core.float"));
        assert_eq!(float.params["value"], json!(0.25));

        let osc = resolve_object_text_v01("osc~ 220");
        assert!(osc.ok());
        assert_eq!(osc.resolved_kind.as_deref(), Some("audio.osc"));
        assert_eq!(osc.params["frequency"], json!(220.0));

        let mul = resolve_object_text_v01("*~");
        assert!(mul.ok());
        assert_eq!(mul.resolved_kind.as_deref(), Some("audio.operator.mul"));
        assert_eq!(mul.instance_ports.len(), 3);

        let scalar_mul = resolve_object_text_v01("*~ 0.5");
        assert_eq!(
            scalar_mul.diagnostics[0].code,
            "object-text.invalid-arg-count"
        );

        let unsupported = resolve_object_text_v01("+~");
        assert_eq!(
            unsupported.diagnostics[0].code,
            "object-text.unsupported-first-party"
        );

        let subpatch = resolve_object_text_v01("p voice");
        assert!(subpatch.ok());
        assert_eq!(subpatch.resolved_kind.as_deref(), Some("core.subpatch"));
        assert_eq!(subpatch.params["patchRef"], json!("voice"));
    }

    #[test]
    fn rejects_payload_identities_as_object_text() {
        for input in [
            "control.number.float",
            "bool",
            "event.bang",
            "gpu.texture2d",
        ] {
            let resolution = resolve_object_text_v01(input);
            assert_eq!(resolution.resolved_kind, None);
            assert_eq!(
                resolution.diagnostics[0].code,
                "object-text.payload-identity"
            );
        }
    }

    #[test]
    fn reports_unresolved_and_syntax_diagnostics_without_runtime_mapping() {
        let unresolved = resolve_object_text_v01("user.manipulator 1");
        assert_eq!(unresolved.diagnostics[0].code, "object-text.unresolved");

        let invalid = resolve_object_text_v01("[+ 1");
        assert_eq!(invalid.diagnostics[0].code, "object-text.invalid-syntax");
    }
}
