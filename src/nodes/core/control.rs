use crate::nodes::CoreNodeConstructor;

use super::CoreNodeDescriptor;

pub(super) static ADD: CoreNodeDescriptor = CoreNodeDescriptor::new(
    "object.core.operator.add",
    "Add",
    &["+", "add", "object.core.operator.add"],
    CoreNodeConstructor::ControlOperator,
);

pub(super) static SUBTRACT: CoreNodeDescriptor = CoreNodeDescriptor::new(
    "object.core.operator.sub",
    "Subtract",
    &["-", "sub", "object.core.operator.sub"],
    CoreNodeConstructor::ControlOperator,
);

pub(super) static MULTIPLY: CoreNodeDescriptor = CoreNodeDescriptor::new(
    "object.core.operator.mul",
    "Multiply",
    &["*", "mul", "object.core.operator.mul"],
    CoreNodeConstructor::ControlOperator,
);

pub(super) static DIVIDE: CoreNodeDescriptor = CoreNodeDescriptor::new(
    "object.core.operator.div",
    "Divide",
    &["/", "div", "object.core.operator.div"],
    CoreNodeConstructor::ControlOperator,
);

pub(super) static POWER: CoreNodeDescriptor = CoreNodeDescriptor::new(
    "object.core.operator.pow",
    "Power",
    &["pow", "object.core.operator.pow"],
    CoreNodeConstructor::ControlOperator,
);

pub(super) static MINIMUM: CoreNodeDescriptor = CoreNodeDescriptor::new(
    "object.core.operator.min",
    "Minimum",
    &["min", "object.core.operator.min"],
    CoreNodeConstructor::ControlOperator,
);

pub(super) static MAXIMUM: CoreNodeDescriptor = CoreNodeDescriptor::new(
    "object.core.operator.max",
    "Maximum",
    &["max", "object.core.operator.max"],
    CoreNodeConstructor::ControlOperator,
);

pub(super) static SQUARE_ROOT: CoreNodeDescriptor = CoreNodeDescriptor::new(
    "object.core.operator.sqrt",
    "Square Root",
    &["sqrt", "object.core.operator.sqrt"],
    CoreNodeConstructor::ControlOperator,
);

pub(super) static FLOAT: CoreNodeDescriptor = CoreNodeDescriptor::new(
    "object.core.float",
    "Float",
    &["f", "float", "number", "object.core.float"],
    CoreNodeConstructor::ControlValue,
);

pub(super) static INTEGER: CoreNodeDescriptor = CoreNodeDescriptor::new(
    "object.core.int",
    "Integer",
    &["i", "int", "object.core.int"],
    CoreNodeConstructor::ControlValue,
);

pub(super) static UNSIGNED_INTEGER: CoreNodeDescriptor = CoreNodeDescriptor::new(
    "object.core.uint",
    "Unsigned Integer",
    &["u", "uint", "object.core.uint"],
    CoreNodeConstructor::ControlValue,
);

pub(super) static BANG: CoreNodeDescriptor = CoreNodeDescriptor::new(
    "object.core.bang",
    "Bang",
    &["b", "bang", "object.core.bang"],
    CoreNodeConstructor::ControlValue,
);

pub(super) static MESSAGE: CoreNodeDescriptor = CoreNodeDescriptor::new(
    "object.core.message",
    "Message",
    &["msg", "message", "object.core.message"],
    CoreNodeConstructor::ControlValue,
);

pub(super) static COMMENT: CoreNodeDescriptor = CoreNodeDescriptor::new(
    "object.core.comment",
    "Comment",
    &["comment", "object.core.comment"],
    CoreNodeConstructor::ControlValue,
);
