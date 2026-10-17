//! wgpu surface, adapter, device, and queue management.

use std::sync::Arc;
use thiserror::Error;
use winit::dpi::PhysicalSize;
use winit::window::Window;

/// Errors that can occur during GPU initialization and configuration.
#[derive(Debug, Error)]
pub enum GpuError {
    /// Failed to create the WGPU surface from the window handle.
    #[error("failed to create surface: {0}")]
    CreateSurface(#[from] wgpu::CreateSurfaceError),

    /// Failed to request a suitable GPU adapter.
    #[error("failed to request GPU adapter: {0}")]
    RequestAdapter(#[from] wgpu::RequestAdapterError),

    /// Failed to request a logical device from the adapter.
    #[error("failed to request logical device: {0}")]
    RequestDevice(#[from] wgpu::RequestDeviceError),

    /// Failed to generate a default surface configuration.
    #[error("failed to generate default surface configuration")]
    NoSurfaceConfig,
}

/// Owns the WGPU graphics state for rendering into a window.
pub struct GpuContext {
    /// WGPU surface attached to the window.
    pub surface: wgpu::Surface<'static>,
    /// Logical GPU device.
    pub device: wgpu::Device,
    /// Command execution queue.
    pub queue: wgpu::Queue,
    /// Active surface configuration.
    pub config: wgpu::SurfaceConfiguration,
}

impl GpuContext {
    /// Initializes WGPU on the given window synchronously using `pollster`.
    pub fn new(window: Arc<Window>) -> Result<Self, GpuError> {
        let size = window.inner_size();

        let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
        #[cfg(target_os = "windows")]
        {
            // On Windows, use DX12 (the native Windows DirectX 12 API) unless overridden by WGPU_BACKEND.
            // Avoid Vulkan by default on Windows because third-party Vulkan layer DLLs
            // (e.g. OBS, RTSS, Discord overlay) frequently crash during instance enumeration with 0xc0000005.
            if std::env::var("WGPU_BACKEND").is_ok() {
                desc = desc.with_env();
            } else {
                desc.backends = wgpu::Backends::DX12;
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            desc = desc.with_env();
        }

        let instance = wgpu::Instance::new(desc);
        let surface = instance.create_surface(window)?;
        pollster::block_on(Self::init_async(&instance, surface, size))
    }

    async fn init_async(
        instance: &wgpu::Instance,
        surface: wgpu::Surface<'static>,
        size: PhysicalSize<u32>,
    ) -> Result<Self, GpuError> {
        let adapter = match instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
                ..Default::default()
            })
            .await
        {
            Ok(adapter) => adapter,
            Err(_) => {
                instance
                    .request_adapter(&wgpu::RequestAdapterOptions {
                        power_preference: wgpu::PowerPreference::LowPower,
                        compatible_surface: Some(&surface),
                        force_fallback_adapter: true,
                        ..Default::default()
                    })
                    .await?
            }
        };

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("oxidite-device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::default(),
                ..Default::default()
            })
            .await?;

        let width = size.width.max(1);
        let height = size.height.max(1);

        let mut config = surface
            .get_default_config(&adapter, width, height)
            .ok_or(GpuError::NoSurfaceConfig)?;

        config.present_mode = wgpu::PresentMode::AutoVsync;
        surface.configure(&device, &config);

        Ok(Self {
            surface,
            device,
            queue,
            config,
        })
    }

    /// Resizes the surface to the new window dimensions.
    pub fn resize(&mut self, new_size: PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);
        }
    }
}
