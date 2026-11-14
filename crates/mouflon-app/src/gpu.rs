//! Rendu GPU (sans calques Core Animation : Windows et Linux plus tard, ou
//! `MOUFLON_GPU=1` sur Mac pour comparer). Tout ce qui se montre est fait de
//! morceaux d'image, que [`Quick`] dessine directement dans la fenêtre :
//! quelques quadrilatères par animal. Une liste qui aurait encore des tracés
//! passe d'abord en morceaux, dessinés sur le processeur
//! ([`mouflon_render::pieces`]).

use std::borrow::Cow;
use std::sync::Arc;
use std::time::{Duration, Instant};

use mouflon_render::DisplayList;
use mouflon_render::pieces::{Pieces, flatten};
use winit::window::Window;

use crate::quick::Quick;

/// Format des fenêtres sur Mac (et le plus souvent ailleurs) : ses programmes
/// se préparent avant d'ouvrir la première fenêtre.
const USUAL_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8Unorm;

#[derive(Debug, thiserror::Error)]
pub enum GpuError {
    #[error("impossible de créer la surface : {0}")]
    Surface(#[from] wgpu::CreateSurfaceError),
    #[error("aucun GPU compatible")]
    NoDevice,
    #[error("format de surface non pris en charge")]
    Format,
    #[error("la fenêtre ne serait pas transparente avec {0}")]
    Opaque(String),
}

/// Instance wgpu et appareils ; l'appareil demande aussi la lecture des
/// textures compressées BC (BC7 pour les images des animaux) quand le GPU la
/// connaît : tous les Mac.
struct GpuContext {
    instance: wgpu::Instance,
    devices: Vec<DeviceHandle>,
}

struct DeviceHandle {
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
}

impl GpuContext {
    /// Sans fenêtre (les essais dessinent dans des textures).
    #[cfg(test)]
    fn new() -> Self {
        Self::with_display(None)
    }

    /// Le GPU, relié au système d'affichage de `display` (une fenêtre de
    /// Mouflon) : OpenGL en a besoin pour dessiner dans une fenêtre (sous
    /// Linux, quand Vulkan manque) ; Vulkan, Metal et DirectX s'en passent.
    fn with_display(display: Option<Arc<Window>>) -> Self {
        let mut backend_options = wgpu::BackendOptions::from_env_or_default();
        // Sous Windows, une fenêtre transparente passe par une surface de
        // composition (DirectComposition) : une surface faite droit sur la
        // fenêtre est toujours opaque. `WGPU_DX12_PRESENTATION_SYSTEM` garde
        // le dernier mot.
        if cfg!(target_os = "windows")
            && std::env::var_os("WGPU_DX12_PRESENTATION_SYSTEM").is_none()
        {
            backend_options.dx12.presentation_system = wgpu::Dx12SwapchainKind::DxgiFromVisual;
        }
        // Et seul DirectX 12 sait faire cette surface de composition : sur une
        // machine qui a aussi Vulkan (toutes celles qui ont une vraie carte
        // graphique), wgpu choisirait Vulkan d'abord, dont la surface est
        // opaque — la fenêtre de l'écran cachait alors tout le bureau.
        let backends = wgpu::Backends::from_env().unwrap_or(if cfg!(target_os = "windows") {
            wgpu::Backends::DX12
        } else {
            wgpu::Backends::default()
        });
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            display: display
                .map(|window| Box::new(window) as Box<dyn wgpu::wgt::WgpuHasDisplayHandle>),
            backends,
            flags: wgpu::InstanceFlags::from_build_config().with_env(),
            memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
            backend_options,
        });
        Self {
            instance,
            devices: Vec::new(),
        }
    }

    /// L'adaptateur qui dessinera : d'ordinaire celui que `wgpu` propose.
    ///
    /// Sous Linux, un Vulkan qui calcule sur le processeur (lavapipe, le seul
    /// Vulkan d'une machine virtuelle) passe après OpenGL : celui des
    /// distributions un peu anciennes ne montre rien dans une fenêtre
    /// transparente. `WGPU_BACKEND`, `WGPU_ADAPTER_NAME` et `WGPU_POWER_PREF`
    /// gardent le dernier mot.
    async fn adapter(
        &self,
        compatible_surface: Option<&wgpu::Surface<'_>>,
    ) -> Option<wgpu::Adapter> {
        let asked = ["WGPU_BACKEND", "WGPU_ADAPTER_NAME", "WGPU_POWER_PREF"]
            .iter()
            .any(|name| std::env::var_os(name).is_some());
        if cfg!(all(unix, not(target_os = "macos"))) && !asked {
            let best = self
                .instance
                .enumerate_adapters(wgpu::Backends::all())
                .await
                .into_iter()
                .filter(|a| compatible_surface.is_none_or(|s| a.is_surface_supported(s)))
                .min_by_key(|a| {
                    let info = a.get_info();
                    linux_rank(info.backend, info.device_type)
                });
            if best.is_some() {
                return best;
            }
        }
        wgpu::util::initialize_adapter_from_env_or_default(&self.instance, compatible_surface)
            .await
            .ok()
    }

    /// Un appareil qui peut dessiner dans `compatible_surface` (ou le premier
    /// venu), créé au besoin : son rang dans `devices`.
    async fn device(&mut self, compatible_surface: Option<&wgpu::Surface<'_>>) -> Option<usize> {
        let found = match compatible_surface {
            Some(surface) => self
                .devices
                .iter()
                .position(|d| d.adapter.is_surface_supported(surface)),
            None => (!self.devices.is_empty()).then_some(0),
        };
        if found.is_some() {
            return found;
        }
        let adapter = self.adapter(compatible_surface).await?;
        let wanted = wgpu::Features::CLEAR_TEXTURE
            | wgpu::Features::PIPELINE_CACHE
            | wgpu::Features::TEXTURE_COMPRESSION_BC
            | wgpu::Features::IMMEDIATES;
        let features = adapter.features() & wanted;
        // Les paramètres d'un élément du rendu direct (208 octets) partent
        // avec sa commande de dessin quand le GPU le permet.
        let immediates = if features.contains(wgpu::Features::IMMEDIATES) {
            adapter.limits().max_immediate_size.min(256)
        } else {
            0
        };
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: None,
                required_features: features,
                required_limits: wgpu::Limits {
                    max_immediate_size: immediates,
                    ..wgpu::Limits::default()
                },
                ..Default::default()
            })
            .await
            .ok()?;
        self.devices.push(DeviceHandle {
            adapter,
            device,
            queue,
        });
        Some(self.devices.len() - 1)
    }
}

/// Ce mode de composition donne-t-il une fenêtre transparente à coup sûr ?
/// Sous Windows (`strict`), seul le mode prémultiplié d'une surface de
/// composition le garantit : « hérité » y veut dire opaque, et sans mode
/// connu la fenêtre l'est toujours. Ailleurs, on fait au mieux (une fenêtre
/// X11 sans compositeur a un fond noir, mais ne couvre pas la souris ni la
/// barre des tâches de la même façon).
fn transparent_enough(mode: wgpu::CompositeAlphaMode, strict: bool) -> bool {
    !strict || mode == wgpu::CompositeAlphaMode::PreMultiplied
}

/// Rang d'un adaptateur sous Linux, le plus petit d'abord : une vraie carte
/// graphique par Vulkan, puis OpenGL, puis Vulkan calculé sur le processeur
/// (voir [`GpuContext::adapter`]).
fn linux_rank(backend: wgpu::Backend, kind: wgpu::DeviceType) -> u8 {
    match (backend, kind) {
        (wgpu::Backend::Vulkan, wgpu::DeviceType::Cpu) => 2,
        (wgpu::Backend::Vulkan, _) => 0,
        _ => 1,
    }
}

/// Une fenêtre à redessiner : sa surface, ce qu'on y dessine, la fenêtre.
#[derive(Debug)]
pub struct Job<'a> {
    pub surface: &'a mut PetSurface,
    pub list: &'a DisplayList,
    pub window: &'a Window,
}

/// Fenêtre de rendu (celle d'un écran).
pub struct PetSurface {
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    dev_id: usize,
}

impl std::fmt::Debug for PetSurface {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PetSurface")
            .field("size", &(self.config.width, self.config.height))
            .finish()
    }
}

pub struct Gpu {
    context: GpuContext,
    quick: Option<Quick>,
    /// Format des fenêtres (le premier vu) : les programmes sont prêts pour lui.
    format: Option<wgpu::TextureFormat>,
    timings: Timings,
}

/// Temps passé à dessiner (voir [`Gpu::take_timings`]).
#[derive(Clone, Copy, Debug, Default)]
pub struct Timings {
    /// Commandes de dessin préparées et envoyées au GPU.
    pub paint: Duration,
    /// Attente d'une image d'écran libre.
    pub acquire: Duration,
    /// Envoi à l'écran.
    pub present: Duration,
    /// La plus longue attente d'une image d'écran, pour une fenêtre.
    pub worst_acquire: Duration,
    /// Images d'écran refusées (surface à refaire, perdue, délai dépassé…).
    pub misses: u32,
}

impl std::fmt::Debug for Gpu {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Gpu")
            .field("devices", &self.context.devices.len())
            .finish()
    }
}

/// La liste telle que [`Quick`] la dessine : elle-même, ou ses morceaux
/// d'image (tracés et calques de fusion dessinés sur le processeur).
fn drawable(list: &DisplayList) -> Cow<'_, DisplayList> {
    if Quick::supports(list) {
        return Cow::Borrowed(list);
    }
    let mut pieces = Pieces::default();
    flatten(list, &mut pieces);
    let mut flat = DisplayList::new(list.width, list.height);
    pieces.write_list(&mut flat);
    Cow::Owned(flat)
}

impl Gpu {
    /// Le GPU qui dessinera dans les fenêtres du système d'affichage de
    /// `window` (voir [`GpuContext::with_display`]).
    #[must_use]
    pub fn for_window(window: Arc<Window>) -> Self {
        Self {
            context: GpuContext::with_display(Some(window)),
            quick: None,
            format: None,
            timings: Timings::default(),
        }
    }

    /// Temps passé à dessiner depuis le dernier appel.
    pub fn take_timings(&mut self) -> Timings {
        std::mem::take(&mut self.timings)
    }

    /// Images des animaux sur le GPU, et octets qu'elles occupent.
    #[must_use]
    pub fn texture_usage(&self) -> (usize, usize) {
        self.quick.as_ref().map_or((0, 0), Quick::texture_usage)
    }

    /// Oublie les images qui n'ont pas servi à la dernière image (voir
    /// [`Quick::forget_unused`]).
    pub fn forget_unused_images(&mut self) {
        if let Some(quick) = self.quick.as_mut() {
            quick.forget_unused();
        }
    }

    /// Crée la surface d'une fenêtre (et le GPU au premier appel).
    pub fn create_surface(
        &mut self,
        window: Arc<Window>,
        width: u32,
        height: u32,
    ) -> Result<PetSurface, GpuError> {
        let surface = self.context.instance.create_surface(window)?;
        let dev_id =
            pollster::block_on(self.context.device(Some(&surface))).ok_or(GpuError::NoDevice)?;
        let handle = &self.context.devices[dev_id];
        let caps = surface.get_capabilities(&handle.adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| {
                matches!(
                    f,
                    wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Rgba8Unorm
                )
            })
            .ok_or(GpuError::Format)?;
        // Les couleurs dessinées sont prémultipliées. (Metal n'annonce que
        // « PostMultiplied », qui chez lui rend seulement le calque non
        // opaque : Core Animation le compose bien en prémultiplié.)
        // (« Inherit » : la fenêtre décide ; sous X11, une fenêtre à couche
        // alpha est composée en prémultiplié.)
        let alpha_mode = [
            wgpu::CompositeAlphaMode::PreMultiplied,
            wgpu::CompositeAlphaMode::PostMultiplied,
            wgpu::CompositeAlphaMode::Inherit,
        ]
        .into_iter()
        .find(|m| caps.alpha_modes.contains(m))
        .unwrap_or(wgpu::CompositeAlphaMode::Auto);
        if !transparent_enough(alpha_mode, cfg!(target_os = "windows")) {
            // Sous Windows, une fenêtre d'écran opaque cache tout le bureau,
            // barre des tâches comprise, et ne se ferme pas au clavier : on
            // ne la montre jamais.
            let info = handle.adapter.get_info();
            return Err(GpuError::Opaque(format!(
                "{} ({:?})",
                info.name, info.backend
            )));
        }
        if alpha_mode == wgpu::CompositeAlphaMode::Auto {
            log::warn!("pas de mode de transparence de fenêtre : le fond risque d'être opaque");
        }
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: width.max(1),
            height: height.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            color_space: wgpu::SurfaceColorSpace::Auto,
            desired_maximum_frame_latency: 2,
            alpha_mode,
            view_formats: vec![],
        };
        surface.configure(&handle.device, &config);
        self.prepare(dev_id, format);
        self.format.get_or_insert(format);
        Ok(PetSurface {
            surface,
            config,
            dev_id,
        })
    }

    /// Le GPU et l'interface graphique employés (« Apple M2, Metal, intégré »),
    /// une fois l'appareil créé.
    #[must_use]
    pub fn describe(&self) -> Option<String> {
        let info = self.context.devices.first()?.adapter.get_info();
        let kind = match info.device_type {
            wgpu::DeviceType::IntegratedGpu => "intégré",
            wgpu::DeviceType::DiscreteGpu => "séparé",
            wgpu::DeviceType::VirtualGpu => "virtuel",
            wgpu::DeviceType::Cpu => "logiciel",
            wgpu::DeviceType::Other => "autre",
        };
        let bc7 = self
            .context
            .devices
            .first()?
            .device
            .features()
            .contains(wgpu::Features::TEXTURE_COMPRESSION_BC);
        Some(format!(
            "{}, {:?}, {kind}{}",
            info.name,
            info.backend,
            if bc7 { ", images BC7" } else { "" }
        ))
    }

    /// Programmes du rendu direct pour des fenêtres au format `format`.
    fn prepare(&mut self, dev_id: usize, format: wgpu::TextureFormat) {
        let device = &self.context.devices[dev_id].device;
        self.quick
            .get_or_insert_with(|| Quick::new(device))
            .prepare(device, format);
    }

    /// Met le GPU en route avant même la première fenêtre, pendant que les
    /// dessins se décodent à côté : appareil, puis programmes du rendu direct
    /// pour le format habituel des fenêtres.
    pub fn prepare_ahead(&mut self) {
        match pollster::block_on(self.context.device(None)) {
            Some(dev_id) => self.prepare(dev_id, USUAL_FORMAT),
            None => log::warn!("pas de GPU pour l'instant"),
        }
    }

    /// Met le GPU en route avant d'ouvrir la moindre fenêtre : appareil,
    /// programmes, puis une image d'essai (les dessins des animaux du
    /// lancement) faite hors écran jusqu'au bout. La première chute ne paie
    /// plus ces mises en route.
    pub fn warm_up(&mut self, list: &DisplayList) -> Result<(), GpuError> {
        let dev_id = pollster::block_on(self.context.device(None)).ok_or(GpuError::NoDevice)?;
        // Le format des fenêtres déjà ouvertes, sinon le format habituel.
        let format = self.format.unwrap_or(USUAL_FORMAT);
        self.prepare(dev_id, format);
        let handle = &self.context.devices[dev_id];
        let (width, height) = (list.width.max(1), list.height.max(1));
        let window = handle.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("mouflon-essai"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            view_formats: &[],
        });
        let view = window.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = handle
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("mouflon-essai"),
            });
        if let Some(quick) = self.quick.as_mut() {
            quick.render(
                &handle.device,
                &handle.queue,
                &mut encoder,
                &view,
                format,
                &drawable(list),
            );
        }
        handle.queue.submit([encoder.finish()]);
        if let Err(e) = handle.device.poll(wgpu::PollType::wait_indefinitely()) {
            log::warn!("mise en route du GPU : {e}");
        }
        Ok(())
    }

    pub fn resize(&self, surface: &mut PetSurface, width: u32, height: u32) {
        let (width, height) = (width.max(1), height.max(1));
        if surface.config.width == width && surface.config.height == height {
            return;
        }
        let device = &self.context.devices[surface.dev_id].device;
        surface.config.width = width;
        surface.config.height = height;
        surface.surface.configure(device, &surface.config);
    }

    /// Dessine les fenêtres de l'image en cours, en un seul envoi au GPU :
    /// leurs passes s'enchaînent dans une même liste de commandes, puis
    /// chacune s'affiche. Renvoie, fenêtre par fenêtre, si elle a été
    /// dessinée.
    ///
    /// Toutes les fenêtres sont sur le même GPU (un Mac n'en montre qu'un aux
    /// fenêtres) : c'est le sien qui reçoit l'envoi.
    pub fn render_all(&mut self, jobs: &mut [Job<'_>]) -> Vec<bool> {
        let mut done = vec![false; jobs.len()];
        let lists: Vec<Cow<'_, DisplayList>> = jobs.iter().map(|job| drawable(job.list)).collect();
        let Some(quick) = self.quick.as_mut() else {
            return done;
        };
        let started = Instant::now();
        let mut frames: Vec<(usize, wgpu::SurfaceTexture)> = Vec::with_capacity(jobs.len());
        for (i, job) in jobs.iter().enumerate() {
            let asked = Instant::now();
            let frame = Self::acquire(&self.context, job.surface);
            self.timings.worst_acquire = self.timings.worst_acquire.max(asked.elapsed());
            match frame {
                Some(frame) => frames.push((i, frame)),
                None => self.timings.misses += 1,
            }
        }
        let acquired = Instant::now();
        self.timings.acquire += acquired - started;
        let Some(&(first, _)) = frames.first() else {
            return done;
        };
        let handle = &self.context.devices[jobs[first].surface.dev_id];
        let draws = frames.iter().map(|&(i, _)| Quick::draws(&lists[i])).sum();
        quick.begin(&handle.device, draws);
        let mut encoder = handle
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("mouflon"),
            });
        for (i, frame) in &frames {
            let view = frame
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());
            quick.draw(
                &handle.device,
                &handle.queue,
                &mut encoder,
                &view,
                jobs[*i].surface.config.format,
                &lists[*i],
            );
        }
        quick.finish(&handle.queue);
        handle.queue.submit([encoder.finish()]);
        let painted = Instant::now();
        self.timings.paint += painted - acquired;
        for (i, frame) in frames {
            jobs[i].window.pre_present_notify();
            handle.queue.present(frame);
            done[i] = true;
        }
        self.timings.present += painted.elapsed();
        done
    }

    /// Image de la fenêtre à dessiner, ou `None` (surface à refaire, écran perdu…).
    fn acquire(context: &GpuContext, surface: &PetSurface) -> Option<wgpu::SurfaceTexture> {
        match surface.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t)
            | wgpu::CurrentSurfaceTexture::Suboptimal(t) => Some(t),
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                let device = &context.devices[surface.dev_id].device;
                surface.surface.configure(device, &surface.config);
                None
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    //! Le rendu direct donne la même image que le dessin sur le processeur,
    //! à l'arrondi près.
    use std::path::Path;
    use std::sync::Arc;

    use mouflon_core::{Particle, ParticleKind, Shadow, Support, Vec2, load_pet};
    use mouflon_render::display::{Bitmap, Command};
    use mouflon_render::kurbo::Affine;
    use mouflon_render::{DisplayList, Frame, PetArt, Style, paint_pet};

    use super::*;

    const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

    #[test]
    fn on_windows_a_screen_window_is_only_shown_if_it_is_sure_to_be_transparent() {
        use wgpu::CompositeAlphaMode::{Auto, Inherit, Opaque, PostMultiplied, PreMultiplied};
        assert!(transparent_enough(PreMultiplied, true));
        for mode in [Auto, Inherit, Opaque, PostMultiplied] {
            assert!(!transparent_enough(mode, true), "{mode:?}");
            // Ailleurs, on fait au mieux.
            assert!(transparent_enough(mode, false), "{mode:?}");
        }
    }

    #[test]
    fn on_linux_a_real_card_comes_first_then_opengl_then_vulkan_on_the_processor() {
        use wgpu::{Backend, DeviceType};
        let mut found = [
            (Backend::Vulkan, DeviceType::Cpu),
            (Backend::Gl, DeviceType::Cpu),
            (Backend::Gl, DeviceType::Other),
            (Backend::Vulkan, DeviceType::IntegratedGpu),
            (Backend::Vulkan, DeviceType::DiscreteGpu),
        ];
        // Le tri garde l'ordre d'arrivée des adaptateurs de même rang, comme
        // le choix de l'app (le premier des meilleurs).
        found.sort_by_key(|&(backend, kind)| linux_rank(backend, kind));
        assert_eq!(
            found,
            [
                (Backend::Vulkan, DeviceType::IntegratedGpu),
                (Backend::Vulkan, DeviceType::DiscreteGpu),
                (Backend::Gl, DeviceType::Cpu),
                (Backend::Gl, DeviceType::Other),
                (Backend::Vulkan, DeviceType::Cpu),
            ]
        );
    }

    /// Lit une texture RGBA 8 bits (lignes alignées comme l'exige wgpu).
    fn read(device: &wgpu::Device, queue: &wgpu::Queue, texture: &wgpu::Texture) -> Vec<u8> {
        let (w, h) = (texture.width(), texture.height());
        let row = (w * 4).div_ceil(256) * 256;
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: u64::from(row * h),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: Some(h),
                },
            },
            texture.size(),
        );
        queue.submit([encoder.finish()]);
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, |r| r.expect("lecture"));
        device
            .poll(wgpu::PollType::wait_indefinitely())
            .expect("attente du GPU");
        let data = buffer.slice(..).get_mapped_range().expect("tampon lisible");
        let mut out = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            let start = (y * row) as usize;
            out.extend_from_slice(&data[start..start + (w * 4) as usize]);
        }
        out
    }

    fn target(device: &wgpu::Device, list: &DisplayList) -> wgpu::Texture {
        device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: list.width,
                height: list.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            format: FORMAT,
            view_formats: &[],
        })
    }

    /// Écarts entre deux images : le plus grand, la moyenne, et la part des
    /// octets qui s'écartent de plus de 6/255.
    fn compare(a: &[u8], b: &[u8]) -> (u8, f64, f64) {
        let mut max = 0u8;
        let mut sum = 0u64;
        let mut far = 0usize;
        for (x, y) in a.iter().zip(b) {
            let d = x.abs_diff(*y);
            max = max.max(d);
            sum += u64::from(d);
            far += usize::from(d > 6);
        }
        (
            max,
            sum as f64 / a.len() as f64,
            far as f64 / a.len() as f64,
        )
    }

    /// Lainou et Pelote (des données de test), images compressées ou non.
    fn pets(compressed: bool) -> Vec<(Arc<mouflon_core::PetDef>, PetArt)> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        // La Solognote (des morceaux peints sur un squelette) n'a pas de
        // planche à compresser.
        let dirs: &[&str] = if compressed {
            &["tests/donnees/lainou", "tests/donnees/pelote"]
        } else {
            &[
                "tests/donnees/lainou",
                "tests/donnees/pelote",
                "pets/solognote",
            ]
        };
        dirs.iter()
            .map(|dir| {
                let def = Arc::new(load_pet(&root.join(dir).join("pet.toml")).expect("pet.toml"));
                let mut art = PetArt::load(&def).expect("dessin");
                if compressed {
                    art = art.compressed(8, &|| {}).expect("images à compresser");
                }
                (def, art)
            })
            .collect()
    }

    /// Listes d'affichage de Lainou et Pelote dans des situations variées, à
    /// deux échelles : (nom, liste).
    fn sample_lists(compressed: bool) -> Vec<(String, DisplayList)> {
        let world = mouflon_core::World::new(vec![mouflon_core::Screen {
            frame: mouflon_core::Rect::new(0.0, 0.0, 1440.0, 900.0),
            usable: mouflon_core::Rect::new(0.0, 25.0, 1440.0, 800.0),
            scale: 2.0,
        }]);
        let dust: Vec<Particle> = (0..6)
            .map(|i| Particle {
                kind: match i % 3 {
                    0 => ParticleKind::Dust,
                    1 => ParticleKind::Bubble,
                    _ => ParticleKind::Leaf,
                },
                pos: Vec2::new(590.0 + i as f32 * 9.0, 760.0 - i as f32 * 7.0),
                vel: Vec2::new(0.0, 0.0),
                age: 0.2 + i as f32 * 0.15,
                life: 1.2,
                size: 4.0 + i as f32,
                phase: 0.0,
            })
            .collect();
        let mut lists = Vec::new();
        for (def, art) in pets(compressed) {
            let mut pet = mouflon_core::Pet::spawn(0, Arc::clone(&def), 7, &world, Some(600.0));
            for _ in 0..600 {
                pet.step(mouflon_core::TICK, &world, &mouflon_core::Input::default());
            }
            let canvas = art.canvas();
            for scale in [2.0, 1.0] {
                for case in 0..7 {
                    let mut view = pet.view(1.0);
                    match case {
                        // Au repos, tourné d'un côté puis de l'autre.
                        0 => view.facing = 1.0,
                        1 => view.facing = -1.0,
                        // Culbute : l'image tourne (lissée).
                        2 => view.pose.roll = 0.9,
                        // En l'air : ombre floue sous lui.
                        3 => {
                            view.support = Support::Air;
                            view.shadow = Some(Shadow::on(view.ground.y + 30.0, 6.0));
                            view.pose.lean = 0.3;
                        }
                        // Il s'efface.
                        4 => view.opacity = 0.45,
                        // Écrasé à l'atterrissage.
                        5 => view.pose.squash = 0.25,
                        // Poussière et bulles.
                        _ => view.particles = &dust,
                    }
                    let mut list = DisplayList::new(
                        (f64::from(canvas.width) * scale) as u32,
                        (f64::from(canvas.height) * scale) as u32,
                    );
                    let frame = Frame {
                        origin: view.ground - canvas.anchor,
                        scale,
                    };
                    paint_pet(&art, &view, &frame, &Style::default(), &mut list);
                    let name = format!("{} ×{scale} cas {case}", def.meta.name);
                    assert!(Quick::supports(&list), "{name}");
                    lists.push((name, list));
                }
            }
        }
        lists
    }

    /// Les animaux, leurs particules (feuilles d'un coup de vent comprises)
    /// et la soucoupe se dessinent tels quels sur le GPU : rien ne passe
    /// d'abord par le processeur.
    #[test]
    fn pets_particles_and_saucer_draw_directly() {
        // Chaque liste est vérifiée en chemin.
        assert!(!sample_lists(false).is_empty());
        let canvas = mouflon_render::saucer::saucer_canvas();
        for beam in [0.0, 1.0] {
            let view = mouflon_core::SaucerView {
                pos: Vec2::new(500.0, 300.0),
                tilt: 0.1,
                beam,
                beam_len: 180.0,
                lights: 0.4,
                smoke: 0.0,
                smoke_time: 0.0,
            };
            let mut list = DisplayList::new(
                (f64::from(canvas.width) * 2.0) as u32,
                (f64::from(canvas.height) * 2.0) as u32,
            );
            let frame = Frame {
                origin: view.pos - canvas.anchor,
                scale: 2.0,
            };
            mouflon_render::saucer::paint_saucer(&view, &frame, &Style::default(), &mut list);
            assert!(Quick::supports(&list), "soucoupe, faisceau {beam}");
        }
    }

    /// Le dessin d'une liste par le processeur (tiny-skia), en pixels RGBA
    /// prémultipliés.
    fn on_the_processor(list: &DisplayList) -> Vec<u8> {
        mouflon_render::raster::rasterize(list)
            .expect("dessin sur le processeur")
            .data()
            .to_vec()
    }

    /// Le rendu direct (sur le GPU) d'une liste en morceaux d'image donne la
    /// même image que leur dessin sur le processeur : mêmes images, mêmes
    /// places, mêmes opacités, au lissage des images tournées près.
    #[test]
    fn direct_rendering_matches_the_processor() {
        let mut context = GpuContext::new();
        let Some(dev_id) = pollster::block_on(context.device(None)) else {
            eprintln!("pas de GPU ici : comparaison sautée");
            return;
        };
        let handle = &context.devices[dev_id];
        let (device, queue) = (&handle.device, &handle.queue);
        let mut quick = Quick::new(device);
        let mut pieces = mouflon_render::pieces::Pieces::default();
        let mut worst = (0u8, 0.0f64, 0.0f64);
        for (name, list) in sample_lists(false) {
            mouflon_render::pieces::flatten(&list, &mut pieces);
            let mut flat = DisplayList::new(list.width, list.height);
            pieces.write_list(&mut flat);
            let fast = target(device, &flat);
            let fast_view = fast.create_view(&Default::default());
            let mut encoder = device.create_command_encoder(&Default::default());
            quick.render(device, queue, &mut encoder, &fast_view, FORMAT, &flat);
            queue.submit([encoder.finish()]);
            let (a, b) = (read(device, queue, &fast), on_the_processor(&flat));
            if let Ok(dir) = std::env::var("MOUFLON_COMPARAISON") {
                let file = format!("{dir}/{}", name.replace([' ', '×'], "-"));
                for (suffix, pixels) in [("gpu", &a), ("processeur", &b)] {
                    let bitmap = mouflon_render::display::Bitmap::from_rgba(
                        list.width,
                        list.height,
                        pixels.clone(),
                    );
                    let png = mouflon_render::sprites::encode_png_rgba(&bitmap);
                    std::fs::write(format!("{file}-{suffix}.png"), png).expect("png");
                }
            }
            let (max, mean, far) = compare(&a, &b);
            eprintln!(
                "{name} : écart max {max}, moyen {mean:.4}, {:.3} % au-delà de 6",
                far * 100.0
            );
            worst = (worst.0.max(max), worst.1.max(mean), worst.2.max(far));
            assert!(
                a.iter().skip(3).step_by(4).any(|&alpha| alpha > 0),
                "image vide"
            );
        }
        eprintln!("pire : {worst:?}");
        // Deux sur 255 au plus aujourd'hui (le lissage des images tournées).
        assert!(worst.0 <= 4, "écart max {}", worst.0);
        assert!(worst.1 < 0.05, "écart moyen {}", worst.1);
    }

    /// Plusieurs calques d'opacité l'un après l'autre (des dessins en
    /// morceaux qui s'effacent ensemble) : chacun se dessine à part, dans une
    /// texture à sa mesure, puis se pose avec son opacité — la même image
    /// que chaque liste seule, posée au même endroit.
    #[test]
    fn several_fading_drawings_each_fade_as_one() {
        let mut context = GpuContext::new();
        let Some(dev_id) = pollster::block_on(context.device(None)) else {
            eprintln!("pas de GPU ici : comparaison sautée");
            return;
        };
        let handle = &context.devices[dev_id];
        let (device, queue) = (&handle.device, &handle.queue);
        let mut quick = Quick::new(device);
        // Trois morceaux qui se chevauchent, dans un calque à moitié opaque.
        let square = |shade: u8| {
            mouflon_render::display::Bitmap::from_rgba(40, 40, vec![shade; 40 * 40 * 4])
        };
        let fading = |list: &mut DisplayList, at: (f64, f64), alpha: f32| {
            let bounds = list.bounds();
            list.push_layer(
                mouflon_render::paint::shape_path(&bounds),
                mouflon_render::kurbo::Affine::IDENTITY,
                alpha,
                mouflon_render::peniko::Mix::Normal,
            );
            for (k, shade) in [200u8, 120, 255].into_iter().enumerate() {
                list.push(mouflon_render::display::Command::Image {
                    bitmap: square(shade),
                    transform: mouflon_render::kurbo::Affine::translate((
                        at.0 + 12.0 * k as f64,
                        at.1 + 7.0 * k as f64,
                    )),
                    alpha: 1.0,
                    smooth: false,
                });
            }
            list.pop_layer();
        };
        let (w, h) = (320, 200);
        let mut both = DisplayList::new(w, h);
        fading(&mut both, (20.0, 30.0), 0.5);
        fading(&mut both, (180.0, 90.0), 0.8);
        assert!(Quick::supports(&both));
        let mut first = DisplayList::new(w, h);
        fading(&mut first, (20.0, 30.0), 0.5);
        let mut second = DisplayList::new(w, h);
        fading(&mut second, (180.0, 90.0), 0.8);
        let images = [&both, &first, &second].map(|list| {
            let texture = target(device, list);
            let view = texture.create_view(&Default::default());
            let mut encoder = device.create_command_encoder(&Default::default());
            quick.render(device, queue, &mut encoder, &view, FORMAT, list);
            queue.submit([encoder.finish()]);
            read(device, queue, &texture)
        });
        // Les deux calques ne se touchent pas : l'image des deux est la
        // somme des images de chacun.
        let sum: Vec<u8> = images[1]
            .iter()
            .zip(&images[2])
            .map(|(a, b)| a.saturating_add(*b))
            .collect();
        assert!(
            images[0] == sum,
            "deux calques à part : une image différente"
        );
        // Opacité d'un bloc : là où les trois carrés se chevauchent, on ne
        // voit que celui du dessus (255), à moitié.
        let at = |x: usize, y: usize| &images[1][(y * w as usize + x) * 4..][..4];
        assert!(
            at(20 + 24 + 5, 30 + 14 + 5)
                .iter()
                .all(|c| (127..=128).contains(c)),
            "{:?}",
            at(20 + 24 + 5, 30 + 14 + 5)
        );
        // Et le calque est vide hors de ses morceaux.
        assert_eq!(at(5, 5), [0, 0, 0, 0]);
    }

    /// Tout en morceaux d'image (ombres et disques cuits, voir
    /// `mouflon_render::pieces`) : la même image que la liste d'origine,
    /// à quelques pixels près (les images cuites sont lissées).
    #[test]
    fn pieces_draw_like_the_list() {
        let mut context = GpuContext::new();
        let Some(dev_id) = pollster::block_on(context.device(None)) else {
            eprintln!("pas de GPU ici : comparaison sautée");
            return;
        };
        let handle = &context.devices[dev_id];
        let (device, queue) = (&handle.device, &handle.queue);
        let mut quick = Quick::new(device);
        let mut lists = sample_lists(false);
        let canvas = mouflon_render::saucer::saucer_canvas();
        for beam in [0.0, 1.0] {
            let view = mouflon_core::SaucerView {
                pos: Vec2::new(500.0, 300.0),
                tilt: 0.1,
                beam,
                beam_len: 180.0,
                lights: 0.4,
                smoke: 0.0,
                smoke_time: 0.0,
            };
            let mut list = DisplayList::new(
                (f64::from(canvas.width) * 2.0) as u32,
                (f64::from(canvas.height) * 2.0) as u32,
            );
            let frame = Frame {
                origin: view.pos - canvas.anchor,
                scale: 2.0,
            };
            mouflon_render::saucer::paint_saucer(&view, &frame, &Style::default(), &mut list);
            lists.push((format!("soucoupe, faisceau {beam}"), list));
        }
        let mut pieces = mouflon_render::pieces::Pieces::default();
        let mut worst = (0u8, 0.0f64, 0.0f64);
        for (name, list) in lists {
            mouflon_render::pieces::flatten(&list, &mut pieces);
            assert!(!pieces.rasterized, "{name} : dessiné d'un bloc");
            let mut flat = DisplayList::new(list.width, list.height);
            pieces.write_list(&mut flat);
            let images = [&list, &flat].map(|list| {
                let texture = target(device, list);
                let view = texture.create_view(&Default::default());
                let mut encoder = device.create_command_encoder(&Default::default());
                quick.render(device, queue, &mut encoder, &view, FORMAT, list);
                queue.submit([encoder.finish()]);
                read(device, queue, &texture)
            });
            if let Ok(dir) = std::env::var("MOUFLON_COMPARAISON") {
                let file = format!("{dir}/{}", name.replace([' ', '×', ','], "-"));
                for (suffix, pixels) in [("liste", &images[0]), ("morceaux", &images[1])] {
                    let bitmap = mouflon_render::display::Bitmap::from_rgba(
                        list.width,
                        list.height,
                        pixels.clone(),
                    );
                    let png = mouflon_render::sprites::encode_png_rgba(&bitmap);
                    std::fs::write(format!("{file}-{suffix}.png"), png).expect("png");
                }
            }
            let (max, mean, far) = compare(&images[0], &images[1]);
            eprintln!(
                "{name} : écart max {max}, moyen {mean:.4}, {:.3} % au-delà de 6",
                far * 100.0
            );
            worst = (worst.0.max(max), worst.1.max(mean), worst.2.max(far));
        }
        eprintln!("pire : {worst:?}");
        assert!(worst.1 < 0.35, "écart moyen {}", worst.1);
        assert!(worst.2 < 0.01, "trop de pixels différents : {}", worst.2);
    }

    /// Un troupeau en morceaux : rangés en pages, ils partent en quelques
    /// appels de dessin au lieu d'un par morceau, pour la même image.
    #[test]
    fn a_flock_of_pieces_draws_in_a_few_calls_and_looks_the_same() {
        let mut context = GpuContext::new();
        let Some(dev_id) = pollster::block_on(context.device(None)) else {
            eprintln!("pas de GPU ici : comparaison sautée");
            return;
        };
        let handle = &context.devices[dev_id];
        let (device, queue) = (&handle.device, &handle.queue);
        for compressed in [false, true] {
            // Tous les animaux d'essai, posés quarante fois chacun sur une
            // grande fenêtre (tournés, réduits, à demi effacés pour
            // certains).
            let mut flock = DisplayList::new(1200, 800);
            let mut pieces = mouflon_render::pieces::Pieces::default();
            let mut count = 0usize;
            for (n, (_, list)) in sample_lists(compressed).iter().enumerate() {
                mouflon_render::pieces::flatten(list, &mut pieces);
                for k in 0..40usize {
                    let i = n * 40 + k;
                    let at = Affine::translate((
                        40.0 + (i * 137 % 1000) as f64 + 0.37 * k as f64,
                        30.0 + (i * 89 % 600) as f64 + 0.21 * k as f64,
                    )) * Affine::rotate(0.05 * (k % 7) as f64)
                        * Affine::scale(0.6 + 0.1 * (k % 5) as f64);
                    for piece in &pieces.items {
                        // Sans calque d'opacité : chaque morceau a la sienne.
                        let fade = if k % 9 == 0 { 0.5 } else { 1.0 };
                        flock.commands.push(Command::Image {
                            bitmap: piece.bitmap.clone(),
                            transform: at * piece.transform,
                            alpha: piece.alpha * fade,
                            smooth: piece.smooth,
                        });
                        count += 1;
                    }
                }
            }
            assert!(count > 400, "{count} morceaux");
            let mut paged = Quick::new(device);
            let mut alone = Quick::new(device).without_pages();
            let images = [&mut paged, &mut alone].map(|quick| {
                let texture = target(device, &flock);
                let view = texture.create_view(&Default::default());
                let mut encoder = device.create_command_encoder(&Default::default());
                quick.render(device, queue, &mut encoder, &view, FORMAT, &flock);
                queue.submit([encoder.finish()]);
                read(device, queue, &texture)
            });
            assert_eq!(alone.calls(), count, "un appel par morceau sans les pages");
            assert!(
                paged.calls() <= 4,
                "{} appels pour {count} morceaux (compressés : {compressed})",
                paged.calls()
            );
            // La même image, à l'arrondi du calcul des coordonnées près :
            // une image lue au pixel le plus proche (un animal en images,
            // ici tourné et réduit exprès) peut changer de pixel juste à la
            // limite entre deux, sur quelques points.
            let (max, mean, far) = compare(&images[0], &images[1]);
            eprintln!(
                "compressés {compressed} : {count} morceaux en {} appel(s), écart max {max}, \
                 moyen {mean:.5}, {:.4} % au-delà de 6",
                paged.calls(),
                far * 100.0
            );
            // (Le rendu logiciel de Windows arrondit autrement d'un octet
            // sur trente : l'écart moyen reste loin sous un niveau.)
            assert!(far < 0.0005, "{:.4} % des octets au-delà de 6", far * 100.0);
            assert!(mean < 0.1, "écart moyen {mean}");
            assert!(images[0].iter().any(|&b| b != 0), "rien de dessiné");
        }
    }

    /// Au-delà de leur budget, les pages qui ne servent plus sont rendues ;
    /// leurs images, redemandées, sont rangées de nouveau et se dessinent
    /// comme avant.
    #[test]
    fn pages_beyond_their_budget_are_given_back_and_their_pieces_placed_again() {
        let mut context = GpuContext::new();
        let Some(dev_id) = pollster::block_on(context.device(None)) else {
            eprintln!("pas de GPU ici : comparaison sautée");
            return;
        };
        let handle = &context.devices[dev_id];
        let (device, queue) = (&handle.device, &handle.queue);
        // Dix-sept grandes images : seize remplissent une page.
        let bitmaps: Vec<Bitmap> = (0..17u32)
            .map(|n| {
                let mut rgba = vec![0u8; 510 * 510 * 4];
                for (i, px) in rgba.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                    let (x, y) = ((i % 510) as u32, (i / 510) as u32);
                    px.copy_from_slice(&[
                        (x * 7 + n * 40) as u8,
                        (y * 5 + n * 90) as u8,
                        (n * 15) as u8,
                        255,
                    ]);
                }
                Bitmap::from_rgba(510, 510, rgba)
            })
            .collect();
        let list_of = |range: std::ops::Range<usize>| {
            let mut list = DisplayList::new(256, 256);
            for (k, bitmap) in bitmaps[range].iter().enumerate() {
                list.commands.push(Command::Image {
                    bitmap: bitmap.clone(),
                    transform: Affine::translate((
                        4.0 + 60.0 * (k % 4) as f64,
                        4.0 + 60.0 * (k / 4) as f64,
                    )) * Affine::scale(0.1),
                    alpha: 1.0,
                    smooth: true,
                });
            }
            list
        };
        let (first, last) = (list_of(0..16), list_of(16..17));
        let mut quick = Quick::new(device).with_pages_budget(1);
        let draw = |quick: &mut Quick, list: &DisplayList| {
            let texture = target(device, list);
            let view = texture.create_view(&Default::default());
            let mut encoder = device.create_command_encoder(&Default::default());
            quick.render(device, queue, &mut encoder, &view, FORMAT, list);
            queue.submit([encoder.finish()]);
            read(device, queue, &texture)
        };
        let before = draw(&mut quick, &first);
        assert_eq!((quick.pages(), quick.calls()), (1, 1));
        // Une dix-septième : une deuxième page, et la première, qui ne sert
        // pas à cette image, est rendue.
        draw(&mut quick, &last);
        assert_eq!(quick.pages(), 1);
        // Les seize premières reviennent : rangées de nouveau (quinze à
        // côté de la dix-septième, la dernière dans une page neuve), elles
        // donnent la même image, en un seul appel qui lit les deux pages ;
        // toutes deux servent, aucune n'est rendue.
        let after = draw(&mut quick, &first);
        assert_eq!((quick.pages(), quick.calls()), (2, 1));
        assert!(before == after, "les images rangées de nouveau diffèrent");
        assert!(before.iter().any(|&b| b != 0), "rien de dessiné");
        // La dix-septième seule : la page neuve, qui ne sert plus, est rendue.
        draw(&mut quick, &last);
        assert_eq!((quick.pages(), quick.calls()), (1, 1));
    }

    /// Paramètres passés avec chaque commande (« immediates ») ou par un
    /// tampon uniforme : exactement la même image.
    #[test]
    fn immediates_and_uniforms_draw_the_same() {
        let mut context = GpuContext::new();
        let Some(dev_id) = pollster::block_on(context.device(None)) else {
            eprintln!("pas de GPU ici : comparaison sautée");
            return;
        };
        let handle = &context.devices[dev_id];
        let (device, queue) = (&handle.device, &handle.queue);
        let mut fast = Quick::new(device);
        let mut slow = Quick::with_uniforms(device);
        eprintln!(
            "« immediates » : {} (jusqu'à {} octets)",
            device.features().contains(wgpu::Features::IMMEDIATES),
            device.limits().max_immediate_size
        );
        for (name, list) in sample_lists(false) {
            let images = [&mut fast, &mut slow].map(|quick| {
                let texture = target(device, &list);
                let view = texture.create_view(&Default::default());
                let mut encoder = device.create_command_encoder(&Default::default());
                quick.render(device, queue, &mut encoder, &view, FORMAT, &list);
                queue.submit([encoder.finish()]);
                read(device, queue, &texture)
            });
            assert!(images[0] == images[1], "{name} : images différentes");
        }
    }

    /// Les images compressées en BC7 donnent, au rendu direct, presque
    /// exactement les mêmes images que les originales : lues par le GPU, ou
    /// décompressées avant de partir quand il ne sait pas les lire.
    #[test]
    fn compressed_images_look_like_the_originals() {
        let mut context = GpuContext::new();
        let Some(dev_id) = pollster::block_on(context.device(None)) else {
            eprintln!("pas de GPU ici : comparaison sautée");
            return;
        };
        let handle = &context.devices[dev_id];
        let (device, queue) = (&handle.device, &handle.queue);
        let bc7 = device
            .features()
            .contains(wgpu::Features::TEXTURE_COMPRESSION_BC);
        eprintln!("textures BC7 lues par le GPU : {bc7}");
        let originals = sample_lists(false);
        let compressed = sample_lists(true);
        for (gpu_reads_bc7, mut quick) in [
            (bc7, Quick::new(device)),
            (false, Quick::new(device).without_bc7()),
        ] {
            let mut worst = (0u8, 0.0f64, 0.0f64);
            for ((name, original), (_, packed)) in originals.iter().zip(&compressed) {
                let images = [original, packed].map(|list| {
                    let texture = target(device, list);
                    let view = texture.create_view(&Default::default());
                    let mut encoder = device.create_command_encoder(&Default::default());
                    quick.render(device, queue, &mut encoder, &view, FORMAT, list);
                    queue.submit([encoder.finish()]);
                    read(device, queue, &texture)
                });
                let (max, mean, far) = compare(&images[0], &images[1]);
                eprintln!(
                    "{name} (BC7 lu par le GPU : {gpu_reads_bc7}) : écart max {max}, moyen \
                     {mean:.4}, {:.3} % au-delà de 6",
                    far * 100.0
                );
                worst = (worst.0.max(max), worst.1.max(mean), worst.2.max(far));
            }
            eprintln!("pire : {worst:?}");
            assert!(worst.1 < 0.2, "écart moyen {}", worst.1);
            assert!(worst.2 < 0.01, "trop de pixels différents : {}", worst.2);
        }
    }

    /// Un dessin vectoriel cuit en images (le Bleu du Maine), posé par le rendu
    /// direct, ressemble à ses tracés dessinés par le processeur : mêmes
    /// formes, même éclairage, aux bords près (les images sont posées
    /// tournées et étirées).
    #[test]
    fn a_baked_rig_looks_like_its_vector_drawing() {
        let mut context = GpuContext::new();
        let Some(dev_id) = pollster::block_on(context.device(None)) else {
            eprintln!("pas de GPU ici : comparaison sautée");
            return;
        };
        let handle = &context.devices[dev_id];
        let (device, queue) = (&handle.device, &handle.queue);
        let mut quick = Quick::new(device);
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../pets/bleu-du-maine/pet.toml");
        let def = Arc::new(load_pet(&root).expect("pet.toml"));
        let baked = PetArt::load(&def).expect("dessin");
        assert!(baked.baked.is_some(), "le Bleu du Maine se cuit");
        let mut vector = baked.clone();
        vector.baked = None;
        let world = mouflon_core::World::new(vec![mouflon_core::Screen {
            frame: mouflon_core::Rect::new(0.0, 0.0, 1440.0, 900.0),
            usable: mouflon_core::Rect::new(0.0, 25.0, 1440.0, 800.0),
            scale: 2.0,
        }]);
        let mut pet = mouflon_core::Pet::spawn(0, Arc::clone(&def), 5, &world, Some(600.0));
        let mut worst = (0u8, 0.0f64, 0.0f64);
        for case in 0..6 {
            for _ in 0..90 {
                pet.step(mouflon_core::TICK, &world, &mouflon_core::Input::default());
            }
            let mut view = pet.view(1.0);
            match case {
                0 => view.facing = 1.0,
                1 => view.facing = -1.0,
                2 => view.pose.squash = 0.2,
                3 => view.pose.roll = 0.5,
                // Il s'efface : ses morceaux ne se voient pas les uns à travers les autres.
                4 => view.opacity = 0.45,
                _ => {}
            }
            let canvas = vector.canvas();
            let lists = [&baked, &vector].map(|art| {
                let mut list = DisplayList::new(
                    (f64::from(canvas.width) * 2.0) as u32,
                    (f64::from(canvas.height) * 2.0) as u32,
                );
                let frame = Frame {
                    origin: view.ground - canvas.anchor,
                    scale: 2.0,
                };
                paint_pet(art, &view, &frame, &Style::default(), &mut list);
                list
            });
            let [fast_list, slow_list] = &lists;
            assert!(Quick::supports(fast_list), "cas {case} : rendu direct");
            let fast = target(device, fast_list);
            let fast_view = fast.create_view(&Default::default());
            let mut encoder = device.create_command_encoder(&Default::default());
            quick.render(device, queue, &mut encoder, &fast_view, FORMAT, fast_list);
            queue.submit([encoder.finish()]);
            let (a, b) = (read(device, queue, &fast), on_the_processor(slow_list));
            if let Ok(dir) = std::env::var("MOUFLON_COMPARAISON") {
                for (suffix, pixels) in [("cuit", &a), ("traces", &b)] {
                    let bitmap = mouflon_render::display::Bitmap::from_rgba(
                        fast_list.width,
                        fast_list.height,
                        pixels.clone(),
                    );
                    let png = mouflon_render::sprites::encode_png_rgba(&bitmap);
                    std::fs::write(format!("{dir}/bleu-du-maine-{case}-{suffix}.png"), png)
                        .expect("png");
                }
            }
            let (max, mean, far) = compare_far(&a, &b, 24);
            eprintln!(
                "Bleu du Maine cas {case} ({}) : écart max {max}, moyen {mean:.3}, {:.3} % au-delà de 24",
                pet.behavior_name(),
                far * 100.0
            );
            worst = (worst.0.max(max), worst.1.max(mean), worst.2.max(far));
        }
        eprintln!("pire : {worst:?}");
        assert!(worst.1 < 1.5, "écart moyen {}", worst.1);
        assert!(worst.2 < 0.02, "trop de pixels différents : {}", worst.2);
    }

    /// Comme [`compare`], avec un seuil d'écart choisi.
    fn compare_far(a: &[u8], b: &[u8], threshold: u8) -> (u8, f64, f64) {
        let (max, mean, _) = compare(a, b);
        let far = a
            .iter()
            .zip(b)
            .filter(|(x, y)| x.abs_diff(**y) > threshold)
            .count();
        (max, mean, far as f64 / a.len() as f64)
    }

    /// Plusieurs fenêtres en un seul envoi : chacune reçoit exactement l'image
    /// qu'elle aurait eue seule, même quand le tampon de paramètres doit
    /// grandir en route.
    #[test]
    fn one_submit_draws_each_window_as_if_alone() {
        let mut context = GpuContext::new();
        let Some(dev_id) = pollster::block_on(context.device(None)) else {
            eprintln!("pas de GPU ici : essai sauté");
            return;
        };
        let handle = &context.devices[dev_id];
        let (device, queue) = (&handle.device, &handle.queue);
        let mut quick = Quick::new(device);
        // Trois fois chaque situation : bien plus d'éléments que le tampon
        // de départ n'en reçoit.
        let lists: Vec<(String, DisplayList)> =
            sample_lists(false).into_iter().cycle().take(84).collect();
        let draws: usize = lists.iter().map(|(_, list)| Quick::draws(list)).sum();
        assert!(
            draws > crate::quick::FIRST_CAPACITY,
            "{draws} éléments seulement"
        );

        let alone: Vec<Vec<u8>> = lists
            .iter()
            .map(|(_, list)| {
                let texture = target(device, list);
                let view = texture.create_view(&Default::default());
                let mut encoder = device.create_command_encoder(&Default::default());
                quick.render(device, queue, &mut encoder, &view, FORMAT, list);
                queue.submit([encoder.finish()]);
                read(device, queue, &texture)
            })
            .collect();

        let textures: Vec<wgpu::Texture> =
            lists.iter().map(|(_, list)| target(device, list)).collect();
        quick.begin(device, draws);
        let mut encoder = device.create_command_encoder(&Default::default());
        for ((_, list), texture) in lists.iter().zip(&textures) {
            let view = texture.create_view(&Default::default());
            quick.draw(device, queue, &mut encoder, &view, FORMAT, list);
        }
        quick.finish(queue);
        queue.submit([encoder.finish()]);
        for (((name, _), texture), expected) in lists.iter().zip(&textures).zip(&alone) {
            assert!(
                read(device, queue, texture) == *expected,
                "{name} : image différente"
            );
        }
    }
}
