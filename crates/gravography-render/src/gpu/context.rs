//! Headless wgpu device creation.

use crate::RenderError;

/// A wgpu device and queue, with information about the adapter.
pub struct GpuContext {
    /// Logical device.
    pub device: wgpu::Device,
    /// Submission queue.
    pub queue: wgpu::Queue,
    /// Adapter description (name, backend, driver).
    pub adapter_info: wgpu::AdapterInfo,
}

impl GpuContext {
    /// Creates a headless device on the default high-performance adapter, using the adapter's own limits (the
    /// tracer needs larger storage buffers than the WebGPU defaults allow on some platforms).
    ///
    /// Returns `Ok(None)` if no adapter is available, so tests can skip with a message (`CLAUDE.md` §4).
    ///
    /// # Errors
    /// [`RenderError::Gpu`] if an adapter exists but the device cannot be created.
    pub fn new_headless() -> Result<Option<Self>, RenderError> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let options = wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            ..Default::default()
        };
        let Ok(adapter) = pollster::block_on(instance.request_adapter(&options)) else {
            return Ok(None);
        };
        let desc = wgpu::DeviceDescriptor {
            label: Some("gravography headless device"),
            required_limits: adapter.limits(),
            ..Default::default()
        };
        let (device, queue) = pollster::block_on(adapter.request_device(&desc))
            .map_err(|e| RenderError::Gpu(format!("device creation failed: {e}")))?;
        Ok(Some(Self {
            device,
            queue,
            adapter_info: adapter.get_info(),
        }))
    }

    /// Creates a headless context for a test, or prints why the test is skipped (`CLAUDE.md` §4: GPU tests skip
    /// with a clear message when no adapter is available). An adapter that exists but fails to create a device is
    /// a real failure and panics.
    pub fn for_tests(test: &str) -> Option<Self> {
        match Self::new_headless() {
            Ok(Some(ctx)) => Some(ctx),
            Ok(None) => {
                println!("SKIPPED {test}: no GPU adapter available");
                None
            }
            Err(e) => panic!("{test}: a GPU adapter exists but the device could not be created: {e}"),
        }
    }

    /// One-line adapter description, e.g. `Apple M1 (Metal)`.
    pub fn describe(&self) -> String {
        format!("{} ({:?})", self.adapter_info.name, self.adapter_info.backend)
    }
}
