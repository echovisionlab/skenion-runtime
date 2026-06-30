use super::{CoreNodeConstructor, CoreNodeImplementation};

mod audio;
mod control;
mod patching;

pub(crate) fn first_party_core_nodes() -> &'static [&'static dyn CoreNodeImplementation] {
    FIRST_PARTY_CORE_NODES
}

static FIRST_PARTY_CORE_NODES: &[&dyn CoreNodeImplementation] = &[
    &control::ADD,
    &control::SUBTRACT,
    &control::MULTIPLY,
    &control::DIVIDE,
    &control::POWER,
    &control::MINIMUM,
    &control::MAXIMUM,
    &control::SQUARE_ROOT,
    &control::FLOAT,
    &control::INTEGER,
    &control::UNSIGNED_INTEGER,
    &control::BANG,
    &control::MESSAGE,
    &control::COMMENT,
    &audio::SIGNAL,
    &audio::OSCILLATOR,
    &audio::MULTIPLY,
    &audio::INPUT,
    &audio::OUTPUT,
    &patching::SUBPATCH,
    &patching::INLET,
    &patching::OUTLET,
];

pub(super) struct CoreNodeDescriptor {
    kind: &'static str,
    display_name: &'static str,
    aliases: &'static [&'static str],
    constructor: CoreNodeConstructor,
}

impl CoreNodeDescriptor {
    pub(super) const fn new(
        kind: &'static str,
        display_name: &'static str,
        aliases: &'static [&'static str],
        constructor: CoreNodeConstructor,
    ) -> Self {
        Self {
            kind,
            display_name,
            aliases,
            constructor,
        }
    }
}

impl CoreNodeImplementation for CoreNodeDescriptor {
    fn kind(&self) -> &'static str {
        self.kind
    }

    fn display_name(&self) -> &'static str {
        self.display_name
    }

    fn aliases(&self) -> &'static [&'static str] {
        self.aliases
    }

    fn constructor(&self) -> CoreNodeConstructor {
        self.constructor
    }
}
