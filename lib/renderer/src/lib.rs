pub mod camera;
pub mod damage_number;
pub mod day_cycle;
mod device;
pub mod effect;
pub mod effect_sprite;
pub mod font_atlas;
mod frame_trace;
pub mod fps;
pub mod global_uniforms;
pub mod gr2_model;
pub mod grass;
pub mod graffiti;
pub mod grid_selector;
pub mod ground;
pub mod ground_proxy;
mod matrix_texture;
mod mesh_batch;
pub use mesh_batch::{QUAD_INDICES, Vertices, Z_QUAD_INDICES, shared_indices};
pub mod model;
pub mod portrait;
pub mod post;
pub mod rsm_anim;
pub mod scene_light;
pub mod shadow;
pub mod screen_distortion;
pub mod sprite;
pub mod sprite_projection;
pub mod texture;
pub mod ui_renderer;
pub mod water;

/// The terrain shader, after the water's waves it draws caustics from and the lamps it shares with grass.
pub const TERRAIN_WGSL: &str = concat!(
    include_str!("shaders/water_waves.wgsl"),
    include_str!("shaders/scene_lamps.wgsl"),
    include_str!("shaders/terrain.wgsl")
);
/// The HD water shader, after its waves.
pub const WATER_HD_WGSL: &str = concat!(
    include_str!("shaders/water_waves.wgsl"),
    include_str!("shaders/water_hd.wgsl")
);

#[cfg(test)]
pub(crate) mod test_support {
    /// wgpu's stub device: real buffers and bind groups, no GPU behind them.
    pub(crate) fn device() -> (wgpu::Device, wgpu::Queue) {
        wgpu::Device::noop(&wgpu::DeviceDescriptor::default())
    }

    /// Two empty bind groups at distinct addresses, for batching that tells textures apart by address.
    pub(crate) fn bind_groups() -> &'static [wgpu::BindGroup; 2] {
        static GROUPS: std::sync::OnceLock<[wgpu::BindGroup; 2]> = std::sync::OnceLock::new();
        GROUPS.get_or_init(|| {
            let (device, _) = device();
            let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: None,
                entries: &[],
            });
            let group = || {
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: None,
                    layout: &layout,
                    entries: &[],
                })
            };
            [group(), group()]
        })
    }
}

pub use camera::Camera;
pub use device::{DEPTH_FORMAT, RenderDevice, block_on};
pub use fps::Fps;
pub use global_uniforms::{FogUniform, GlobalUniforms, LightUniform, PointLightGpu};

pub use damage_number::render_damage_number_quads;
pub use nostalre_effects::sfx::{SfxEmission, SfxPos};

pub use effect::{
    BlendBucket, BlendKind, DrawRecord, EffectDispatcher, PipelineKind, StrEffectCache,
    StrEffectEntry, StrEmitterInput, build_str_effect_batches, d3d_blend_to_wgpu,
    prepare_billboard_records, prepare_cylinder_records, prepare_frustum_records,
    prepare_ground_disc_records, prepare_line_strip_records, prepare_quad_horn_records,
    prepare_radial_ring_records, prepare_screen_quad_records, prepare_sphere_records,
    prepare_texture3d_records, prepare_world_quad_records,
};
pub use effect_sprite::{
    BurstParticle, EffectSpriteCache, EffectSpriteEntry, EmitterDraw, SpriteEffectEmitter,
    build_emitter_batches, collect_sprite_effect_draws, prepare_sprite_particle_records,
    project_billboard,
};
pub use font_atlas::FontAtlas;
pub use gr2_model::{
    Gr2Geometry, Gr2ModelAsset, Gr2ModelDraw, Gr2ModelPipeline, Gr2ModelVertex, Gr2TextureData,
    build_gr2_geometry, decode_textures,
};
pub use grid_selector::GridSelectorRenderer;
pub use ground::GroundRenderer;
pub use ground_proxy::GroundProxyRenderer;
pub use model::{AnimatedModelRenderer, ModelRenderer};
pub use screen_distortion::ScreenDistortion;
pub use sprite::{
    BodyChannels, ClipQuad, CompositeClips, EntitySprite, RobeLayer, SpriteBatch, SpriteRenderer,
    SpriteTextures, SpriteUniforms, SpriteVertex, build_clip_quad, build_clip_quad_scaled,
    build_composite_clips, build_entity_sprite, compose_actor_batches, scale_clip_vertices,
    transform_batch_vertices, upload_glyph_textures, upload_sprite_textures,
};
pub use texture::TextureCache;
pub use ui_renderer::{UiDrawCommand, UiRenderer, UiVertex};
pub use water::WaterRenderer;
pub use wgpu;

pub const SPRITE_SHADER_SRC: &str = include_str!("shaders/sprite.wgsl");

use nostalre_formats::fog_table::FogEntry;
use nostalre_formats::gnd::GndFile;
use nostalre_formats::map_water::map_water;
use nostalre_formats::grf::GrfArchive;
use nostalre_formats::rsm::RsmFile;
use nostalre_formats::rsw::{RswFile, RswObject};
use std::borrow::Cow;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackgroundMode {
    Clear,
    GroundProxy,
    RswMap,
}

impl Default for BackgroundMode {
    fn default() -> Self {
        BackgroundMode::RswMap
    }
}

pub enum UiTextureRef {
    FontAtlas,
    White,
    /// A texture cache path: a constant one borrowed, a built one owned.
    Named(Cow<'static, str>),
    Inline(usize),
}

/// One UI mesh. A quad's vertices are inline and its indices a shared static slice, so drawing a widget allocates
/// nothing.
pub struct UiDrawCall {
    pub vertices: Vertices<UiVertex>,
    pub indices: Cow<'static, [u32]>,
    pub texture: UiTextureRef,
}

pub struct FrameInputs<'a> {
    pub ui_draw_calls: &'a [UiDrawCall],
    /// Appended to `after_ui_batches`, so tooltips paint over the cursor sprite
    /// instead of behind it.
    pub tooltip_draw_calls: &'a [UiDrawCall],
    pub effect_sprite_batches: &'a [SpriteBatch<'a>],
    /// Effects under the units, drawn before the sprites (kRO's floors)
    pub effect_under_sprite_batches: &'a [SpriteBatch<'a>],
    pub effect_draws: &'a effect::EffectDrawList,
    pub sprite_particle_records: Vec<DrawRecord<'a>>,
    pub sprite_batches: &'a [SpriteBatch<'a>],
    pub silhouette_batches: &'a [SpriteBatch<'a>],
    /// Drawn after the UI, in order: effects kRO plays over its windows, then the cursor
    pub after_ui_batches: Vec<SpriteBatch<'a>>,
    pub inline_textures: &'a [&'a wgpu::BindGroup],
    pub elapsed: f32,
    /// Seconds since the previous frame. Callers that render more than once per
    /// frame (offscreen capture) must pass 0.0 for the extra passes so
    /// time-stepped state is not advanced twice.
    pub delta: f32,
}

pub struct Renderer {
    pub device: RenderDevice,
    pub camera: Camera,
    pub global_uniforms: GlobalUniforms,
    pub texture_cache: TextureCache,
    pub ground_renderer: Option<GroundRenderer>,
    pub ground_proxy: Option<GroundProxyRenderer>,
    pub model_renderer: Option<ModelRenderer>,
    pub animated_model_renderer: Option<AnimatedModelRenderer>,
    pub skill_unit_models: std::collections::HashMap<u32, ModelRenderer>,
    /// Animated GR2 entity models keyed by entity gid (emperium, guardians…).
    pub gr2_models: std::collections::HashMap<u32, Gr2ModelDraw>,
    /// Geometry and textures of each loaded `.gr2`, keyed by GRF path and
    /// shared by every entity drawn with it.
    pub gr2_assets: std::collections::HashMap<String, std::rc::Rc<Gr2ModelAsset>>,
    pub gr2_pipeline: Gr2ModelPipeline,
    pub water_renderer: Option<WaterRenderer>,
    pub grid_selector: Option<GridSelectorRenderer>,
    pub sprite_renderer: SpriteRenderer,
    pub effect_sprite_renderer: SpriteRenderer,
    pub ui_sprite_renderer: SpriteRenderer,
    pub effect_primitives: effect::EffectPrimitiveRegistry,
    pub effect_dispatcher: effect::EffectDispatcher,
    /// The effects behind the sprites have buffers of their own: both dispatches upload in the same frame, and
    /// uploads land before either draws, so sharing made the behind pass draw the front pass's geometry.
    effect_behind_dispatcher: effect::EffectDispatcher,
    /// The map's RSW lights; the frame shades with the nearest (global_uniforms::nearest_point_lights)
    point_lights: Vec<PointLightGpu>,
    /// The map's lamp models' lights, lit at night (`scene_light::night_lights`)
    lamp_lights: Vec<PointLightGpu>,
    point_light_center: Option<glam::Vec3>,
    pub ui_renderer: UiRenderer,
    pub font_atlas: FontAtlas,
    pub font_atlas_bind_group: wgpu::BindGroup,
    pub white_bind_group: wgpu::BindGroup,
    pub font_px_height: f32,
    pub dpi_scale: f32,
    pub clear_color: wgpu::Color,
    pub screen_distortion: ScreenDistortion,
    /// The format the scene pipelines draw into: the HD target's, or the swapchain's view when HD is off
    world_format: wgpu::TextureFormat,
    /// The HD frame, when HD was on at startup
    post: Option<post::PostChain>,
    pub background_mode: BackgroundMode,
    /// The map's day lighting, captured in `load_map`. Each light written starts from
    /// it (`refresh_light`) so the day/night fade never loses the map's light dir,
    /// ambient or shadow strength.
    pub base_light: LightUniform,
    /// Today's day/night fade: the diffuse and the sprites' factor (`set_day_night`)
    legacy_diffuse: [f32; 3],
    legacy_sprite_light: [f32; 3],
    /// 0 day to 1 night, for the HD night
    night: f32,
    /// The server says it is night (status 160)
    server_night: bool,
    /// The map's own sun (RSW angles), the day cycle's noon
    map_sun: day_cycle::SunAngles,
    /// Where in the hour the wall clock is (0 to 1), and a `/sky` preview over it
    clock_phase: f32,
    sky_preview: Option<f32>,
    /// The sky on screen, eased toward the clock's; `None` until the cycle runs on this map
    sky_shown: Option<day_cycle::Sky>,
    /// This map has night (`day_cycle::learn_darkens`)
    map_darkens: bool,
    /// HD sun shadows: the map (while they run), the row's strength, the preset's quality
    shadow_map: Option<shadow::ShadowMap>,
    shadow_strength: f32,
    shadow_tier: Option<shadow::ShadowTier>,
    /// What the map was last drawn for, its light box, frames since, and a count bumped by map/tier changes
    shadow_last: Option<shadow::RedrawState>,
    shadow_vp: glam::Mat4,
    shadow_frames: u32,
    shadow_generation: u64,
    /// The light's direction (toward the sun or moon) last written
    sun_dir_now: glam::Vec3,
    /// The current map is indoors: no HD night, clouds or torch
    indoor: bool,
    /// HD scene light rows (Settings)
    scene: scene_light::SceneOptions,
    cloud_offset: [f32; 2],
    /// The torch strength last sent with the point lights
    point_light_torch: f32,
    /// The point lights last sent, for lighting sprites the way the terrain is lit
    point_lights_sent: global_uniforms::PointLightsUniform,
    /// The scene light and sprite light last written, for `actor_scene_light`
    scene_now: [f32; 4],
    sprite_light_now: [f32; 3],
    /// HD world glow levels from Settings (lamp, pools, sun); the shaders get them only while bloom shows.
    hd_glow: [f32; 3],
    /// HD grass, when HD runs
    grass: Option<grass::GrassRenderer>,
    grass_tier: grass::scatter::Tier,
    grass_density: f32,
    /// Cells under a model floor on this map: no grass there
    model_floor_cells: Vec<bool>,
    /// The optional blade picture (`grass::BLADE_ART_PATH`) is looked for once, at the first map
    grass_art_tried: bool,
    /// The loaded map's fog, kept so a `fog_scale` change can re-derive the
    /// distances without the caller handing back the table entry. `None` when
    /// the map has no entry or the player turned fog off.
    fog_entry: Option<FogEntry>,
    fog_scale: f32,
    lightmap_enabled: bool,
    /// HD water (refraction, depth colour, caustics) when the frame allows it; classic water otherwise.
    hd_water: bool,
    filter_effects: bool,
    /// Effect textures uploaded so far, so a filter change can rebuild them.
    effect_textures: Vec<String>,
    /// NOSTALRE_FRAME_TRACE: what each frame hands the GPU, in the log
    frame_trace: Option<frame_trace::FrameTrace>,
}

/// The view planes the fog table's `near`/`far` columns are fractions of. Not
/// [`Camera::near`]/[`Camera::far`], which are ours and are set wider.
const FOG_TABLE_NEAR_PLANE: f32 = 10.0;
const FOG_TABLE_FAR_PLANE: f32 = 1500.0;

fn fog_world_distance(fraction: f32, scale: f32) -> f32 {
    (FOG_TABLE_NEAR_PLANE + fraction * (FOG_TABLE_FAR_PLANE - FOG_TABLE_NEAR_PLANE)) * scale
}

#[allow(clippy::too_many_arguments)]
fn build_effect_records<'tex>(
    effect_draws: &effect::EffectDrawList,
    camera: &Camera,
    texture_cache: &'tex TextureCache,
    white_bind_group: &'tex wgpu::BindGroup,
    logical_w: f32,
    logical_h: f32,
    primitives: &effect::EffectPrimitiveRegistry,
    out: &mut Vec<DrawRecord<'tex>>,
) {
    let texture_lookup = |name: &str| -> Option<&'tex wgpu::BindGroup> {
        if name.is_empty() {
            return None;
        }
        // Runtime-composed textures are registered under their own key, which must not
        // be rewritten into a GRF path.
        name.split('|').find_map(|candidate| {
            texture_cache
                .get(candidate)
                .or_else(|| texture_cache.get(&effect::effect_texture_path(candidate)))
        })
    };
    prepare_billboard_records(
        effect_draws,
        camera,
        logical_w,
        logical_h,
        white_bind_group,
        &texture_lookup,
        out,
    );
    for renderer in primitives.iter() {
        renderer.prepare(effect_draws, camera, white_bind_group, &texture_lookup, out);
    }
}

impl Renderer {
    pub async fn new(
        window: Arc<winit::window::Window>,
        font_px_height: f32,
        dpi_scale: f32,
        hd: post::HdOptions,
    ) -> Self {
        let device = RenderDevice::new(window).await;
        let float_supported = post::float_target_supported(
            &device
                .adapter
                .get_texture_format_features(wgpu::TextureFormat::Rgba16Float),
        );
        let world_format = if hd.enabled {
            post::pick_scene_format(device.surface_format, float_supported, hd.float_target)
        } else {
            device.scene_format
        };
        // the composite writes the swapchain's scene view, or the screen distortion's of the same format
        let post = post::PostChain::for_options(
            &device.device,
            &hd,
            world_format,
            device.scene_format,
            device.surface_config.width,
            device.surface_config.height,
        );
        tracing::info!("HD: {} (scene target {world_format:?})", hd.enabled);
        let camera = Camera::with_aspect(
            device.surface_config.width as f32 / device.surface_config.height as f32,
        );
        let global_uniforms = GlobalUniforms::new(&device.device);
        let texture_cache = TextureCache::new(&device.device);
        let grass = post.is_some().then(|| {
            let mut grass = grass::GrassRenderer::new(
                &device.device,
                &device.queue,
                world_format,
                &global_uniforms,
                &texture_cache.bind_group_layout,
            );
            grass.set_flora(hd.flora);
            grass
        });

        let gr2_pipeline = Gr2ModelPipeline::new(
            &device.device,
            world_format,
            &global_uniforms,
            &texture_cache,
        );

        let mut font_atlas = FontAtlas::from_embedded(font_px_height, dpi_scale);
        let font_atlas_bind_group = texture::create_font_atlas_bind_group(
            &device.device,
            &device.queue,
            &font_atlas.image,
            &texture_cache.bind_group_layout,
            "font_atlas",
        );
        font_atlas.release_image();

        let screen_distortion = ScreenDistortion::new(&device.device, device.surface_format);

        let white_img = image::RgbaImage::from_pixel(1, 1, image::Rgba([255, 255, 255, 255]));
        let white_bind_group = texture::create_texture_bind_group(
            &device.device,
            &device.queue,
            &white_img,
            &texture_cache.bind_group_layout,
            "ui_white",
        );

        let logical_w = device.surface_config.width as f32 / dpi_scale;
        let logical_h = device.surface_config.height as f32 / dpi_scale;

        // Entity sprites: the colour pass tests against world geometry per-pixel
        // (gradient) but writes no depth, so coplanar body layers/copies blend by
        // paint order (no seams). A flat feet-depth body silhouette is stamped
        // afterward (render_silhouette) purely so effects occlude against the body
        // the way the game did before per-pixel gradient depth — effects above the
        // feet on top, ground effects at the feet occluded.
        let sprite_renderer = SpriteRenderer::new(
            &device.device,
            world_format,
            &texture_cache.bind_group_layout,
            logical_w,
            logical_h,
            include_str!("shaders/sprite.wgsl"),
            false,
        );
        let effect_sprite_renderer = SpriteRenderer::new(
            &device.device,
            world_format,
            &texture_cache.bind_group_layout,
            logical_w,
            logical_h,
            include_str!("shaders/sprite.wgsl"),
            false,
        );
        let effect_primitives = effect::EffectPrimitiveRegistry::new(
            &device.device,
            world_format,
            &global_uniforms.bind_group_layout,
            &texture_cache.bind_group_layout,
        );
        let effect_dispatcher = effect::EffectDispatcher::new(&device.device);
        let effect_behind_dispatcher = effect::EffectDispatcher::new(&device.device);

        let ui_sprite_renderer = SpriteRenderer::new(
            &device.device,
            device.scene_format,
            &texture_cache.bind_group_layout,
            logical_w,
            logical_h,
            include_str!("shaders/sprite.wgsl"),
            false,
        );

        let ui_renderer = UiRenderer::new(
            &device.device,
            device.scene_format,
            &texture_cache.bind_group_layout,
            logical_w,
            logical_h,
        );

        Self {
            device,
            camera,
            global_uniforms,
            texture_cache,
            ground_renderer: None,
            ground_proxy: None,
            model_renderer: None,
            animated_model_renderer: None,
            skill_unit_models: std::collections::HashMap::new(),
            gr2_models: std::collections::HashMap::new(),
            gr2_assets: std::collections::HashMap::new(),
            gr2_pipeline,
            water_renderer: None,
            grid_selector: None,
            sprite_renderer,
            effect_sprite_renderer,
            ui_sprite_renderer,
            effect_primitives,
            effect_dispatcher,
            effect_behind_dispatcher,
            point_lights: Vec::new(),
            lamp_lights: Vec::new(),
            point_light_center: None,
            ui_renderer,
            font_atlas,
            font_atlas_bind_group,
            white_bind_group,
            font_px_height,
            dpi_scale,
            clear_color: wgpu::Color {
                r: 0.392,
                g: 0.584,
                b: 0.929,
                a: 1.0,
            },
            screen_distortion,
            world_format,
            post,
            background_mode: BackgroundMode::default(),
            base_light: LightUniform::default(),
            legacy_diffuse: [1.0; 3],
            legacy_sprite_light: [1.0; 3],
            night: 0.0,
            server_night: false,
            map_sun: day_cycle::SunAngles {
                longitude: 0.0,
                latitude: 0.0,
            },
            clock_phase: 0.0,
            sky_preview: None,
            sky_shown: None,
            map_darkens: true,
            shadow_map: None,
            shadow_strength: hd.shadow_strength,
            shadow_tier: shadow::tier(hd.shadow_tier),
            shadow_last: None,
            shadow_vp: glam::Mat4::IDENTITY,
            shadow_frames: 0,
            shadow_generation: 0,
            sun_dir_now: glam::Vec3::NEG_Y,
            indoor: false,
            scene: hd.scene,
            cloud_offset: [0.0; 2],
            point_light_torch: 0.0,
            point_lights_sent: bytemuck::Zeroable::zeroed(),
            scene_now: [0.0; 4],
            sprite_light_now: [1.0; 3],
            hd_glow: hd.glow,
            grass,
            grass_tier: grass::scatter::Tier::from_index(hd.grass_tier),
            grass_density: hd.grass,
            model_floor_cells: Vec::new(),
            grass_art_tried: false,
            fog_entry: None,
            fog_scale: 1.0,
            lightmap_enabled: true,
            hd_water: true,
            filter_effects: true,
            effect_textures: Vec::new(),
            frame_trace: frame_trace::FrameTrace::from_env(),
        }
    }

    pub fn set_background_mode(&mut self, mode: BackgroundMode) {
        self.background_mode = mode;
    }

    /// Today's day/night fade (`diffuse`, the sprites' `sprite_light`) and how far into the night it is (0 to 1)
    /// for the HD night, which replaces the fade while it runs.
    pub fn set_day_night(&mut self, diffuse: [f32; 3], sprite_light: [f32; 3], night: f32, server_night: bool) {
        self.legacy_diffuse = diffuse;
        self.legacy_sprite_light = sprite_light;
        self.night = night;
        self.server_night = server_night;
        self.refresh_light();
    }

    pub fn toggle_lightmap(&mut self) -> bool {
        self.set_lightmap_enabled(!self.lightmap_enabled);
        self.lightmap_enabled
    }

    pub fn lightmap_enabled(&self) -> bool {
        self.lightmap_enabled
    }

    pub fn world_format(&self) -> wgpu::TextureFormat {
        self.world_format
    }

    /// Whether HD runs this session (fixed at startup: the scene pipelines are built for its target).
    pub fn hd_running(&self) -> bool {
        self.post.is_some()
    }

    pub fn hd_effects(&self) -> bool {
        self.post.as_ref().is_some_and(post::PostChain::effects)
    }

    pub fn set_hd_effects(&mut self, on: bool) {
        if let Some(post) = &mut self.post {
            post.set_effects(on);
        }
        self.refresh_light();
    }

    /// Bloom's mip count and strength while HD runs (Settings).
    pub fn set_hd_bloom(&mut self, levels: u32, intensity: f32) {
        if let Some(post) = &mut self.post {
            post.set_bloom(&self.device.device, levels, intensity);
        }
        self.refresh_light();
    }

    /// Grows the map's grass (HD only; none indoors). `map_name` seeds the layout.
    pub fn load_grass(
        &mut self,
        gnd: &GndFile,
        rsw: &RswFile,
        grf: &GrfArchive,
        map_name: &str,
        indoor: bool,
    ) {
        let Some(grass) = &mut self.grass else {
            return;
        };
        if !self.grass_art_tried {
            self.grass_art_tried = true;
            if let Some(picture) = texture::optional_picture(grass::BLADE_ART_PATH, grf) {
                tracing::info!("grass: blade art from {}", grass::BLADE_ART_PATH);
                grass.set_art(
                    &self.device.device,
                    &self.device.queue,
                    &grass::atlas::BladeArt::from_image(&picture),
                );
            }
        }
        if indoor {
            grass.clear();
            return;
        }
        let textures: Vec<grass::classify::TextureInfo> = gnd
            .textures
            .iter()
            .map(|name| {
                let path = nostalre_resources::texture::named(name);
                let rgb = texture::average_color(&path, grf).unwrap_or([0.0; 3]);
                grass::classify::TextureInfo {
                    class: grass::classify::classify_texture(name, rgb),
                    rgb,
                }
            })
            .collect();
        grass.set_map(
            &grass::GrassMap {
                gnd,
                name: map_name,
                textures: &textures,
                water: map_water(&rsw.water, gnd).as_ref(),
                floors: &self.model_floor_cells,
            },
            self.grass_tier,
            self.grass_density,
        );
    }

    /// Grass density 0 to 1 (Settings), applied to the current map at once.
    pub fn set_hd_grass(&mut self, density: f32) {
        self.grass_density = density;
        if let Some(grass) = &mut self.grass {
            grass.set_density(density);
        }
    }

    /// Ferns, mushrooms and flowers among the grass, 0 to 1 (Settings), applied to the current map at once.
    pub fn set_hd_flora(&mut self, flora: f32) {
        if let Some(grass) = &mut self.grass {
            grass.set_flora(flora);
        }
    }

    /// Grass quality, 0 (Low) to 3 (Ultra), from the HD preset (Settings): the current map's grass is remade with it.
    pub fn set_hd_grass_tier(&mut self, tier: u8) {
        self.grass_tier = grass::scatter::Tier::from_index(tier);
        if let Some(grass) = &mut self.grass {
            grass.set_tier(self.grass_tier);
        }
    }

    /// World glow levels (lamp, pools, sun) while HD runs (Settings).
    pub fn set_hd_glow(&mut self, levels: [f32; 3]) {
        self.hd_glow = levels;
        self.refresh_light();
    }

    /// Colour grade strength and vignette while HD runs (Settings).
    pub fn set_hd_grade(&mut self, strength: f32, vignette: bool) {
        if let Some(post) = &mut self.post {
            post.set_grade(strength, vignette);
        }
    }

    /// The map's colour grade; HD fades to it.
    pub fn set_map_grade(&mut self, map_name: &str) {
        if let Some(post) = &mut self.post {
            post.set_map(map_name);
        }
    }

    /// HD water shows one shader term (see water_hd.wgsl), 0 off. False when this map has no water
    /// to show it on.
    pub fn set_water_debug(&mut self, term: u8) -> bool {
        match &mut self.water_renderer {
            Some(water) => {
                water.set_debug(term);
                true
            }
            None => false,
        }
    }

    pub fn toggle_hd_water(&mut self) -> bool {
        self.hd_water = !self.hd_water;
        self.hd_water
    }

    pub fn set_lightmap_enabled(&mut self, enabled: bool) {
        self.lightmap_enabled = enabled;
        if let Some(ground) = &mut self.ground_renderer {
            ground.set_lightmap_enabled(enabled);
        }
        self.global_uniforms
            .set_cell_light_enabled(&self.device.queue, enabled);
        self.refresh_light();
    }

    /// HD scene light rows (Settings), live.
    pub fn set_hd_scene(&mut self, scene: scene_light::SceneOptions) {
        self.scene = scene;
        // the lamps and torch are picked again with the rows' new values
        self.point_light_center = None;
        self.refresh_light();
    }

    /// Whether the current map is indoors (no HD night, clouds or torch there).
    pub fn set_map_indoor(&mut self, indoor: bool) {
        self.indoor = indoor;
        self.refresh_light();
    }

    /// The map's day light with the lightmap setting applied: without lightmaps the sky lifts to make up.
    fn day_light(&self) -> LightUniform {
        let mut light = self.base_light;
        if !self.lightmap_enabled {
            for c in light.ambient_color.iter_mut().take(3) {
                *c = (*c * 1.5).min(1.0);
            }
        }
        light
    }

    /// The HD night runs: HD effects on, outdoors, Night look on.
    fn hd_night(&self) -> bool {
        self.hd_effects() && !self.indoor && self.scene.night_look
    }

    /// Sun shadow strength 0 to 1 and quality 0 (Low: none) to 3 (Ultra), from Settings, live.
    pub fn set_hd_shadows(&mut self, strength: f32, tier: u8) {
        self.shadow_strength = strength;
        self.shadow_tier = shadow::tier(tier);
    }

    /// Sun shadows run this frame (the map's baked shadows step aside meanwhile).
    pub fn sun_shadows_active(&self) -> bool {
        self.shadow_map.is_some()
    }

    /// One frame of sun shadows: the map made, kept or dropped by the rows and the map; whether it needs drawing
    /// again this frame (the light box then written for the caster pass); the receivers' uniform.
    fn prepare_shadow(&mut self) -> bool {
        let tier = self.shadow_tier;
        let on = matches!(self.background_mode, BackgroundMode::RswMap)
            && shadow::active(self.hd_effects(), self.indoor, self.shadow_strength, tier);
        let Some(tier) = tier.filter(|_| on) else {
            if self.shadow_map.take().is_some() {
                self.global_uniforms
                    .set_shadow_map(&self.device.device, None);
                self.global_uniforms
                    .update_shadow(&self.device.queue, &shadow::ShadowUniform::OFF);
                self.shadow_last = None;
            }
            return false;
        };
        if self.shadow_map.as_ref().is_none_or(|map| map.tier != tier) {
            let map = shadow::ShadowMap::new(
                &self.device.device,
                &self.global_uniforms.shadow_pass_layout,
                tier,
            );
            self.global_uniforms
                .set_shadow_map(&self.device.device, Some(&map.view));
            self.shadow_map = Some(map);
            self.shadow_generation += 1;
        }
        let now = shadow::RedrawState {
            center: self.camera.target,
            dir: self.sun_dir_now,
            models_ready: self.model_renderer.is_some(),
            generation: self.shadow_generation,
        };
        self.shadow_frames += 1;
        let animated = self.animated_model_renderer.is_some();
        let redraw = shadow::needs_redraw(self.shadow_last.as_ref(), &now, self.shadow_frames, animated);
        if redraw {
            let light_box = shadow::light_box(now.dir, now.center, tier.size);
            if let Some(map) = &self.shadow_map {
                map.write_pass(&self.device.queue, light_box.light_vp);
            }
            self.shadow_vp = light_box.light_vp;
            self.shadow_last = Some(now);
            self.shadow_frames = 0;
        }
        let night = self.night_now();
        self.global_uniforms.update_shadow(
            &self.device.queue,
            &shadow::ShadowUniform {
                light_vp: self.shadow_vp.to_cols_array_2d(),
                params: [
                    self.shadow_strength * (1.0 - 0.6 * night),
                    2.0 * shadow::HALF_EXTENT / tier.size as f32,
                    tier.taps as f32,
                    1.0,
                ],
            },
        );
        redraw
    }

    /// Where in the hour the wall clock is (0 to 1); the day cycle reads it each frame.
    pub fn set_time_of_day(&mut self, phase: f32) {
        self.clock_phase = phase;
    }

    /// `/sky`: show this point of the hour (0 to 1) instead of the clock's, this session; `None`: the clock.
    pub fn set_sky_preview(&mut self, phase: Option<f32>) {
        self.sky_preview = phase;
    }

    /// The sky drives the light now (HD effects, outdoors, Day cycle row).
    fn sky_now(&self) -> Option<&day_cycle::Sky> {
        day_cycle::active(self.hd_effects(), self.indoor, &self.scene)
            .then_some(self.sky_shown.as_ref())
            .flatten()
    }

    /// How far into the night the HD night is: the day cycle's while it runs, else the server's fade.
    fn night_now(&self) -> f32 {
        self.sky_now().map_or(self.night, |sky| sky.night)
    }

    /// One frame of the day cycle: the sky eases toward the clock's (or `/sky`'s) and the light follows.
    fn update_sky(&mut self, delta: f32) {
        if !day_cycle::active(self.hd_effects(), self.indoor, &self.scene) {
            if self.sky_shown.take().is_some() {
                self.refresh_light();
            }
            return;
        }
        self.map_darkens = day_cycle::learn_darkens(self.map_darkens, self.clock_phase, self.server_night);
        let phase = self.sky_preview.unwrap_or(self.clock_phase);
        let target = day_cycle::sky(phase, self.server_night, self.map_darkens, self.map_sun);
        let shown = match self.sky_shown {
            Some(shown) => shown.ease(&target, delta.clamp(0.0, 0.25)),
            None => target,
        };
        if self.sky_shown != Some(shown) {
            self.sky_shown = Some(shown);
            self.refresh_light();
        }
    }

    /// Writes the world light (day/night, HD scene light, the glow levels bloom can show now) and the
    /// sprites' light.
    fn refresh_light(&mut self) {
        let day = self.day_light();
        let (lit, legacy_diffuse) = match self.sky_now() {
            Some(sky) => (
                day_cycle::apply(&day, sky),
                std::array::from_fn(|i| self.legacy_diffuse[i] * sky.sun_tint[i]),
            ),
            None => (day, self.legacy_diffuse),
        };
        let mut light = scene_light::frame_light(
            &lit,
            legacy_diffuse,
            self.night_now(),
            self.hd_effects(),
            self.indoor,
            &self.scene,
        );
        light.glow = match &self.post {
            Some(post) => post::glow::effective(self.hd_glow, &post.live_options(), post.effects()),
            None => [0.0; 3],
        };
        light.cloud[..2].copy_from_slice(&self.cloud_offset);
        self.sun_dir_now = glam::Vec3::from_slice(&light.light_dir[..3]);
        self.global_uniforms
            .update_light(&self.device.queue, &light);
        let sprite_light = if self.hd_night() {
            scene_light::sprite_light(&day, &light)
        } else {
            self.legacy_sprite_light
        };
        self.sprite_renderer
            .set_world_light(&self.device.queue, sprite_light);
        self.ui_sprite_renderer
            .set_world_light(&self.device.queue, sprite_light);
        self.scene_now = light.scene;
        self.sprite_light_now = sprite_light;
    }

    /// The HD scene light on a sprite at `world` (an actor's chest) standing in baked light `baked` (the raw
    /// lightmap colour under it): a factor on its colour, over the sprites' world light. Lamps light it as they
    /// light the ground; at night a baked light pool brings back its day look. `[1; 3]` with HD scene light and
    /// HD night off.
    pub fn actor_scene_light(&self, world: glam::Vec3, baked: [f32; 3]) -> [f32; 3] {
        let [on, night, _, cap] = self.scene_now;
        if on <= 0.0 && night <= 0.0 {
            return [1.0; 3];
        }
        let bound = cap as u32 + u32::from(night > 0.0);
        let lamps = scene_light::lamps_at(&self.point_lights_sent, bound, world);
        scene_light::actor_factor(self.sprite_light_now, baked, night, lamps)
    }

    pub fn set_fog(&mut self, fog: Option<FogEntry>) {
        self.fog_entry = fog;
        self.upload_fog();
    }

    fn effect_filter(&self) -> wgpu::FilterMode {
        if self.filter_effects {
            wgpu::FilterMode::Linear
        } else {
            wgpu::FilterMode::Nearest
        }
    }

    /// Filters effect textures, the way the original game does. Pass the archive
    /// to rebuild the textures already uploaded.
    pub fn set_effect_filtering(&mut self, on: bool, grf: Option<&GrfArchive>) {
        if self.filter_effects == on {
            return;
        }
        self.filter_effects = on;
        let Some(grf) = grf else {
            return;
        };
        let paths = std::mem::take(&mut self.effect_textures);
        for path in &paths {
            self.texture_cache.remove(path);
        }
        self.preload_effect_textures(&paths, grf);
    }

    /// Entity sprites only: effect billboards share the sprite shader but must
    /// keep sampling their textures plainly.
    pub fn set_sprite_upscale(&mut self, on: bool) {
        self.sprite_renderer.set_sharpen(&self.device.queue, on);
    }

    /// Filters ground and model textures, the way the original game does. Pass
    /// the archive to rebuild the textures a loaded map already uploaded.
    pub fn set_world_filtering(&mut self, on: bool, grf: Option<&GrfArchive>) {
        self.texture_cache
            .set_world_filtering(on, grf, &self.device.device, &self.device.queue);
    }

    /// Multiplies both fog distances, so a wider view than the original game's
    /// does not sit entirely inside the fog. 1.0 is the original.
    pub fn set_fog_scale(&mut self, scale: f32) {
        self.fog_scale = scale.max(f32::EPSILON);
        self.upload_fog();
    }

    fn upload_fog(&mut self) {
        let scale = self.fog_scale;
        let fog_uniform = match self.fog_entry {
            Some(entry) => FogUniform {
                color: [entry.color[0], entry.color[1], entry.color[2], 1.0],
                near: fog_world_distance(entry.near, scale),
                far: fog_world_distance(entry.far, scale),
                enabled: 1.0,
                _pad: 0.0,
            },
            None => FogUniform::default(),
        };
        self.global_uniforms
            .update_fog(&self.device.queue, &fog_uniform);
        self.sprite_renderer.set_fog(
            &self.device.queue,
            &fog_uniform,
            self.camera.near,
            self.camera.far,
        );
    }

    pub fn load_map(
        &mut self,
        gnd: &GndFile,
        rsw: &RswFile,
        grf: &GrfArchive,
        fog: Option<FogEntry>,
    ) {
        self.set_fog(fog);
        let held = self.texture_cache.map_texture_count();
        self.texture_cache.clear_map_textures();
        tracing::info!("map textures: dropped the last map's ({held} held, pinned kept)");

        let center_x = gnd.width as f32 * gnd.zoom / 2.0;
        let center_z = gnd.height as f32 * gnd.zoom / 2.0;
        self.camera.target = glam::Vec3::new(center_x, 0.0, center_z);

        let mut light = LightUniform::default();
        if let (Some(longitude), Some(latitude)) = (rsw.light.longitude, rsw.light.latitude) {
            let lon_rad = (longitude as f32).to_radians();
            let lat_rad = (latitude as f32).to_radians();
            let dir = glam::Vec3::new(
                -lon_rad.cos() * lat_rad.sin(),
                -lat_rad.cos(),
                -lon_rad.sin() * lat_rad.sin(),
            )
            .normalize();
            light.light_dir = [dir.x, dir.y, dir.z, 0.0];
        }
        self.map_sun = day_cycle::SunAngles {
            longitude: rsw.light.longitude.unwrap_or(0) as f32,
            latitude: rsw.light.latitude.unwrap_or(0) as f32,
        };
        self.sky_shown = None;
        self.map_darkens = true;
        self.shadow_generation += 1;
        if let Some(diffuse) = rsw.light.diffuse {
            light.diffuse_color = [diffuse[0], diffuse[1], diffuse[2], 1.0];
        }
        if let Some(ambient) = rsw.light.ambient {
            light.ambient_color = [ambient[0], ambient[1], ambient[2], 1.0];
        }
        self.base_light = light;
        self.legacy_diffuse = [light.diffuse_color[0], light.diffuse_color[1], light.diffuse_color[2]];
        self.refresh_light();

        let scale_factor = gnd.zoom / 10.0;
        let center_x = gnd.width as f32 * gnd.zoom / 2.0;
        let center_z = gnd.height as f32 * gnd.zoom / 2.0;
        let point_lights: Vec<PointLightGpu> = rsw
            .objects
            .iter()
            .filter_map(|obj| {
                if let RswObject::Light(l) = obj
                    && scene_light::is_placed(l.position)
                {
                    Some(PointLightGpu {
                        position: [
                            l.position[0] * scale_factor + center_x,
                            l.position[1] * scale_factor,
                            l.position[2] * scale_factor + center_z,
                            0.0,
                        ],
                        color_range: [l.color[0], l.color[1], l.color[2], l.range * scale_factor],
                    })
                } else {
                    None
                }
            })
            .collect();
        tracing::info!("Loaded {} RSW point lights", point_lights.len());
        self.point_lights = point_lights;
        self.point_light_center = None;

        let ground_renderer = GroundRenderer::from_gnd(
            gnd,
            grf,
            &self.device.device,
            &self.device.queue,
            &self.global_uniforms,
            &mut self.texture_cache,
            self.world_format,
        );
        self.ground_renderer = Some(ground_renderer);
        let cell_lightmap = gnd.has_lightmap_data().then(|| {
            let (w, h) = ground::cell_lightmap_size(gnd);
            (w, h, ground::pack_cell_lightmap(gnd))
        });
        self.global_uniforms.update_cell_light(
            &self.device.device,
            &self.device.queue,
            cell_lightmap
                .as_ref()
                .map(|(w, h, p)| (*w, *h, p.as_slice())),
            gnd.zoom,
            ground::LIGHTMAP_CELL_STRIDE as f32,
        );
        self.set_lightmap_enabled(self.lightmap_enabled);

        let props = ModelRenderer::from_rsw(
            rsw,
            gnd,
            grf,
            &self.device.device,
            &self.device.queue,
            &self.global_uniforms,
            &mut self.texture_cache,
            self.world_format,
        );
        self.model_renderer = props.static_models;
        self.animated_model_renderer = props.animated_models;
        self.model_floor_cells = props.floor_cells;
        self.lamp_lights = props.lamp_heads.into_iter().map(scene_light::lamp_light).collect();
        tracing::info!("{} lamp models light the night", self.lamp_lights.len());
        self.point_light_center = None;

        let water = map_water(&rsw.water, gnd);
        tracing::info!(
            "water: {}",
            match &water {
                None => "none".to_string(),
                Some(_) if rsw.water.level.is_some() => "rsw".to_string(),
                Some(w) => format!("gnd, {} zone(s), types {:?}", w.zones.len(), w.types()),
            }
        );
        self.water_renderer = water.as_ref().and_then(|water| WaterRenderer::from_map_water(
            water,
            gnd,
            grf,
            &self.device.device,
            &self.device.queue,
            &self.global_uniforms,
            &mut self.texture_cache,
            self.world_format,
        ));

        self.skill_unit_models.clear();
        self.gr2_models.clear();
        self.background_mode = BackgroundMode::RswMap;
    }

    pub fn has_skill_unit_model(&self, key: u32) -> bool {
        self.skill_unit_models.contains_key(&key)
    }

    pub fn add_skill_unit_model(
        &mut self,
        key: u32,
        rsm: &RsmFile,
        grf: &GrfArchive,
        world_pos: [f32; 3],
        scale_factor: f32,
    ) {
        if let Some(model) = ModelRenderer::from_rsm_at(
            rsm,
            grf,
            &self.device.device,
            &self.device.queue,
            &self.global_uniforms,
            &mut self.texture_cache,
            self.world_format,
            world_pos,
            scale_factor,
        ) {
            self.skill_unit_models.insert(key, model);
        }
    }

    pub fn remove_skill_unit_model(&mut self, key: u32) {
        self.skill_unit_models.remove(&key);
    }

    pub fn retain_skill_unit_models(&mut self, keep: &std::collections::HashSet<u32>) {
        self.skill_unit_models.retain(|k, _| keep.contains(k));
    }

    pub fn preload_effect_textures(&mut self, paths: &[String], grf: &GrfArchive) {
        for path in paths {
            self.load_effect_texture(path, grf);
        }
    }

    /// One effect texture into the cache, kept for a filtering change; false when the archive lacks it
    pub fn load_effect_texture(&mut self, path: &str, grf: &GrfArchive) -> bool {
        if self.texture_cache.get(path).is_some() {
            // a map model may have loaded it first: keep it past that map
            self.texture_cache.pin(path);
            return true;
        }
        let address_mode = if nostalre_effects::texture_wraps(path) {
            wgpu::AddressMode::Repeat
        } else {
            wgpu::AddressMode::ClampToEdge
        };
        let loaded = texture::load_keyed_texture(
            path,
            grf,
            &self.device.device,
            &self.device.queue,
            &self.texture_cache.bind_group_layout,
            address_mode,
            self.effect_filter(),
        );
        if nostalre_profiling::debug::trace_texture_load() {
            let status = if loaded.is_some() { "ok   " } else { "MISS " };
            tracing::info!("[effect-preload]   {status} {path}");
        }
        let Some((bind_group, w, h)) = loaded else {
            return false;
        };
        self.texture_cache.insert(path, bind_group, w, h);
        self.effect_textures.push(path.to_string());
        true
    }

    pub fn preload_textures(&mut self, paths: &[&str], grf: &GrfArchive) -> bool {
        let mut all_loaded = true;
        for path in paths {
            if self
                .texture_cache
                .get_or_load(path, grf, &self.device.device, &self.device.queue, true)
                .is_none()
            {
                all_loaded = false;
            }
        }
        all_loaded
    }

    /// Builds a Graffiti message decal and registers it under `key`. Returns false
    /// when the alphabet atlas is missing from the GRF.
    pub fn build_graffiti_texture(&mut self, key: &str, message: &str, grf: &GrfArchive) -> bool {
        let Ok(bytes) = grf.read_file(graffiti::ALPHABET_TEXTURE) else {
            return false;
        };
        let Ok(atlas) = image::load_from_memory_with_format(&bytes, image::ImageFormat::Bmp) else {
            return false;
        };
        let mut atlas = atlas.to_rgba8();
        nostalre_formats::apply_magenta_transparency(atlas.as_mut());
        let composed = graffiti::compose(&atlas, message);
        let (w, h) = (composed.width(), composed.height());
        let bind_group = texture::create_texture_bind_group_from_rgba(
            &self.device.device,
            &self.device.queue,
            composed.as_raw(),
            w,
            h,
            &self.texture_cache.bind_group_layout,
            key,
            wgpu::FilterMode::Linear,
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::AddressMode::ClampToEdge,
        );
        self.texture_cache.insert(key, bind_group, w, h);
        true
    }

    /// Changes the UI scale at runtime. The font atlas is re-rasterized at the
    /// new scale so text stays crisp; per-frame layout picks up `dpi_scale`.
    pub fn set_dpi_scale(&mut self, dpi_scale: f32) {
        if dpi_scale <= 0.0 || (dpi_scale - self.dpi_scale).abs() < f32::EPSILON {
            return;
        }
        self.dpi_scale = dpi_scale;
        self.font_atlas = FontAtlas::from_embedded(self.font_px_height, dpi_scale);
        self.font_atlas_bind_group = texture::create_font_atlas_bind_group(
            &self.device.device,
            &self.device.queue,
            &self.font_atlas.image,
            &self.texture_cache.bind_group_layout,
            "font_atlas",
        );
        self.font_atlas.release_image();
    }

    /// See [`RenderDevice::recreate_surface`]; the rest of the renderer follows the new size.
    pub fn recreate_surface(&mut self, window: std::sync::Arc<winit::window::Window>) {
        let size = window.inner_size();
        self.device.recreate_surface(window);
        self.resize(size.width, size.height);
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.device.resize(width, height);
        if width > 0 && height > 0 {
            self.camera.aspect = width as f32 / height as f32;
            let logical_w = width as f32 / self.dpi_scale;
            let logical_h = height as f32 / self.dpi_scale;
            self.sprite_renderer
                .resize(&self.device.queue, logical_w, logical_h);
            // `effect_sprite_renderer` carries its own `SpriteUniforms`; if we
            // skip it here its screen_size diverges from the camera's
            // projection screen_w/h and billboards shift off-center after the
            // first window resize.
            self.effect_sprite_renderer
                .resize(&self.device.queue, logical_w, logical_h);
            self.ui_renderer
                .resize(&self.device.queue, logical_w, logical_h);
        }
    }

    pub fn enable_ground_proxy(&mut self) {
        if self.ground_proxy.is_some() {
            return;
        }
        let proxy = GroundProxyRenderer::new(
            &self.device.device,
            self.world_format,
            &self.global_uniforms.bind_group_layout,
        );
        proxy.initialise(&self.device.queue);
        self.ground_proxy = Some(proxy);
    }

    pub fn render(&mut self, frame: FrameInputs) {
        nostalre_profiling::profile_function!();
        let output = match self.device.surface.get_current_texture() {
            Ok(tex) => tex,
            Err(e) => {
                if let Some(trace) = &mut self.frame_trace {
                    trace.skipped(&e.to_string(), std::time::Instant::now());
                }
                match e {
                    wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated => {
                        self.device.reconfigure_surface()
                    }
                    wgpu::SurfaceError::Timeout => {}
                    e => tracing::error!("Surface error: {e}"),
                }
                return;
            }
        };

        let scene_view = output.texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(self.device.scene_format),
            ..Default::default()
        });
        let depth_view = self.device.depth_view.clone();
        let phys_w = self.device.surface_config.width;
        let phys_h = self.device.surface_config.height;
        let clear = self.clear_color;
        let distortion = self.screen_distortion.is_active();
        // with HD or a screen distortion the frame goes through an offscreen target; both: HD composites into it
        let output_view = if distortion {
            self.screen_distortion
                .scene_view(&self.device.device, phys_w, phys_h)
        } else {
            scene_view
        };
        let hd_target = self.post.as_mut().map(|post| {
            post.resize(&self.device.device, phys_w, phys_h);
            (post.scene_texture().clone(), post.scene_view().clone())
        });
        if let Some((scene_texture, target_view)) = hd_target {
            self.render_into(
                &target_view,
                &output_view,
                &depth_view,
                Some(&scene_texture),
                phys_w,
                phys_h,
                clear,
                frame,
            );
        } else {
            let scene_texture = if distortion {
                self.screen_distortion.scene_texture()
            } else {
                self.device
                    .scene_copy_supported()
                    .then(|| output.texture.clone())
            };
            self.render_into(
                &output_view,
                &output_view,
                &depth_view,
                scene_texture.as_ref(),
                phys_w,
                phys_h,
                clear,
                frame,
            );
        }
        if distortion {
            let mut encoder = self
                .device
                .device
                .create_command_encoder(&Default::default());
            let view = output.texture.create_view(&Default::default());
            self.screen_distortion
                .resolve(&mut encoder, &self.device.queue, &view);
            self.device.queue.submit(std::iter::once(encoder.finish()));
        }
        output.present();
    }

    /// `scene_texture`: the texture behind `scene_view` when it can be copied from (COPY_SRC), for HD
    /// water; without one the water is drawn classic. With HD on, `scene_view` must be the HD target: it is
    /// composited into `ui_view` before the UI.
    #[allow(clippy::too_many_arguments)]
    pub fn render_into(
        &mut self,
        scene_view: &wgpu::TextureView,
        ui_view: &wgpu::TextureView,
        depth_view: &wgpu::TextureView,
        scene_texture: Option<&wgpu::Texture>,
        physical_w: u32,
        physical_h: u32,
        clear_color: wgpu::Color,
        frame: FrameInputs,
    ) {
        nostalre_profiling::profile_function!();
        if physical_w == 0 || physical_h == 0 {
            return;
        }
        let FrameInputs {
            ui_draw_calls,
            tooltip_draw_calls,
            effect_sprite_batches,
            effect_under_sprite_batches,
            effect_draws,
            sprite_particle_records,
            sprite_batches,
            silhouette_batches,
            mut after_ui_batches,
            inline_textures,
            elapsed,
            delta,
        } = frame;
        if let Some(trace) = &mut self.frame_trace {
            trace.frame(
                frame_trace::SpriteCount::of(sprite_batches),
                std::time::Instant::now(),
            );
        }
        let logical_w = physical_w as f32 / self.dpi_scale;
        let logical_h = physical_h as f32 / self.dpi_scale;
        self.camera.aspect = physical_w as f32 / physical_h as f32;
        self.sprite_renderer
            .resize(&self.device.queue, logical_w, logical_h);
        self.effect_sprite_renderer
            .resize(&self.device.queue, logical_w, logical_h);
        self.ui_sprite_renderer
            .resize(&self.device.queue, logical_w, logical_h);
        self.ui_renderer
            .resize(&self.device.queue, logical_w, logical_h);

        self.global_uniforms
            .update_camera(&self.device.queue, &self.camera);
        self.update_sky(delta);
        let draw_shadow = self.prepare_shadow();
        // the lights nearest the camera, again when it moved half a cell or the torch fades
        let center = self.camera.target;
        let torch_night = if self.hd_night() { self.night_now() } else { 0.0 };
        if self
            .point_light_center
            .is_none_or(|c| c.distance(center) > 5.0)
            || torch_night != self.point_light_torch
        {
            // the player's own torch only when its Settings row is on
            let torch = scene_light::torch(center, torch_night, !self.indoor)
                .filter(|_| self.scene.player_light);
            let lamps = scene_light::lamp_strength(torch_night, self.scene.lamp_light);
            let lights = scene_light::night_lights(&self.point_lights, &self.lamp_lights, lamps);
            let nearest = global_uniforms::nearest_point_lights(&lights, center, torch);
            self.global_uniforms
                .update_point_lights(&self.device.queue, &nearest);
            self.point_light_center = Some(center);
            self.point_light_torch = torch_night;
            self.point_lights_sent = nearest;
        }
        if self.hd_effects() && !self.indoor && self.scene.clouds > 0.0 {
            self.cloud_offset = scene_light::cloud_offset(elapsed);
            self.global_uniforms
                .update_cloud(&self.device.queue, self.cloud_offset);
        }

        let hd_water = self.hd_water && scene_texture.is_some();
        let caustics = match &self.water_renderer {
            Some(water) => {
                let screen = [
                    physical_w as f32,
                    physical_h as f32,
                    self.camera.near,
                    self.camera.far,
                ];
                water.update(&self.device.queue, elapsed, screen);
                hd_water
                    .then(|| water.caustics(elapsed))
                    .unwrap_or_default()
            }
            None => Default::default(),
        };
        self.global_uniforms
            .update_water(&self.device.queue, &caustics);

        // not drawn while `/hd` is off: no chunks made either
        let hd_effects = self.hd_effects();
        if let Some(grass) = self.grass.as_mut().filter(|_| hd_effects) {
            grass.update(
                &self.device.device,
                &self.device.queue,
                self.camera.view_projection(),
                self.camera.target,
                elapsed,
            );
        }

        if let Some(animated) = &mut self.animated_model_renderer {
            // A long stall must not fling props through their whole animation
            // in one step.
            animated.update(&self.device.queue, delta.clamp(0.0, 0.25));
        }

        let view = scene_view;
        let mut encoder = self
            .device
            .device
            .create_command_encoder(&Default::default());

        if draw_shadow && let Some(map) = &self.shadow_map {
            nostalre_profiling::profile_scope!("sun-shadow");
            let mut pass = map.begin(&mut encoder);
            if let Some(ground) = &self.ground_renderer {
                ground.render_shadow(&mut pass, &map.pass_bind_group, &self.texture_cache);
            }
            if let Some(model) = &self.model_renderer {
                model.render_shadow(&mut pass, &map.pass_bind_group, &self.texture_cache);
            }
            if let Some(animated) = &self.animated_model_renderer {
                animated.render_shadow(&mut pass, &map.pass_bind_group, &self.texture_cache);
            }
        }

        {
            nostalre_profiling::profile_scope!("scene-opaque");
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("main"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(clear_color),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });

            match self.background_mode {
                BackgroundMode::RswMap => {
                    if let Some(ground) = &self.ground_renderer {
                        nostalre_profiling::profile_scope!("ground");
                        ground.render(&mut pass, &self.global_uniforms, &self.texture_cache);
                    }
                    if let Some(model) = &self.model_renderer {
                        nostalre_profiling::profile_scope!("model");
                        model.render(&mut pass, &self.global_uniforms, &self.texture_cache);
                    }
                    if let Some(animated) = &self.animated_model_renderer {
                        nostalre_profiling::profile_scope!("animated-models");
                        animated.render(&mut pass, &self.global_uniforms, &self.texture_cache);
                    }
                    if let Some(grass) = self.grass.as_ref().filter(|_| self.hd_effects()) {
                        nostalre_profiling::profile_scope!("grass");
                        grass.render(&mut pass, &self.global_uniforms);
                    }
                    if !self.skill_unit_models.is_empty() {
                        nostalre_profiling::profile_scope!("skill-unit-models");
                        for model in self.skill_unit_models.values() {
                            model.render(&mut pass, &self.global_uniforms, &self.texture_cache);
                        }
                    }
                    if !self.gr2_models.is_empty() {
                        nostalre_profiling::profile_scope!("gr2-models");
                        for model in self.gr2_models.values() {
                            model.render(&mut pass, &self.gr2_pipeline, &self.global_uniforms);
                        }
                    }
                    if let Some(grid) = &self.grid_selector {
                        grid.render(&mut pass, &self.global_uniforms, &self.texture_cache);
                    }
                }
                BackgroundMode::GroundProxy => {
                    if let Some(proxy) = &self.ground_proxy {
                        proxy.render(&mut pass, &self.global_uniforms, &self.camera);
                    }
                }
                BackgroundMode::Clear => {}
            }
        }

        if !effect_draws.behind.is_empty() {
            nostalre_profiling::profile_scope!("effect-behind");
            let behind_list = effect_draws.behind_as_list();
            let mut behind_records = self.effect_behind_dispatcher.record_list();
            build_effect_records(
                &behind_list,
                &self.camera,
                &self.texture_cache,
                &self.white_bind_group,
                logical_w,
                logical_h,
                &self.effect_primitives,
                &mut behind_records,
            );
            if !behind_records.is_empty() {
                self.effect_behind_dispatcher.dispatch(
                    &behind_records,
                    &mut encoder,
                    &view,
                    depth_view,
                    &self.device.device,
                    &self.device.queue,
                    &self.global_uniforms.bind_group,
                    &self.effect_sprite_renderer.uniform_bind_group,
                    &self.effect_sprite_renderer,
                    &self.effect_primitives,
                );
            }
        }

        if !effect_under_sprite_batches.is_empty() {
            nostalre_profiling::profile_scope!("effect-under-sprite");
            self.effect_sprite_renderer.render_second(
                &mut encoder,
                view,
                Some(depth_view),
                &self.device.device,
                &self.device.queue,
                effect_under_sprite_batches,
            );
        }

        if !sprite_batches.is_empty() {
            nostalre_profiling::profile_scope!("sprite");
            self.sprite_renderer.render(
                &mut encoder,
                &view,
                Some(depth_view),
                &self.device.device,
                &self.device.queue,
                None,
                sprite_batches,
            );
        }

        // Stamp the flat-depth body silhouette after the colour pass so the
        // effect passes below occlude against the body (effects above the feet
        // draw on top; ground effects at the feet are hidden).
        if !silhouette_batches.is_empty() {
            nostalre_profiling::profile_scope!("silhouette");
            self.sprite_renderer.render_silhouette(
                &mut encoder,
                &view,
                depth_view,
                &self.device.device,
                &self.device.queue,
                silhouette_batches,
            );
        }

        // Water draws after the silhouette, never after the colour pass: the colour
        // pass writes no depth, so at that point the body's pixels still hold the
        // depth of the ground behind it and the surface would swallow the whole
        // sprite. Against the silhouette's flat feet depth the surface cuts the body
        // at the waterline instead, so a character wading in deep water is submerged
        // further than one in the shallows.
        if let (BackgroundMode::RswMap, Some(water)) =
            (self.background_mode, &mut self.water_renderer)
        {
            nostalre_profiling::profile_scope!("water");
            match scene_texture.filter(|_| hd_water) {
                Some(scene_texture) => water.render_hd(
                    &self.device.device,
                    &mut encoder,
                    water::HdWaterTarget {
                        scene_texture,
                        scene_view: view,
                        depth_view,
                    },
                    &self.global_uniforms,
                    &self.texture_cache,
                    elapsed,
                ),
                None => {
                    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("water"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &view,
                            depth_slice: None,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Load,
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                            view: depth_view,
                            depth_ops: Some(wgpu::Operations {
                                load: wgpu::LoadOp::Load,
                                store: wgpu::StoreOp::Store,
                            }),
                            stencil_ops: None,
                        }),
                        ..Default::default()
                    });
                    water.render(
                        &mut pass,
                        &self.global_uniforms,
                        &self.texture_cache,
                        elapsed,
                    );
                }
            }
        }

        if !effect_sprite_batches.is_empty() {
            nostalre_profiling::profile_scope!("effect-sprite");
            self.effect_sprite_renderer.render(
                &mut encoder,
                &view,
                Some(depth_view),
                &self.device.device,
                &self.device.queue,
                None,
                effect_sprite_batches,
            );
        }

        let mut records = self.effect_dispatcher.record_list();
        {
            nostalre_profiling::profile_scope!("effect-build");
            build_effect_records(
                effect_draws,
                &self.camera,
                &self.texture_cache,
                &self.white_bind_group,
                logical_w,
                logical_h,
                &self.effect_primitives,
                &mut records,
            );
        }
        records.extend(sprite_particle_records);
        if !records.is_empty() {
            nostalre_profiling::profile_scope!("effect-dispatch");
            self.effect_dispatcher.dispatch(
                &records,
                &mut encoder,
                &view,
                depth_view,
                &self.device.device,
                &self.device.queue,
                &self.global_uniforms.bind_group,
                &self.effect_sprite_renderer.uniform_bind_group,
                &self.effect_sprite_renderer,
                &self.effect_primitives,
            );
        }

        if let Some(post) = &mut self.post {
            post.run(
                &self.device.device,
                &self.device.queue,
                &mut encoder,
                ui_view,
                delta,
            );
        }

        {
            nostalre_profiling::profile_scope!("submit-scene");
            self.device.queue.submit(std::iter::once(encoder.finish()));
        }

        let mut encoder = self
            .device
            .device
            .create_command_encoder(&Default::default());

        if !ui_draw_calls.is_empty() {
            nostalre_profiling::profile_scope!("ui");
            let resolved: Vec<UiDrawCommand> = ui_draw_calls
                .iter()
                .map(|call| {
                    let bind_group = match &call.texture {
                        UiTextureRef::FontAtlas => &self.font_atlas_bind_group,
                        UiTextureRef::White => &self.white_bind_group,
                        UiTextureRef::Named(name) => self
                            .texture_cache
                            .get(name)
                            .unwrap_or(&self.white_bind_group),
                        UiTextureRef::Inline(idx) => inline_textures[*idx],
                    };
                    UiDrawCommand {
                        vertices: &call.vertices,
                        indices: &call.indices,
                        texture: bind_group,
                    }
                })
                .collect();

            self.ui_renderer.render(
                &mut encoder,
                ui_view,
                &self.device.device,
                &self.device.queue,
                &resolved,
            );
        }

        after_ui_batches.extend(tooltip_draw_calls.iter().map(|call| {
            SpriteBatch {
                vertices: call.vertices.map(|v| SpriteVertex {
                    position: [v.position[0], v.position[1], 0.0],
                    tex_coord: v.tex_coord,
                    color: v.color,
                }),
                indices: &call.indices,
                texture: match &call.texture {
                    UiTextureRef::FontAtlas => &self.font_atlas_bind_group,
                    UiTextureRef::White => &self.white_bind_group,
                    UiTextureRef::Named(name) => self
                        .texture_cache
                        .get(name)
                        .unwrap_or(&self.white_bind_group),
                    UiTextureRef::Inline(idx) => inline_textures[*idx],
                },
                additive: false,
                no_depth: false,
                weapon_aura: false,
            }
        }));

        if !after_ui_batches.is_empty() {
            nostalre_profiling::profile_scope!("cursor");
            self.ui_sprite_renderer.render(
                &mut encoder,
                ui_view,
                None,
                &self.device.device,
                &self.device.queue,
                None,
                &after_ui_batches,
            );
        }

        {
            nostalre_profiling::profile_scope!("submit-ui");
            self.device.queue.submit(std::iter::once(encoder.finish()));
        }
    }

    pub fn try_load_grf_font(&mut self, grf: &GrfArchive) {
        let extra_chars = font_atlas::euc_kr_charset();
        let font_paths = [
            nostalre_resources::font::NANUM_BARUN_GOTHIC_BOLD,
            nostalre_resources::font::NANUM_BARUN_GOTHIC,
        ];
        for path in &font_paths {
            if let Ok(data) = grf.read_file(path) {
                self.set_font_atlas(FontAtlas::build_with_extra_chars(
                    &data,
                    self.font_px_height,
                    self.dpi_scale,
                    &extra_chars,
                ));
                tracing::info!("Loaded GRF font: {path}");
                return;
            }
        }
        self.set_font_atlas(FontAtlas::from_embedded_cjk(
            self.font_px_height,
            self.dpi_scale,
            &extra_chars,
        ));
        tracing::info!("No GRF font found, using embedded CJK font for Korean text");
    }

    fn set_font_atlas(&mut self, atlas: FontAtlas) {
        self.font_atlas = atlas;
        self.font_atlas_bind_group = texture::create_font_atlas_bind_group(
            &self.device.device,
            &self.device.queue,
            &self.font_atlas.image,
            &self.texture_cache.bind_group_layout,
            "font_atlas",
        );
        self.font_atlas.release_image();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn background_mode_default_is_rsw_map() {
        assert_eq!(BackgroundMode::default(), BackgroundMode::RswMap);
    }

    #[test]
    fn fog_table_fractions_map_onto_the_original_view_planes() {
        assert_eq!(fog_world_distance(0.0, 1.0), 10.0);
        assert_eq!(fog_world_distance(1.0, 1.0), 1500.0);
        assert_eq!(fog_world_distance(0.2, 1.0), 308.0);
        assert_eq!(fog_world_distance(0.8, 1.0), 1202.0);

        assert_eq!(fog_world_distance(0.2, 2.0), 616.0);
        assert_eq!(fog_world_distance(0.8, 2.0), 2404.0);
    }

    #[test]
    fn every_shader_is_valid_wgsl() {
        let shaders: [(&str, &str); 16] = [
            ("sprite", SPRITE_SHADER_SRC),
            ("terrain", TERRAIN_WGSL),
            ("model", include_str!("shaders/model.wgsl")),
            (
                "model_animated",
                include_str!("shaders/model_animated.wgsl"),
            ),
            ("gr2_model", include_str!("shaders/gr2_model.wgsl")),
            ("water", include_str!("shaders/water.wgsl")),
            ("water_hd", WATER_HD_WGSL),
            ("ground_proxy", include_str!("shaders/ground_proxy.wgsl")),
            ("grid_selector", include_str!("shaders/grid_selector.wgsl")),
            ("ui", include_str!("shaders/ui.wgsl")),
            (
                "screen_distortion",
                include_str!("shaders/screen_distortion.wgsl"),
            ),
            (
                "effect_cylinder",
                include_str!("shaders/effect_cylinder.wgsl"),
            ),
            (
                "effect_frustum",
                include_str!("shaders/effect_frustum.wgsl"),
            ),
            (
                "effect_fullscreen",
                include_str!("shaders/effect_fullscreen.wgsl"),
            ),
            (
                "effect_ground_disc",
                include_str!("shaders/effect_ground_disc.wgsl"),
            ),
            ("effect_sphere", include_str!("shaders/effect_sphere.wgsl")),
        ];

        for (name, src) in shaders {
            let module = naga::front::wgsl::parse_str(src).unwrap_or_else(|e| {
                panic!("{name}.wgsl failed to parse: {}", e.emit_to_string(src))
            });
            naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::all(),
            )
            .validate(&module)
            .unwrap_or_else(|e| panic!("{name}.wgsl failed validation: {e:?}"));
        }
    }

    #[test]
    fn background_mode_cycle_round_trip() {
        let mut mode = BackgroundMode::default();
        let cycle = [
            BackgroundMode::GroundProxy,
            BackgroundMode::Clear,
            BackgroundMode::RswMap,
        ];
        for expected in cycle {
            mode = match mode {
                BackgroundMode::RswMap => BackgroundMode::GroundProxy,
                BackgroundMode::GroundProxy => BackgroundMode::Clear,
                BackgroundMode::Clear => BackgroundMode::RswMap,
            };
            assert_eq!(mode, expected);
        }
    }
}
