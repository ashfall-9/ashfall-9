#![forbid(unsafe_code)]

use std::collections::HashSet;

use engine_core::{InterfaceVersion, QueueClass};
use serde::{Deserialize, Serialize};

pub const RENDER_GRAPH_VERSION: InterfaceVersion = InterfaceVersion::new("render_graph", 0, 1, 0);

#[repr(transparent)]
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct GraphImage(pub u32);

#[repr(transparent)]
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct GraphBuffer(pub u32);

#[repr(transparent)]
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct GraphPassId(pub u32);

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub enum GraphResource {
    Image(GraphImage),
    Buffer(GraphBuffer),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResourceAccess {
    Read,
    Write,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceUse {
    pub resource: GraphResource,
    pub access: ResourceAccess,
    pub label: String,
}

impl ResourceUse {
    pub fn read(resource: GraphResource, label: impl Into<String>) -> Self {
        Self {
            resource,
            access: ResourceAccess::Read,
            label: label.into(),
        }
    }

    pub fn write(resource: GraphResource, label: impl Into<String>) -> Self {
        Self {
            resource,
            access: ResourceAccess::Write,
            label: label.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderPassDesc {
    pub name: String,
    pub queue: QueueClass,
    pub resources: Vec<ResourceUse>,
    pub timing_scope: String,
}

impl RenderPassDesc {
    pub fn graphics(name: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            timing_scope: name.clone(),
            name,
            queue: QueueClass::Graphics,
            resources: Vec::new(),
        }
    }

    pub fn reads(mut self, resource: GraphResource, label: impl Into<String>) -> Self {
        self.resources.push(ResourceUse::read(resource, label));
        self
    }

    pub fn writes(mut self, resource: GraphResource, label: impl Into<String>) -> Self {
        self.resources.push(ResourceUse::write(resource, label));
        self
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderGraph {
    next_image: u32,
    next_buffer: u32,
    passes: Vec<RenderPassDesc>,
}

impl RenderGraph {
    pub fn create_image(&mut self) -> GraphImage {
        let image = GraphImage(self.next_image);
        self.next_image += 1;
        image
    }

    pub fn create_buffer(&mut self) -> GraphBuffer {
        let buffer = GraphBuffer(self.next_buffer);
        self.next_buffer += 1;
        buffer
    }

    pub fn add_pass(&mut self, pass: RenderPassDesc) -> GraphPassId {
        let id = GraphPassId(self.passes.len() as u32);
        self.passes.push(pass);
        id
    }

    pub fn passes(&self) -> &[RenderPassDesc] {
        &self.passes
    }

    pub fn validate(&self) -> GraphValidationReport {
        let mut written = HashSet::new();
        let mut diagnostics = Vec::new();

        for pass in &self.passes {
            let mut pass_writes = HashSet::new();
            for usage in &pass.resources {
                match usage.access {
                    ResourceAccess::Read if !written.contains(&usage.resource) => {
                        diagnostics.push(GraphDiagnostic {
                            pass_name: pass.name.clone(),
                            resource_label: usage.label.clone(),
                            kind: GraphDiagnosticKind::ReadBeforeWrite,
                        });
                    }
                    ResourceAccess::Write => {
                        pass_writes.insert(usage.resource);
                    }
                    ResourceAccess::Read => {}
                }
            }
            written.extend(pass_writes);
        }

        GraphValidationReport {
            passed: diagnostics.is_empty(),
            diagnostics,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphValidationReport {
    pub passed: bool,
    pub diagnostics: Vec<GraphDiagnostic>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphDiagnostic {
    pub pass_name: String,
    pub resource_label: String,
    pub kind: GraphDiagnosticKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GraphDiagnosticKind {
    ReadBeforeWrite,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_before_write_fails_validation() {
        let mut graph = RenderGraph::default();
        let color = graph.create_image();
        graph.add_pass(
            RenderPassDesc::graphics("tone_map").reads(GraphResource::Image(color), "hdr_color"),
        );

        let report = graph.validate();

        assert!(!report.passed);
        assert_eq!(
            report.diagnostics[0].kind,
            GraphDiagnosticKind::ReadBeforeWrite
        );
    }

    #[test]
    fn write_then_read_passes_validation() {
        let mut graph = RenderGraph::default();
        let color = graph.create_image();
        graph.add_pass(
            RenderPassDesc::graphics("clear").writes(GraphResource::Image(color), "hdr_color"),
        );
        graph.add_pass(
            RenderPassDesc::graphics("tone_map").reads(GraphResource::Image(color), "hdr_color"),
        );

        assert!(graph.validate().passed);
    }
}
