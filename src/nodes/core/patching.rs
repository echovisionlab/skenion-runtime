use crate::nodes::CoreNodeConstructor;

use super::CoreNodeDescriptor;

pub(super) static SUBPATCH: CoreNodeDescriptor = CoreNodeDescriptor::new(
    "object.core.subpatch",
    "Subpatch",
    &["p", "object.core.subpatch"],
    CoreNodeConstructor::Subpatch,
);

pub(super) static INLET: CoreNodeDescriptor = CoreNodeDescriptor::new(
    "object.core.inlet",
    "Inlet",
    &["inlet", "object.core.inlet"],
    CoreNodeConstructor::BoundaryPort,
);

pub(super) static OUTLET: CoreNodeDescriptor = CoreNodeDescriptor::new(
    "object.core.outlet",
    "Outlet",
    &["outlet", "object.core.outlet"],
    CoreNodeConstructor::BoundaryPort,
);
