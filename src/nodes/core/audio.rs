use crate::nodes::CoreNodeConstructor;

use super::CoreNodeDescriptor;

pub(super) static SIGNAL: CoreNodeDescriptor = CoreNodeDescriptor::new(
    "object.core.audio.sig",
    "Signal",
    &["sig~", "object.core.audio.sig"],
    CoreNodeConstructor::Audio,
);

pub(super) static OSCILLATOR: CoreNodeDescriptor = CoreNodeDescriptor::new(
    "object.core.audio.osc",
    "Oscillator",
    &["osc~", "object.core.audio.osc"],
    CoreNodeConstructor::Audio,
);

pub(super) static MULTIPLY: CoreNodeDescriptor = CoreNodeDescriptor::new(
    "object.core.audio.operator.mul",
    "Audio Multiply",
    &["*~", "object.core.audio.operator.mul"],
    CoreNodeConstructor::Audio,
);

pub(super) static INPUT: CoreNodeDescriptor = CoreNodeDescriptor::new(
    "object.core.audio.input",
    "Audio Input",
    &["adc~", "object.core.audio.input"],
    CoreNodeConstructor::Audio,
);

pub(super) static OUTPUT: CoreNodeDescriptor = CoreNodeDescriptor::new(
    "object.core.audio.output",
    "Audio Output",
    &["dac~", "object.core.audio.output"],
    CoreNodeConstructor::Audio,
);
