#![forbid(unsafe_code)]

use std::panic::{self, AssertUnwindSafe};

use engine_core::{
    GpuCapabilities, GpuFeatureReport, InterfaceVersion, MemoryHeapReport, RendererGpuTier,
};
use serde::{Deserialize, Serialize};
use vulkano::memory::MemoryHeapFlags;
use vulkano_util::context::{VulkanoConfig, VulkanoContext};

pub const GFX_VK_VERSION: InterfaceVersion = InterfaceVersion::new("gfx_vk", 0, 1, 0);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VulkanBootstrapConfig {
    pub print_device_name: bool,
    pub prefer_validation: bool,
}

impl Default for VulkanBootstrapConfig {
    fn default() -> Self {
        Self {
            print_device_name: true,
            prefer_validation: cfg!(debug_assertions),
        }
    }
}

pub struct VulkanContext {
    context: VulkanoContext,
    capabilities: GpuCapabilities,
}

impl VulkanContext {
    pub fn new(config: VulkanBootstrapConfig) -> anyhow::Result<Self> {
        let vulkano_config = VulkanoConfig {
            print_device_name: config.print_device_name,
            ..Default::default()
        };
        let context = panic::catch_unwind(AssertUnwindSafe(|| VulkanoContext::new(vulkano_config)))
            .map_err(|_| anyhow::anyhow!("Vulkano context creation panicked"))?;
        let capabilities = capabilities_from_context(&context);

        Ok(Self {
            context,
            capabilities,
        })
    }

    pub fn capabilities(&self) -> &GpuCapabilities {
        &self.capabilities
    }

    pub fn device_name(&self) -> &str {
        &self.capabilities.device_name
    }

    pub fn vulkano_context(&self) -> &VulkanoContext {
        &self.context
    }
}

pub fn try_collect_gpu_capabilities(
    config: VulkanBootstrapConfig,
) -> anyhow::Result<GpuCapabilities> {
    Ok(VulkanContext::new(config)?.capabilities().clone())
}

fn capabilities_from_context(context: &VulkanoContext) -> GpuCapabilities {
    let physical_device = context.device().physical_device();
    let properties = physical_device.properties();

    GpuCapabilities {
        schema_version: "gpu_info.v1".to_string(),
        device_name: properties.device_name.clone(),
        vendor_id: properties.vendor_id,
        device_id: properties.device_id,
        api_version: properties.api_version.to_string(),
        driver_version: properties.driver_version.to_string(),
        selected_tier: RendererGpuTier::Vulkan13Portable,
        features: GpuFeatureReport {
            dynamic_rendering: true,
            synchronization2: true,
            timeline_semaphore: true,
            descriptor_indexing: false,
            descriptor_buffer: false,
            buffer_device_address: false,
            ray_query: false,
            ray_tracing_pipeline: false,
            acceleration_structure: false,
        },
        memory_heaps: memory_heap_reports(context),
        max_image_dimension_2d: properties.max_image_dimension2_d,
        max_storage_buffer_range: u64::from(properties.max_storage_buffer_range),
        timestamp_period_ns: properties.timestamp_period,
    }
}

fn memory_heap_reports(context: &VulkanoContext) -> Vec<MemoryHeapReport> {
    context
        .device()
        .physical_device()
        .memory_properties()
        .memory_heaps
        .iter()
        .map(|heap| MemoryHeapReport {
            size_mb: heap.size / (1024 * 1024),
            device_local: heap.flags.intersects(MemoryHeapFlags::DEVICE_LOCAL),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unavailable_gpu_report_is_explicit() {
        let report = GpuCapabilities::unavailable("test");

        assert_eq!(report.selected_tier, RendererGpuTier::Unavailable);
        assert!(report.device_name.contains("test"));
    }

    #[test]
    fn bootstrap_config_defaults_to_debug_validation_preference() {
        let config = VulkanBootstrapConfig::default();

        assert_eq!(config.prefer_validation, cfg!(debug_assertions));
    }
}
