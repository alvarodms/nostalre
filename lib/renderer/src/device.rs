use std::sync::Arc;

/// 32-bit float: 16-bit depth stepped about 3 world units at the farthest outdoor zoom (decals, water edges
/// and the cell grid flickered)
pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

pub fn block_on<F: std::future::Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    let waker = std::task::Waker::noop();
    let mut cx = std::task::Context::from_waker(waker);
    loop {
        match future.as_mut().poll(&mut cx) {
            std::task::Poll::Ready(val) => return val,
            std::task::Poll::Pending => std::hint::spin_loop(),
        }
    }
}

pub struct RenderDevice {
    pub instance: wgpu::Instance,
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub surface: wgpu::Surface<'static>,
    pub surface_config: wgpu::SurfaceConfiguration,
    pub surface_format: wgpu::TextureFormat,
    pub scene_format: wgpu::TextureFormat,
    pub depth_texture: wgpu::Texture,
    pub depth_view: wgpu::TextureView,
}

/// An instance over `backends`, a surface on `window` and the adapter that draws to it; none when no backend in the
/// set has one (a GL-only pick on a desktop without GL, Vulkan on a Mac)
async fn open(
    window: &Arc<winit::window::Window>,
    backends: wgpu::Backends,
) -> Option<(wgpu::Instance, wgpu::Surface<'static>, wgpu::Adapter)> {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends,
        flags: wgpu::InstanceFlags::from_build_config().with_env(),
        ..Default::default()
    });
    let surface = match instance.create_surface(window.clone()) {
        Ok(surface) => surface,
        Err(e) => {
            tracing::warn!("no surface over {backends:?}: {e}");
            return None;
        }
    };
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        })
        .await;
    match adapter {
        Ok(adapter) => Some((instance, surface, adapter)),
        Err(e) => {
            tracing::warn!("no adapter over {backends:?}: {e}");
            None
        }
    }
}

/// The API's name as Settings shows it
pub fn backend_label(backend: wgpu::Backend) -> &'static str {
    match backend {
        wgpu::Backend::Vulkan => "Vulkan",
        wgpu::Backend::Gl if cfg!(target_os = "android") => "OpenGL ES",
        wgpu::Backend::Gl => "OpenGL",
        wgpu::Backend::Metal => "Metal",
        wgpu::Backend::Dx12 => "DirectX 12",
        wgpu::Backend::BrowserWebGpu => "WebGPU",
        wgpu::Backend::Noop => "none",
    }
}

impl RenderDevice {
    /// Draws through `backends` (WGPU_BACKEND overrides it), or any backend when none of those can start.
    pub async fn new(window: Arc<winit::window::Window>, backends: wgpu::Backends) -> Self {
        let wanted = backends.with_env();
        let (instance, surface, adapter) = match open(&window, wanted).await {
            Some(opened) => opened,
            None if wanted != wgpu::Backends::all() => {
                tracing::warn!("{wanted:?} cannot start: drawing with any backend");
                open(&window, wgpu::Backends::all())
                    .await
                    .expect("a graphics adapter")
            }
            None => panic!("no graphics adapter"),
        };

        let info = adapter.get_info();
        tracing::info!(
            "GPU: {} ({:?}), driver {} {}",
            info.name,
            info.backend,
            info.driver,
            info.driver_info
        );

        // GLES drivers may fall short of the default limits somewhere the game never goes: ask for what the
        // adapter has there rather than fail
        let required_limits = if wgpu::Limits::default().check_limits(&adapter.limits()) {
            wgpu::Limits::default().using_resolution(adapter.limits())
        } else {
            tracing::warn!("adapter below the default limits: asking for its own");
            adapter.limits()
        };
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("nostalre-client"),
                // nothing needs adapter-specific format features: validation follows the portable rules
                required_features: wgpu::Features::empty(),
                required_limits,
                memory_hints: wgpu::MemoryHints::Performance,
                ..Default::default()
            })
            .await
            .unwrap();

        let size = window.inner_size();
        let surface_caps = surface.get_capabilities(&adapter);
        // The scene draws through a plain (non-sRGB) view of an sRGB swapchain. Drivers without
        // VK_KHR_swapchain_mutable_format (Adreno on Android) cannot add that view: they get a plain
        // swapchain instead, which leaves the scene's pixels unchanged.
        let srgb_swapchain = adapter
            .get_downlevel_capabilities()
            .flags
            .contains(wgpu::DownlevelFlags::SURFACE_VIEW_FORMATS);
        // 8-bit RGBA/BGRA first: a driver may list a float or 10-bit format before them
        let eight_bit = |f: &wgpu::TextureFormat| {
            matches!(
                f.remove_srgb_suffix(),
                wgpu::TextureFormat::Rgba8Unorm | wgpu::TextureFormat::Bgra8Unorm
            )
        };
        let surface_format = surface_caps
            .formats
            .iter()
            .copied()
            .find(|f| eight_bit(f) && f.is_srgb() == srgb_swapchain)
            .or_else(|| {
                surface_caps
                    .formats
                    .iter()
                    .copied()
                    .find(|f| f.is_srgb() == srgb_swapchain)
            })
            .or_else(|| surface_caps.formats.first().copied())
            .unwrap_or(wgpu::TextureFormat::Bgra8UnormSrgb);
        let scene_format = surface_format.remove_srgb_suffix();

        // Android: vsync, so the frame pacing caps it at 60 (Mailbox draws frames nobody sees: heat, battery)
        let present_mode = if !cfg!(target_os = "android")
            && surface_caps
                .present_modes
                .contains(&wgpu::PresentMode::Mailbox)
        {
            wgpu::PresentMode::Mailbox
        } else {
            wgpu::PresentMode::Fifo
        };

        // HD water copies the finished opaque frame out of the swapchain image to refract it
        let scene_copy = surface_caps.usages.contains(wgpu::TextureUsages::COPY_SRC);
        let surface_config = wgpu::SurfaceConfiguration {
            usage: if scene_copy {
                wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC
            } else {
                wgpu::TextureUsages::RENDER_ATTACHMENT
            },
            format: surface_format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode,
            // the scene leaves alpha below 1 in places: a compositor that blends would show through it
            alpha_mode: if surface_caps
                .alpha_modes
                .contains(&wgpu::CompositeAlphaMode::Opaque)
            {
                wgpu::CompositeAlphaMode::Opaque
            } else {
                wgpu::CompositeAlphaMode::Auto
            },
            desired_maximum_frame_latency: 2,
            view_formats: if scene_format == surface_format {
                vec![]
            } else {
                vec![scene_format]
            },
        };
        surface.configure(&device, &surface_config);

        let (depth_texture, depth_view) =
            Self::create_depth_texture(&device, surface_config.width, surface_config.height);

        Self {
            instance,
            adapter,
            device,
            queue,
            surface,
            surface_config,
            surface_format,
            scene_format,
            depth_texture,
            depth_view,
        }
    }

    fn create_depth_texture(
        device: &wgpu::Device,
        width: u32,
        height: u32,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("depth"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        (texture, view)
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.surface_config.width = width;
        self.surface_config.height = height;
        self.surface.configure(&self.device, &self.surface_config);
        let (depth_texture, depth_view) = Self::create_depth_texture(&self.device, width, height);
        self.depth_texture = depth_texture;
        self.depth_view = depth_view;
    }

    /// A new surface for `window`, whose native window changed (Android destroys it when the app leaves the
    /// screen and makes a new one when it comes back).
    pub fn recreate_surface(&mut self, window: Arc<winit::window::Window>) {
        self.surface = self.instance.create_surface(window.clone()).unwrap();
        let size = window.inner_size();
        self.resize(size.width.max(1), size.height.max(1));
    }

    /// The API and the GPU drawing, as Settings shows them: `Vulkan · Mali-G77 MC9`
    pub fn gpu_label(&self) -> String {
        let info = self.adapter.get_info();
        format!("{} · {}", backend_label(info.backend), info.name)
    }

    /// Whether the swapchain image can be copied from (HD water's refraction).
    pub fn scene_copy_supported(&self) -> bool {
        self.surface_config
            .usage
            .contains(wgpu::TextureUsages::COPY_SRC)
    }

    pub fn reconfigure_surface(&self) {
        self.surface.configure(&self.device, &self.surface_config);
    }
}
