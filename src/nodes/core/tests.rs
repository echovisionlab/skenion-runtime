use super::*;
use crate::nodes::{CoreNodeConstructor, CoreNodeImplementation};

static TEST_ALIASES: &[&str] = &["test", "object.test.alias"];

#[test]
fn descriptor_exposes_core_node_metadata() {
    let descriptor = CoreNodeDescriptor::new(
        "object.test.node",
        "Test Node",
        TEST_ALIASES,
        CoreNodeConstructor::Audio,
    );

    assert_eq!(descriptor.kind(), "object.test.node");
    assert_eq!(descriptor.display_name(), "Test Node");
    assert_eq!(descriptor.aliases(), TEST_ALIASES);
    assert_eq!(descriptor.constructor(), CoreNodeConstructor::Audio);
    assert_eq!(descriptor.catalog_category(), "Core Audio");
}

#[test]
fn descriptor_maps_non_audio_nodes_to_core_catalog_category() {
    for constructor in [
        CoreNodeConstructor::ControlOperator,
        CoreNodeConstructor::ControlValue,
        CoreNodeConstructor::Subpatch,
        CoreNodeConstructor::BoundaryPort,
    ] {
        let descriptor = CoreNodeDescriptor::new("object.test.node", "Test Node", &[], constructor);
        assert_eq!(descriptor.catalog_category(), "Core");
    }
}
