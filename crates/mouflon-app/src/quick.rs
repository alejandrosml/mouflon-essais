//! Rendu direct des animaux en images.
//!
//! Un animal en sprites, c'est une image (une case de sa planche), une ombre
//! floue et quelques particules rondes. Un moteur de tracés sur le GPU
//! (Vello, autrefois) sait tout dessiner, mais au prix d'une dizaine de
//! passes de calcul sur toute la fenêtre à chaque image, de tampons de
//! travail et d'une texture intermédiaire. Ici, chaque élément est un simple
//! quadrilatère dessiné droit dans la fenêtre, et son programme calcule sa
//! couleur pixel par pixel :
//!
//! - **image** : la case, téléversée une fois (alpha prémultiplié, bordée
//!   d'un pixel transparent pour que ses bords tournés restent lisses),
//!   lue au plus proche ou par interpolation bilinéaire ; une image
//!   compressée en BC7 part telle quelle, et le GPU la décode en la lisant
//!   (s'il ne sait pas, elle est décompressée avant de partir) ;
//! - **ombre** : rectangle arrondi flouté, par la même approximation
//!   analytique que Vello (Raph Levien, « Blurred rounded rectangles ») ;
//! - **disque** : cercle antialiasé, rempli d'une couleur ou d'un dégradé
//!   (linéaire, ou radial à deux cercles comme en SVG).
//!
//! Les paramètres de chaque élément (une douzaine de `vec4`) partent avec
//! la commande de dessin elle-même (« immediates » : `setVertexBytes` sur
//! Metal) : ni tampon à remplir à chaque image, ni tampon de transit à créer
//! puis détruire. Sur un GPU qui ne le permet pas, ils passent par un tampon
//! uniforme, une place par élément choisie par décalage dynamique.
//!
//! Les images d'un troupeau sont petites et très nombreuses (une trentaine
//! de morceaux par mouton dessiné) : elles sont rangées côte à côte dans de
//! grandes **pages** ([`PAGE`] pixels de côté), et tous les morceaux qui se
//! suivent partent en **un seul appel de dessin**, une ligne de tampon par
//! morceau (sa place à l'écran, sa place dans sa page, son opacité). Un
//! appel lit jusqu'à quatre pages à la fois (des images compressées et
//! d'autres qui ne le sont pas n'ont pas les mêmes) : mille moutons, c'est
//! trente mille quadrilatères et, presque toujours, un seul appel. Une
//! image trop grande pour une page garde sa texture et son appel à elle.
//!
//! Un calque d'opacité qui ne contient qu'un élément revient à multiplier
//! l'opacité de cet élément : c'est le cas d'un animal en images qui
//! s'efface. Un calque de plusieurs éléments (un dessin vectoriel cuit en
//! morceaux, qui s'efface) se dessine à part dans une texture à sa mesure,
//! posée ensuite d'un coup avec son opacité : les morceaux ne se voient pas
//! les uns à travers les autres. Il peut y en avoir autant que d'animaux qui
//! s'effacent. Une liste qui a des tracés (ou des calques de fusion) passe
//! d'abord en morceaux d'image, dessinés sur le processeur : voir
//! [`Quick::supports`] et [`mouflon_render::pieces`].

use std::collections::HashMap;
use std::num::NonZeroU64;

use mouflon_render::display::{Bitmap, Command, DisplayList, Paint, Pixels, Stop};
use mouflon_render::kurbo::{Affine, Point, Rect, Shape, Vec2};
use mouflon_render::peniko::{Color, Extend, Mix};

/// Place d'un élément dans le tampon de paramètres : l'alignement exigé
/// pour un décalage dynamique (256 octets sur tous les GPU).
const SLOT: usize = 256;
/// Octets des paramètres d'un élément (`Draw` : treize `vec4`).
const DRAW_BYTES: u32 = 13 * 16;
/// Déclaration des paramètres dans le programme, et sa version « immediates ».
const UNIFORM_DRAW: &str = "@group(0) @binding(0) var<uniform> draw: Draw;";
const IMMEDIATE_DRAW: &str = "var<immediate> draw: Draw;";
/// Places du tampon de paramètres au départ ; il double quand une image de
/// toutes les fenêtres en demande davantage.
pub(crate) const FIRST_CAPACITY: usize = 64;
/// Arrêts de couleur d'un dégradé, au plus.
const MAX_STOPS: usize = 4;
/// Images gardées sur le GPU, en octets : au-delà, les moins récemment
/// montrées sont oubliées (et renvoyées si elles reservent).
const TEXTURE_BUDGET: usize = 32 << 20;
/// Côté d'une page d'images, en pixels.
const PAGE: u32 = 2048;
/// Une image entre dans une page si aucun de ses côtés ne dépasse ceci.
const PAGE_PIECE: u32 = 512;
/// Pages gardées sur le GPU, en octets : au-delà, celles qui n'ont pas servi
/// depuis le plus longtemps sont rendues (leurs images sont rangées de
/// nouveau si elles reservent).
const PAGES_BUDGET: usize = 96 << 20;
/// Hauteurs des rangées d'une page : chaque image va dans la rangée de la
/// plus petite hauteur qui la contient (au plus un tiers de perdu), toutes
/// multiples de quatre (les blocs des images compressées).
const SHELF_HEIGHTS: [u32; 13] = [8, 12, 16, 24, 32, 48, 64, 96, 128, 192, 256, 384, 512];
/// Pages qu'un même appel de dessin peut lire.
const CALL_PAGES: usize = 4;
/// Pas de page à ce rang d'un appel.
const NO_PAGE: u32 = u32::MAX;
/// Octets d'un morceau dans le tampon des morceaux (`Piece` : trois `vec4`).
const PIECE_BYTES: usize = 48;
/// Morceaux que le tampon reçoit au départ.
const FIRST_PIECES: usize = 1024;

const SHADER: &str = r"
struct Draw {
    // Quadrilatère, en pixels de la fenêtre : origine et deux côtés.
    quad0: vec4<f32>,   // origine x, y ; côté u x, y
    quad1: vec4<f32>,   // côté v x, y ; largeur, hauteur de la fenêtre
    // Pixel → repère de l'élément : local = M · pixel + t.
    inv0: vec4<f32>,    // M : colonne x (x, y), colonne y (x, y)
    inv1: vec4<f32>,    // t x, y ; opacité ; drapeau (image lisse, sorte de remplissage du disque)
    color: vec4<f32>,   // ombre : couleur prémultipliée ; disque : nombre d'arrêts en x
    shape: vec4<f32>,   // ombre : largeur, hauteur, rayon, écart type ; disque : centre x, y, rayon, pixels par unité
    grad0: vec4<f32>,   // dégradé : départ x, y, rayon de départ, extension
    grad1: vec4<f32>,   // dégradé : arrivée x, y, rayon d'arrivée, -
    offsets: vec4<f32>,
    stops: array<vec4<f32>, 4>,
}

@group(0) @binding(0) var<uniform> draw: Draw;
@group(0) @binding(1) var nearest: sampler;
@group(0) @binding(2) var bilinear: sampler;
@group(1) @binding(0) var image: texture_2d<f32>;
// Les autres pages d'un appel de morceaux (`image` est la première).
@group(1) @binding(1) var page1: texture_2d<f32>;
@group(1) @binding(2) var page2: texture_2d<f32>;
@group(1) @binding(3) var page3: texture_2d<f32>;
// Côté d'une page, en pixels.
const PAGE_SIDE: f32 = 2048.0;

struct Varyings {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs(@builtin(vertex_index) index: u32) -> Varyings {
    let uv = vec2(f32(index & 1u), f32(index >> 1u));
    let p = draw.quad0.xy + draw.quad0.zw * uv.x + draw.quad1.xy * uv.y;
    let ndc = vec2(p.x / draw.quad1.z * 2.0 - 1.0, 1.0 - p.y / draw.quad1.w * 2.0);
    return Varyings(vec4(ndc, 0.0, 1.0), uv);
}

fn to_local(pixel: vec2<f32>) -> vec2<f32> {
    return draw.inv0.xy * pixel.x + draw.inv0.zw * pixel.y + draw.inv1.xy;
}

// Où lire l'image pour `uv` : là même si elle est lisse (`soft`), sinon au
// centre du pixel le plus proche, où l'interpolation ne mélange rien. Un
// seul échantillonneur suffit ainsi aux deux lectures (OpenGL n'en accepte
// pas deux pour une même image).
fn texel(uv: vec2<f32>, soft: f32) -> vec2<f32> {
    if soft > 0.5 {
        return uv;
    }
    let size = vec2<f32>(textureDimensions(image));
    return (floor(uv * size) + 0.5) / size;
}

@fragment
fn fs_image(in: Varyings) -> @location(0) vec4<f32> {
    let c = textureSampleLevel(image, bilinear, texel(in.uv, draw.inv1.w), 0.0);
    return c * draw.inv1.z;
}

// Un morceau d'image rangé dans une page : une ligne du tampon des morceaux.
struct Piece {
    @location(0) quad0: vec4<f32>,  // origine x, y ; côté u x, y (pixels de la fenêtre)
    @location(1) quad1: vec4<f32>,  // côté v x, y ; opacité ; image lisse (0 ou 1) + deux fois le rang de sa page
    @location(2) rect: vec4<f32>,   // sa place dans la page : u0, v0, u1, v1
}

struct PieceVaryings {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) @interpolate(flat) tint: vec2<f32>,
    @location(2) @interpolate(flat) rect: vec4<f32>,
}

@vertex
fn vs_pieces(@builtin(vertex_index) index: u32, piece: Piece) -> PieceVaryings {
    let c = vec2(f32(index & 1u), f32(index >> 1u));
    let p = piece.quad0.xy + piece.quad0.zw * c.x + piece.quad1.xy * c.y;
    let ndc = vec2(p.x / draw.quad1.z * 2.0 - 1.0, 1.0 - p.y / draw.quad1.w * 2.0);
    let uv = mix(piece.rect.xy, piece.rect.zw, c);
    return PieceVaryings(vec4(ndc, 0.0, 1.0), uv, piece.quad1.zw, piece.rect);
}

@fragment
fn fs_pieces(in: PieceVaryings) -> @location(0) vec4<f32> {
    let flag = u32(in.tint.y + 0.5);
    var uv = in.uv;
    if flag % 2u == 0u {
        // Au pixel le plus proche (voir `texel`).
        uv = (floor(uv * PAGE_SIDE) + 0.5) / PAGE_SIDE;
    }
    // Comme au bord d'une image seule : jamais un pixel de la voisine.
    let half = vec2(0.5 / PAGE_SIDE);
    uv = clamp(uv, in.rect.xy + half, in.rect.zw - half);
    var c: vec4<f32>;
    switch flag / 2u {
        case 1u { c = textureSampleLevel(page1, bilinear, uv, 0.0); }
        case 2u { c = textureSampleLevel(page2, bilinear, uv, 0.0); }
        case 3u { c = textureSampleLevel(page3, bilinear, uv, 0.0); }
        default { c = textureSampleLevel(image, bilinear, uv, 0.0); }
    }
    return c * in.tint.x;
}

// Fonction d'erreur approchée (celle de Vello).
fn erf7(x: f32) -> f32 {
    let y = clamp(x * 1.1283791671, -100.0, 100.0);
    let yy = y * y;
    let z = y + (0.24295 + (0.03395 + 0.0104 * yy) * yy) * (y * yy);
    return z / sqrt(1.0 + z * z);
}

@fragment
fn fs_shadow(in: Varyings) -> @location(0) vec4<f32> {
    // Vello évalue l'ombre au coin du pixel.
    let local = to_local(in.position.xy - vec2(0.5));
    let w = draw.shape.x;
    let h = draw.shape.y;
    let radius = draw.shape.z;
    let std_dev = max(draw.shape.w, 1e-5);
    let inv_std_dev = 1.0 / std_dev;
    let min_edge = min(w, h);
    let radius_max = 0.5 * min_edge;
    let r0 = min(length(vec2(radius, std_dev * 1.15)), radius_max);
    let r1 = min(length(vec2(radius, std_dev * 2.0)), radius_max);
    let exponent = 2.0 * r1 / r0;
    let delta = 1.25 * std_dev
        * (exp(-pow(0.5 * inv_std_dev * w, 2.0)) - exp(-pow(0.5 * inv_std_dev * h, 2.0)));
    let width = w + min(delta, 0.0);
    let height = h - max(delta, 0.0);
    let scale = 0.5 * erf7(inv_std_dev * 0.5 * (max(width, height) - 0.5 * radius));
    let y0 = abs(local.y) - (height * 0.5 - r1);
    let x0 = abs(local.x) - (width * 0.5 - r1);
    let d_pos = pow(pow(max(x0, 0.0), exponent) + pow(max(y0, 0.0), exponent), 1.0 / exponent);
    let d_neg = min(max(x0, y0), 0.0);
    let d = d_pos + d_neg - r1;
    let alpha = scale * (erf7(inv_std_dev * (min_edge + d)) - erf7(inv_std_dev * d));
    return draw.color * clamp(alpha, 0.0, 1.0) * draw.inv1.z;
}

fn extend(t: f32, mode: f32) -> f32 {
    if mode > 1.5 {
        // Miroir.
        let r = t - 2.0 * floor(t * 0.5);
        return select(r, 2.0 - r, r > 1.0);
    }
    if mode > 0.5 {
        return t - floor(t);
    }
    return clamp(t, 0.0, 1.0);
}

fn ramp(t: f32) -> vec4<f32> {
    let n = i32(draw.color.x);
    if t <= draw.offsets[0] || n < 2 {
        return draw.stops[0];
    }
    for (var k = 1; k < n; k++) {
        if t <= draw.offsets[k] {
            let span = max(draw.offsets[k] - draw.offsets[k - 1], 1e-6);
            return mix(draw.stops[k - 1], draw.stops[k], (t - draw.offsets[k - 1]) / span);
        }
    }
    return draw.stops[n - 1];
}

// Dégradé radial à deux cercles : le plus grand t pour lequel le point est
// sur le cercle de centre départ + t (arrivée - départ), de rayon
// r0 + t (r1 - r0) positif.
fn radial(p: vec2<f32>) -> f32 {
    let f = draw.grad0.xy;
    let r0 = draw.grad0.z;
    let d = draw.grad1.xy - f;
    let dr = draw.grad1.z - r0;
    let q = p - f;
    let a = dot(d, d) - dr * dr;
    let b = dot(q, d) + r0 * dr;
    let c = dot(q, q) - r0 * r0;
    if abs(a) < 1e-6 {
        return c / max(2.0 * b, 1e-6);
    }
    let s = sqrt(max(b * b - a * c, 0.0));
    let t1 = (b + s) / a;
    let t2 = (b - s) / a;
    let high = max(t1, t2);
    return select(min(t1, t2), high, r0 + high * dr >= 0.0);
}

@fragment
fn fs_disc(in: Varyings) -> @location(0) vec4<f32> {
    let p = to_local(in.position.xy);
    let center = draw.shape.xy;
    let distance = (length(p - center) - draw.shape.z) * draw.shape.w;
    let cover = clamp(0.5 - distance, 0.0, 1.0);
    var color = draw.stops[0];
    if draw.inv1.w > 1.5 {
        color = ramp(extend(radial(p), draw.grad0.w));
    } else if draw.inv1.w > 0.5 {
        let d = draw.grad1.xy - draw.grad0.xy;
        let t = dot(p - draw.grad0.xy, d) / max(dot(d, d), 1e-12);
        color = ramp(extend(t, draw.grad0.w));
    }
    return color * cover * draw.inv1.z;
}
";

/// Sorte d'élément, donc de programme.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Image,
    Shadow,
    Disc,
    /// Des morceaux d'une même page, en un appel (voir [`Call::pieces`]).
    Pieces,
}

/// Un élément à dessiner : son programme, sa place dans le tampon de
/// paramètres et son image.
#[derive(Clone, Copy, Debug)]
struct Call {
    kind: Kind,
    image: Option<u64>,
    /// Dans ce calque à part (son rang dans `Quick::layers`), dessiné dans
    /// sa texture puis posé d'un coup.
    layer: Option<u32>,
    /// Pose ce calque à part (image : la texture du calque).
    composite: Option<u32>,
    /// Des morceaux rangés en pages : le premier et leur nombre, dans le
    /// tampon des morceaux, et les pages qu'ils lisent (leurs numéros).
    pieces: (u32, u32),
    pages: [u32; CALL_PAGES],
}

/// Une texture où se dessine un calque à part, gardée d'une image à l'autre.
#[derive(Debug)]
struct Spare {
    format: wgpu::TextureFormat,
    size: (u32, u32),
    view: wgpu::TextureView,
    bind_group: wgpu::BindGroup,
    _texture: wgpu::Texture,
    /// Dernière image où elle a servi (voir `Quick::tick`).
    used: u64,
}

/// Un calque à part de l'image en cours : sa texture (son rang dans
/// `Quick::spares`) et son opacité.
#[derive(Clone, Copy, Debug)]
struct LayerJob {
    spare: usize,
    alpha: f32,
}

/// Une texture de calque inutilisée depuis ce nombre d'images est rendue.
const SPARE_IDLE: u64 = 600;

/// Les trois programmes, pour un format de fenêtre.
#[derive(Debug)]
struct Pipelines {
    image: wgpu::RenderPipeline,
    shadow: wgpu::RenderPipeline,
    disc: wgpu::RenderPipeline,
    pieces: wgpu::RenderPipeline,
}

/// Une image sur le GPU.
#[derive(Debug)]
struct Texture {
    bind_group: wgpu::BindGroup,
    _texture: wgpu::Texture,
    bytes: usize,
    last_used: u64,
}

/// Les rangées d'une page : une rangée ouverte par hauteur
/// ([`SHELF_HEIGHTS`]), remplie de gauche à droite ; pleine, une autre
/// s'ouvre sous toutes les autres.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Shelves {
    /// La rangée ouverte de chaque hauteur : sa ligne, et sa première
    /// colonne libre.
    open: [Option<(u32, u32)>; SHELF_HEIGHTS.len()],
    /// Première ligne libre sous les rangées.
    next: u32,
}

impl Shelves {
    /// Une place pour une image de `width` × `height` pixels : son coin
    /// haut gauche, ou `None` si la page n'en a plus.
    fn place(&mut self, width: u32, height: u32) -> Option<(u32, u32)> {
        let class = SHELF_HEIGHTS.iter().position(|&h| h >= height)?;
        if width == 0 || width > PAGE {
            return None;
        }
        if let Some((y, x)) = self.open[class]
            && x + width <= PAGE
        {
            self.open[class] = Some((y, x + width));
            return Some((x, y));
        }
        let y = self.next;
        if y + SHELF_HEIGHTS[class] > PAGE {
            return None;
        }
        self.next = y + SHELF_HEIGHTS[class];
        self.open[class] = Some((y, width));
        Some((0, y))
    }
}

/// Une page d'images sur le GPU.
#[derive(Debug)]
struct Page {
    id: u32,
    format: wgpu::TextureFormat,
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    shelves: Shelves,
    bytes: usize,
    last_used: u64,
}

/// La place d'une image dans une page : le numéro de la page, et son
/// rectangle (de 0 à 1 sur chaque côté de la page).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Placed {
    page: u32,
    rect: [f32; 4],
}

/// Le moteur de rendu direct : programmes, paramètres et images.
#[derive(Debug)]
pub struct Quick {
    module: wgpu::ShaderModule,
    image_layout: wgpu::BindGroupLayout,
    /// Les pages d'un appel de morceaux, et le programme qui les lit.
    pages_layout: wgpu::BindGroupLayout,
    pieces_layout: wgpu::PipelineLayout,
    shapes_layout: wgpu::PipelineLayout,
    images_layout: wgpu::PipelineLayout,
    params_layout: wgpu::BindGroupLayout,
    nearest: wgpu::Sampler,
    bilinear: wgpu::Sampler,
    /// Tampon de paramètres (sans « immediates » seulement).
    params: Option<wgpu::Buffer>,
    /// Échantillonneurs, et le tampon de paramètres s'il y en a un.
    params_group: wgpu::BindGroup,
    /// Éléments que le tampon de paramètres peut recevoir.
    capacity: usize,
    /// Les paramètres partent avec chaque commande de dessin.
    immediates: bool,
    pipelines: HashMap<wgpu::TextureFormat, Pipelines>,
    textures: HashMap<u64, Texture>,
    texture_bytes: usize,
    /// Les pages d'images, et la place de chaque image rangée.
    pages: Vec<Page>,
    placed: HashMap<u64, Placed>,
    next_page: u32,
    /// Les groupes de pages déjà liés ensemble pour un appel.
    page_groups: HashMap<[u32; CALL_PAGES], wgpu::BindGroup>,
    /// Les petites images vont en pages (toujours, sauf pour comparer).
    paged: bool,
    /// Octets de pages gardés ([`PAGES_BUDGET`]).
    pages_budget: usize,
    /// Les morceaux de l'image en cours, toutes fenêtres confondues, et le
    /// tampon où ils partent ([`PIECE_BYTES`] octets chacun).
    pieces: Vec<u8>,
    pieces_buffer: Option<wgpu::Buffer>,
    pieces_capacity: usize,
    /// Premier élément de la fenêtre en cours (un appel ne réunit pas les
    /// morceaux de deux fenêtres).
    window_first: usize,
    /// Le GPU lit les textures BC7.
    bc7: bool,
    /// Textures des calques à part, à leur mesure (arrondie), réservées.
    spares: Vec<Spare>,
    /// Les calques à part de l'image en cours, toutes fenêtres confondues.
    layers: Vec<LayerJob>,
    /// Numéro de l'image en cours (pour oublier les images les plus anciennes).
    tick: u64,
    /// Paramètres et éléments de l'image en cours, toutes fenêtres
    /// confondues (gardés pour ne pas réallouer à chaque image).
    bytes: Vec<u8>,
    calls: Vec<Call>,
}

impl Quick {
    #[must_use]
    pub fn new(device: &wgpu::Device) -> Self {
        let immediates = device.features().contains(wgpu::Features::IMMEDIATES)
            && device.limits().max_immediate_size >= DRAW_BYTES;
        Self::with(device, immediates)
    }

    /// Le même moteur, paramètres passés par un tampon uniforme (comme sur
    /// un GPU sans « immediates »).
    #[cfg(test)]
    pub(crate) fn with_uniforms(device: &wgpu::Device) -> Self {
        Self::with(device, false)
    }

    fn with(device: &wgpu::Device, immediates: bool) -> Self {
        debug_assert!(SHADER.contains(UNIFORM_DRAW));
        let source = if immediates {
            SHADER.replace(UNIFORM_DRAW, IMMEDIATE_DRAW)
        } else {
            SHADER.to_owned()
        };
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("mouflon-rapide"),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
        let sampler_entry = |binding: u32| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        };
        let buffer_entry = wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: true,
                min_binding_size: NonZeroU64::new(SLOT as u64),
            },
            count: None,
        };
        let entries = [buffer_entry, sampler_entry(1), sampler_entry(2)];
        let params_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("mouflon-rapide-parametres"),
            entries: if immediates { &entries[1..] } else { &entries },
        });
        let immediate_size = if immediates { DRAW_BYTES } else { 0 };
        let image_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("mouflon-rapide-image"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });
        let page_entry = |binding: u32| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let pages_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("mouflon-rapide-pages"),
            entries: &[page_entry(0), page_entry(1), page_entry(2), page_entry(3)],
        });
        let pieces_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("mouflon-rapide-morceaux"),
            bind_group_layouts: &[Some(&params_layout), Some(&pages_layout)],
            immediate_size,
        });
        let shapes_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("mouflon-rapide-formes"),
            bind_group_layouts: &[Some(&params_layout)],
            immediate_size,
        });
        let images_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("mouflon-rapide-images"),
            bind_group_layouts: &[Some(&params_layout), Some(&image_layout)],
            immediate_size,
        });
        let sampler = |filter: wgpu::FilterMode| {
            device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("mouflon-rapide"),
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                mag_filter: filter,
                min_filter: filter,
                ..Default::default()
            })
        };
        let (nearest, bilinear) = (
            sampler(wgpu::FilterMode::Nearest),
            sampler(wgpu::FilterMode::Linear),
        );
        let (params, params_group, capacity) = if immediates {
            (
                None,
                samplers_group(device, &params_layout, &nearest, &bilinear),
                usize::MAX,
            )
        } else {
            let (buffer, group) =
                params_buffer(device, &params_layout, &nearest, &bilinear, FIRST_CAPACITY);
            (Some(buffer), group, FIRST_CAPACITY)
        };
        debug_assert!(SHADER.contains(&format!("PAGE_SIDE: f32 = {PAGE}.0;")));
        Self {
            module,
            image_layout,
            pages_layout,
            pieces_layout,
            shapes_layout,
            images_layout,
            params_layout,
            nearest,
            bilinear,
            params,
            params_group,
            capacity,
            immediates,
            pipelines: HashMap::new(),
            spares: Vec::new(),
            layers: Vec::new(),
            textures: HashMap::new(),
            texture_bytes: 0,
            pages: Vec::new(),
            placed: HashMap::new(),
            next_page: 0,
            page_groups: HashMap::new(),
            paged: device.limits().max_texture_dimension_2d >= PAGE,
            pages_budget: PAGES_BUDGET,
            pieces: Vec::new(),
            pieces_buffer: None,
            pieces_capacity: 0,
            window_first: 0,
            bc7: device
                .features()
                .contains(wgpu::Features::TEXTURE_COMPRESSION_BC),
            tick: 0,
            bytes: Vec::with_capacity(SLOT * 8),
            calls: Vec::with_capacity(8),
        }
    }

    /// Les programmes pour des fenêtres au format `format` (compilés à la
    /// première demande).
    pub fn prepare(&mut self, device: &wgpu::Device, format: wgpu::TextureFormat) {
        if self.pipelines.contains_key(&format) {
            return;
        }
        let pipeline = |layout: &wgpu::PipelineLayout, entry: &str| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(entry),
                layout: Some(layout),
                vertex: wgpu::VertexState {
                    module: &self.module,
                    entry_point: Some("vs"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &[],
                },
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleStrip,
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &self.module,
                    entry_point: Some(entry),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    // Couleurs prémultipliées, comme les attend Core Animation.
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        // Les morceaux en pages : une ligne du tampon par morceau.
        let pieces = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("fs_pieces"),
            layout: Some(&self.pieces_layout),
            vertex: wgpu::VertexState {
                module: &self.module,
                entry_point: Some("vs_pieces"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: PIECE_BYTES as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &wgpu::vertex_attr_array![
                        0 => Float32x4,
                        1 => Float32x4,
                        2 => Float32x4,
                    ],
                })],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &self.module,
                entry_point: Some("fs_pieces"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let pipelines = Pipelines {
            image: pipeline(&self.images_layout, "fs_image"),
            shadow: pipeline(&self.shapes_layout, "fs_shadow"),
            disc: pipeline(&self.shapes_layout, "fs_disc"),
            pieces,
        };
        self.pipelines.insert(format, pipelines);
    }

    /// La liste se dessine-t-elle ici ? Des images, des ombres, des disques,
    /// et des calques d'opacité qui couvrent toute la fenêtre sans rien
    /// fondre ni découper, l'un après l'autre (pas l'un dans l'autre) : tout
    /// ce que donnent des morceaux d'image ([`mouflon_render::pieces`]).
    #[must_use]
    pub fn supports(list: &DisplayList) -> bool {
        let canvas = list.bounds();
        let mut depth = 0usize;
        for command in &list.commands {
            match command {
                Command::Image { .. } | Command::BlurredRect { .. } => {}
                Command::Disc { paint, .. } => {
                    if !paint_supported(paint) {
                        return false;
                    }
                }
                Command::PushLayer {
                    clip,
                    transform,
                    mix,
                    ..
                } => {
                    let covered = transform.transform_rect_bbox(clip.bounding_box());
                    // Sans calque dedans ni fusion, sur toute la fenêtre.
                    if *mix != Mix::Normal || !contains(covered, canvas) || depth > 0 {
                        return false;
                    }
                    depth += 1;
                }
                Command::PopLayer => depth = depth.saturating_sub(1),
                Command::Fill { .. } | Command::Stroke { .. } => return false,
            }
        }
        true
    }

    /// Éléments que la liste fera dessiner (la pose d'un calque compte pour
    /// un de plus).
    #[must_use]
    pub fn draws(list: &DisplayList) -> usize {
        list.commands
            .iter()
            .filter(|c| {
                matches!(
                    c,
                    Command::Image { .. }
                        | Command::BlurredRect { .. }
                        | Command::Disc { .. }
                        | Command::PushLayer { .. }
                )
            })
            .count()
    }

    /// Commence une image de toutes les fenêtres, qui dessineront ensemble au
    /// plus `draws` éléments : leurs passes partiront au GPU en un seul envoi.
    pub fn begin(&mut self, device: &wgpu::Device, draws: usize) {
        self.tick += 1;
        self.bytes.clear();
        self.calls.clear();
        self.layers.clear();
        self.pieces.clear();
        // Au plus un morceau par élément.
        if self.paged && (draws > self.pieces_capacity || self.pieces_buffer.is_none()) {
            let capacity = draws.max(FIRST_PIECES).next_power_of_two();
            self.pieces_buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("mouflon-rapide-morceaux"),
                size: (PIECE_BYTES * capacity) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
            self.pieces_capacity = capacity;
        }
        if draws > self.capacity {
            let capacity = draws.next_power_of_two();
            let params;
            (params, self.params_group) = params_buffer(
                device,
                &self.params_layout,
                &self.nearest,
                &self.bilinear,
                capacity,
            );
            self.params = Some(params);
            self.capacity = capacity;
        }
    }

    /// Ajoute à `encoder` une passe qui dessine la liste (que
    /// [`Self::supports`] accepte) dans `target`, une texture au format
    /// `format` d'abord effacée, précédée d'une passe par calque à part. Les
    /// images manquantes sont téléversées au passage ; les paramètres
    /// partent avec [`Self::finish`].
    pub fn draw(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        format: wgpu::TextureFormat,
        list: &DisplayList,
    ) {
        self.prepare(device, format);
        let first = self.calls.len();
        self.window_first = first;
        let first_layer = self.layers.len();
        let size = (f64::from(list.width.max(1)), f64::from(list.height.max(1)));
        // Les calques, dans l'ordre : combien d'éléments, et où ils sont.
        let spans = layer_spans(list);
        let mut next_span = 0;
        let mut alpha = 1.0f32;
        // Le calque à part en cours : son rang, le passage des pixels de la
        // fenêtre à ceux de sa texture, la taille de sa texture.
        let mut apart: Option<(u32, Affine, (f64, f64))> = None;
        // Un calque tout entier hors de la fenêtre : rien ne s'en voit.
        let mut hidden = false;
        for command in &list.commands {
            // Jamais plus que le tampon n'en reçoit.
            if self.calls.len() >= self.capacity {
                break;
            }
            let (to_target, target_size) =
                apart.map_or((Affine::IDENTITY, size), |(_, to, s)| (to, s));
            match command {
                Command::Image { .. } | Command::BlurredRect { .. } | Command::Disc { .. }
                    if hidden => {}
                Command::Image {
                    bitmap,
                    transform,
                    alpha: own,
                    smooth,
                } => {
                    // L'image et sa bordure transparente d'un pixel (et ce qui
                    // complète une image compressée, transparent aussi).
                    let (tw, th) = bitmap.texture_size();
                    let mut p = Params::quad(
                        target_size,
                        to_target * *transform,
                        Rect::new(-1.0, -1.0, f64::from(tw) - 1.0, f64::from(th) - 1.0),
                    );
                    p.set(ALPHA, own * alpha);
                    p.set(FLAG, if *smooth { 1.0 } else { 0.0 });
                    match self.place(device, queue, bitmap) {
                        // Rangée dans une page : un morceau de plus dans
                        // l'appel en cours, s'il est de la même page.
                        Some(placed) => self.piece(placed, &p, apart.map(|(job, ..)| job)),
                        None => {
                            self.upload(device, queue, bitmap);
                            self.push(Kind::Image, Some(bitmap.id()), &p);
                        }
                    }
                }
                Command::BlurredRect {
                    rect,
                    radius,
                    std_dev,
                    color,
                    transform,
                } => {
                    let transform = to_target * *transform;
                    // Comme Vello : la gaussienne est coupée à 2,5 écarts types.
                    let kernel = 2.5 * std_dev;
                    let mut p = Params::quad(target_size, transform, rect.inflate(kernel, kernel));
                    p.inverse(transform * Affine::translate(rect.center().to_vec2()));
                    p.set(ALPHA, alpha);
                    p.vec4(4, premultiplied(*color, 1.0));
                    p.vec4(
                        5,
                        [
                            rect.width() as f32,
                            rect.height() as f32,
                            *radius as f32,
                            *std_dev as f32,
                        ],
                    );
                    self.push(Kind::Shadow, None, &p);
                }
                Command::Disc {
                    center,
                    radius,
                    paint,
                    transform,
                } => {
                    let transform = to_target * *transform;
                    let scale = transform.determinant().abs().sqrt().max(1e-9);
                    // Un pixel et demi de plus autour, pour le bord antialiasé.
                    let margin = 1.5 / scale;
                    let r = radius + margin;
                    let bounds = Rect::new(center.x - r, center.y - r, center.x + r, center.y + r);
                    let mut p = Params::quad(target_size, transform, bounds);
                    p.inverse(transform);
                    p.set(ALPHA, alpha);
                    p.vec4(
                        5,
                        [
                            center.x as f32,
                            center.y as f32,
                            *radius as f32,
                            scale as f32,
                        ],
                    );
                    p.paint(paint);
                    self.push(Kind::Disc, None, &p);
                }
                Command::PushLayer { alpha: layer, .. } => {
                    let layer = layer.clamp(0.0, 1.0);
                    match spans.get(next_span) {
                        // Plusieurs éléments : à part, dans une texture à leur mesure.
                        Some(&(count, bounds)) if count > 1 => {
                            match pixel_rect(bounds, list.width.max(1), list.height.max(1)) {
                                Some((x, y, w, h)) => {
                                    let spare = self.spare(device, format, w, h);
                                    let (tw, th) = self.spares[spare].size;
                                    let job = self.layers.len() as u32;
                                    self.layers.push(LayerJob {
                                        spare,
                                        alpha: layer,
                                    });
                                    apart = Some((
                                        job,
                                        Affine::translate((-f64::from(x), -f64::from(y))),
                                        (f64::from(tw), f64::from(th)),
                                    ));
                                }
                                None => hidden = true,
                            }
                        }
                        _ => alpha = layer,
                    }
                    next_span += 1;
                    continue;
                }
                Command::PopLayer => {
                    alpha = 1.0;
                    hidden = false;
                    // Le calque dessiné à part, posé à sa place avec son opacité.
                    if let Some((job, to_layer, (tw, th))) = apart.take() {
                        let mut p =
                            Params::quad(size, to_layer.inverse(), Rect::new(0.0, 0.0, tw, th));
                        p.set(ALPHA, self.layers[job as usize].alpha);
                        p.set(FLAG, 0.0);
                        self.push(Kind::Image, None, &p);
                        if let Some(call) = self.calls.last_mut() {
                            call.composite = Some(job);
                        }
                    }
                    continue;
                }
                Command::Fill { .. } | Command::Stroke { .. } => continue,
            }
            // Un élément d'un calque à part.
            if let Some((job, ..)) = apart
                && let Some(call) = self.calls.last_mut()
            {
                call.layer = Some(job);
            }
        }
        // Les pages de chaque appel de morceaux, liées ensemble.
        for at in first..self.calls.len() {
            let call = self.calls[at];
            if call.kind == Kind::Pieces {
                self.link_pages(device, call.pages);
            }
        }
        // Chaque calque à part dans sa texture, puis la fenêtre.
        for job in first_layer..self.layers.len() {
            let spare = &self.spares[self.layers[job].spare];
            self.pass(encoder, &spare.view, format, first, Some(job as u32));
        }
        self.pass(encoder, target, format, first, None);
    }

    /// Une passe : les éléments `first..` du calque à part `layer`, ou ceux
    /// de la fenêtre (avec la pose des calques à part), dans `target`
    /// d'abord effacée.
    fn pass(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        format: wgpu::TextureFormat,
        first: usize,
        layer: Option<u32>,
    ) {
        let Some(pipelines) = self.pipelines.get(&format) else {
            return;
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(if layer.is_some() {
                "mouflon-rapide-calque"
            } else {
                "mouflon-rapide"
            }),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        let mut current: Option<Kind> = None;
        if self.immediates {
            pass.set_bind_group(0, &self.params_group, &[]);
        }
        if let Some(buffer) = &self.pieces_buffer {
            pass.set_vertex_buffer(0, buffer.slice(..));
        }
        for (i, call) in self.calls.iter().enumerate().skip(first) {
            if call.layer != layer {
                continue;
            }
            if current != Some(call.kind) {
                pass.set_pipeline(match call.kind {
                    Kind::Image => &pipelines.image,
                    Kind::Shadow => &pipelines.shadow,
                    Kind::Disc => &pipelines.disc,
                    Kind::Pieces => &pipelines.pieces,
                });
                current = Some(call.kind);
            }
            if self.immediates {
                let at = i * SLOT;
                pass.set_immediates(0, &self.bytes[at..at + DRAW_BYTES as usize]);
            } else {
                pass.set_bind_group(0, &self.params_group, &[(i * SLOT) as u32]);
            }
            if let Some(job) = call.composite {
                let Some(spare) = self
                    .layers
                    .get(job as usize)
                    .and_then(|job| self.spares.get(job.spare))
                else {
                    continue;
                };
                pass.set_bind_group(1, &spare.bind_group, &[]);
            } else if call.kind == Kind::Pieces {
                // Tous les morceaux qui se suivent, d'un coup.
                let (Some(group), true) = (
                    self.page_groups.get(&call.pages),
                    self.pieces_buffer.is_some(),
                ) else {
                    continue;
                };
                pass.set_bind_group(1, group, &[]);
                let (first_piece, count) = call.pieces;
                pass.draw(0..4, first_piece..first_piece + count);
                continue;
            } else if let Some(texture) = call.image.and_then(|key| self.textures.get(&key)) {
                pass.set_bind_group(1, &texture.bind_group, &[]);
            } else if call.kind == Kind::Image {
                continue;
            }
            pass.draw(0..4, 0..1);
        }
    }

    /// Une texture libre pour un calque à part d'au moins `width` × `height`
    /// pixels, au format `format` : la plus petite des réserves qui n'a pas
    /// encore servi à cette image, sinon une nouvelle (arrondie à 256
    /// pixels, pour resservir à des calques voisins). Son rang.
    fn spare(
        &mut self,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        width: u32,
        height: u32,
    ) -> usize {
        let tick = self.tick;
        let found = self
            .spares
            .iter()
            .enumerate()
            .filter(|(_, s)| {
                s.format == format && s.used != tick && s.size.0 >= width && s.size.1 >= height
            })
            .min_by_key(|(_, s)| u64::from(s.size.0) * u64::from(s.size.1))
            .map(|(i, _)| i);
        let index = found.unwrap_or_else(|| {
            let most = device.limits().max_texture_dimension_2d;
            let round = |n: u32| n.div_ceil(256).saturating_mul(256).min(most).max(n);
            let size = (round(width), round(height));
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("mouflon-calque"),
                size: wgpu::Extent3d {
                    width: size.0,
                    height: size.1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("mouflon-calque"),
                layout: &self.image_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                }],
            });
            self.spares.push(Spare {
                format,
                size,
                view,
                bind_group,
                _texture: texture,
                used: tick,
            });
            self.spares.len() - 1
        });
        self.spares[index].used = tick;
        index
    }

    /// Envoie les paramètres de toutes les passes de l'image (avant
    /// `queue.submit`), puis oublie au besoin les images les plus anciennes.
    pub fn finish(&mut self, queue: &wgpu::Queue) {
        if let Some(params) = &self.params
            && !self.bytes.is_empty()
        {
            queue.write_buffer(params, 0, &self.bytes);
        }
        if let Some(buffer) = &self.pieces_buffer
            && !self.pieces.is_empty()
        {
            queue.write_buffer(buffer, 0, &self.pieces);
        }
        self.forget_old();
        self.forget_old_pages();
        // Les textures de calques qui ne servent plus depuis un moment. (Les
        // rangs de l'image en cours ne servent plus : `layers` est refait à
        // la suivante.)
        let tick = self.tick;
        self.spares
            .retain(|spare| tick.saturating_sub(spare.used) < SPARE_IDLE);
    }

    /// Dessine une liste seule : une image d'une seule fenêtre.
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        format: wgpu::TextureFormat,
        list: &DisplayList,
    ) {
        self.begin(device, Self::draws(list));
        self.draw(device, queue, encoder, target, format, list);
        self.finish(queue);
    }

    fn push(&mut self, kind: Kind, image: Option<u64>, params: &Params) {
        for value in params.0 {
            self.bytes.extend_from_slice(&value.to_le_bytes());
        }
        self.bytes.resize(self.calls.len() * SLOT + SLOT, 0);
        self.calls.push(Call {
            kind,
            image,
            layer: None,
            composite: None,
            pieces: (0, 0),
            pages: [NO_PAGE; CALL_PAGES],
        });
    }

    /// Un morceau rangé dans une page, posé par `params` (son quadrilatère,
    /// son opacité, son lissage), dans le calque à part `layer` ou dans la
    /// fenêtre : il rejoint l'appel en cours s'il est du même calque et que
    /// sa page y est (ou peut encore y entrer), sinon il en ouvre un.
    fn piece(&mut self, placed: Placed, params: &Params, layer: Option<u32>) {
        let index = (self.pieces.len() / PIECE_BYTES) as u32;
        if index as usize >= self.pieces_capacity {
            return;
        }
        // Le rang de sa page dans l'appel en cours : celui qu'elle y a
        // déjà, ou le premier libre.
        let open = (self.calls.len() > self.window_first)
            .then(|| self.calls.last_mut())
            .flatten()
            .filter(|call| call.kind == Kind::Pieces && call.layer == layer);
        let slot = open.and_then(|call| {
            let slot = call
                .pages
                .iter()
                .position(|&page| page == placed.page || page == NO_PAGE)?;
            call.pages[slot] = placed.page;
            call.pieces.1 += 1;
            Some(slot)
        });
        let slot = slot.unwrap_or_else(|| {
            // Un appel de plus : ses paramètres ne disent que la taille de
            // ce où il dessine (`quad1.zw`).
            self.push(Kind::Pieces, None, params);
            if let Some(call) = self.calls.last_mut() {
                call.pieces = (index, 1);
                call.layer = layer;
                call.pages[0] = placed.page;
            }
            0
        });
        let p = &params.0;
        // L'origine et les deux côtés ; l'opacité ; le lissage et le rang de
        // la page ; sa place dans la page.
        let flag = p[FLAG] + 2.0 * slot as f32;
        let line = [p[0], p[1], p[2], p[3], p[4], p[5], p[ALPHA], flag];
        for value in line.into_iter().chain(placed.rect) {
            self.pieces.extend_from_slice(&value.to_le_bytes());
        }
    }

    /// Lie ensemble les pages d'un appel de morceaux (une fois : le groupe
    /// ressert tant que ces pages vivent). Les rangs vides lisent la
    /// première.
    fn link_pages(&mut self, device: &wgpu::Device, pages: [u32; CALL_PAGES]) {
        if self.page_groups.contains_key(&pages) {
            return;
        }
        let view = |id: u32| {
            self.pages
                .iter()
                .find(|page| page.id == id)
                .or_else(|| self.pages.iter().find(|page| page.id == pages[0]))
                .map(|page| &page.view)
        };
        let (Some(a), Some(b), Some(c), Some(d)) = (
            view(pages[0]),
            view(pages[1]),
            view(pages[2]),
            view(pages[3]),
        ) else {
            return;
        };
        let views = [a, b, c, d];
        let entries: [wgpu::BindGroupEntry<'_>; CALL_PAGES] =
            std::array::from_fn(|binding| wgpu::BindGroupEntry {
                binding: binding as u32,
                resource: wgpu::BindingResource::TextureView(views[binding]),
            });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("mouflon-pages"),
            layout: &self.pages_layout,
            entries: &entries,
        });
        self.page_groups.insert(pages, group);
    }

    /// La place de l'image dans une page, où elle est rangée au besoin
    /// (téléversée telle qu'elle le serait seule : voir [`Self::upload`]).
    /// `None` : elle est trop grande, ou a déjà sa texture à elle.
    fn place(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        bitmap: &Bitmap,
    ) -> Option<Placed> {
        if !self.paged {
            return None;
        }
        let (id, tick) = (bitmap.id(), self.tick);
        if let Some(&placed) = self.placed.get(&id) {
            if let Some(page) = self.pages.iter_mut().find(|p| p.id == placed.page) {
                page.last_used = tick;
                return Some(placed);
            }
            self.placed.remove(&id);
        }
        let (tw, th) = bitmap.texture_size();
        if tw == 0
            || th == 0
            || tw > PAGE_PIECE
            || th > PAGE_PIECE
            || self.textures.contains_key(&id)
        {
            return None;
        }
        let rgba;
        let (format, data, bytes_per_row, rows) = match &bitmap.pixels {
            Pixels::Bc7(blocks) if self.bc7 => (
                wgpu::TextureFormat::Bc7RgbaUnorm,
                blocks.data(),
                tw / 4 * 16,
                th / 4,
            ),
            _ => {
                rgba = bitmap.texture_rgba();
                (wgpu::TextureFormat::Rgba8Unorm, rgba.as_slice(), tw * 4, th)
            }
        };
        if data.len() < (bytes_per_row * rows) as usize {
            return None;
        }
        // La première page de ce format qui a de la place, sinon une neuve.
        let mut spot = self.pages.iter_mut().find_map(|page| {
            (page.format == format)
                .then(|| page.shelves.place(tw, th))
                .flatten()
                .map(|at| (page.id, at))
        });
        if spot.is_none() {
            let mut page = self.new_page(device, format);
            spot = page.shelves.place(tw, th).map(|at| (page.id, at));
            self.pages.push(page);
        }
        let (page_id, (x, y)) = spot?;
        let page = self.pages.iter_mut().find(|p| p.id == page_id)?;
        page.last_used = tick;
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &page.texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x, y, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bytes_per_row),
                rows_per_image: Some(rows),
            },
            wgpu::Extent3d {
                width: tw,
                height: th,
                depth_or_array_layers: 1,
            },
        );
        let side = PAGE as f32;
        let placed = Placed {
            page: page_id,
            rect: [
                x as f32 / side,
                y as f32 / side,
                (x + tw) as f32 / side,
                (y + th) as f32 / side,
            ],
        };
        self.placed.insert(id, placed);
        Some(placed)
    }

    /// Une page vide (transparente) pour des images de ce format.
    fn new_page(&mut self, device: &wgpu::Device, format: wgpu::TextureFormat) -> Page {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("mouflon-page"),
            size: wgpu::Extent3d {
                width: PAGE,
                height: PAGE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let id = self.next_page;
        self.next_page += 1;
        // Un octet par pixel en BC7, quatre sinon.
        let per_pixel = if format == wgpu::TextureFormat::Bc7RgbaUnorm {
            1
        } else {
            4
        };
        Page {
            id,
            format,
            texture,
            view,
            shelves: Shelves::default(),
            bytes: (PAGE * PAGE) as usize * per_pixel,
            last_used: self.tick,
        }
    }

    /// Rend les pages `gone` et oublie la place des images qui y étaient.
    fn drop_pages(&mut self, gone: impl Fn(&Page) -> bool) {
        let before = self.pages.len();
        self.pages.retain(|page| !gone(page));
        if self.pages.len() != before {
            let pages = &self.pages;
            let alive = |id: u32| pages.iter().any(|page| page.id == id);
            self.placed.retain(|_, placed| alive(placed.page));
            // Un groupe tient ses pages en vie : ceux des pages rendues
            // partent avec elles.
            self.page_groups
                .retain(|ids, _| ids.iter().all(|&id| id == NO_PAGE || alive(id)));
        }
    }

    /// Au-delà de leur budget, rend les pages qui n'ont pas servi depuis le
    /// plus longtemps (jamais celles de l'image en cours).
    fn forget_old_pages(&mut self) {
        let bytes = |pages: &[Page]| pages.iter().map(|p| p.bytes).sum::<usize>();
        while bytes(&self.pages) > self.pages_budget {
            let tick = self.tick;
            let Some(oldest) = self
                .pages
                .iter()
                .filter(|p| p.last_used < tick)
                .min_by_key(|p| p.last_used)
                .map(|p| p.id)
            else {
                return;
            };
            self.drop_pages(|page| page.id == oldest);
        }
    }

    /// Téléverse une image si elle n'est pas déjà sur le GPU : prémultipliée
    /// et bordée d'un pixel transparent (voir [`Bitmap::texture_size`]), en
    /// blocs BC7 si elle est compressée et que le GPU les lit.
    fn upload(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, bitmap: &Bitmap) {
        let tick = self.tick;
        if let Some(texture) = self.textures.get_mut(&bitmap.id()) {
            texture.last_used = tick;
            return;
        }
        let (tw, th) = bitmap.texture_size();
        let rgba;
        let (format, data, bytes_per_row, rows) = match &bitmap.pixels {
            Pixels::Bc7(blocks) if self.bc7 => (
                wgpu::TextureFormat::Bc7RgbaUnorm,
                blocks.data(),
                tw / 4 * 16,
                th / 4,
            ),
            _ => {
                rgba = bitmap.texture_rgba();
                (wgpu::TextureFormat::Rgba8Unorm, rgba.as_slice(), tw * 4, th)
            }
        };
        if data.len() < (bytes_per_row * rows) as usize {
            return;
        }
        let size = wgpu::Extent3d {
            width: tw,
            height: th,
            depth_or_array_layers: 1,
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("mouflon-image"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bytes_per_row),
                rows_per_image: Some(rows),
            },
            size,
        );
        let bytes = (bytes_per_row * rows) as usize;
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("mouflon-image"),
            layout: &self.image_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            }],
        });
        self.texture_bytes += bytes;
        self.textures.insert(
            bitmap.id(),
            Texture {
                bind_group,
                _texture: texture,
                bytes,
                last_used: tick,
            },
        );
    }

    /// Le même moteur, qui décompresse lui-même les images BC7 (comme sur un
    /// GPU qui ne les lit pas).
    #[cfg(test)]
    pub(crate) fn without_bc7(mut self) -> Self {
        self.bc7 = false;
        self
    }

    /// Le même moteur, chaque image dans sa texture et son appel à elle
    /// (comme avant les pages), pour comparer.
    #[cfg(test)]
    pub(crate) fn without_pages(mut self) -> Self {
        self.paged = false;
        self
    }

    /// Appels de dessin de la dernière image (toutes fenêtres confondues).
    #[cfg(test)]
    pub(crate) fn calls(&self) -> usize {
        self.calls.len()
    }

    /// Le même moteur, qui ne garde que ce nombre de pages.
    #[cfg(test)]
    pub(crate) fn with_pages_budget(mut self, pages: usize) -> Self {
        self.pages_budget = pages * (PAGE * PAGE * 4) as usize;
        self
    }

    /// Pages sur le GPU.
    #[cfg(test)]
    pub(crate) fn pages(&self) -> usize {
        self.pages.len()
    }

    /// Oublie les images qui n'ont pas servi à la dernière image : celles
    /// qu'une version compressée remplace ne reserviront plus. (Une page
    /// part quand plus aucune de ses images ne sert.)
    pub fn forget_unused(&mut self) {
        let tick = self.tick;
        self.textures.retain(|_, t| t.last_used >= tick);
        self.texture_bytes = self.textures.values().map(|t| t.bytes).sum();
        self.drop_pages(|page| page.last_used < tick);
    }

    /// Au-delà du budget, oublie les images les moins récemment montrées
    /// (jamais celles de l'image en cours), jusqu'aux trois quarts du budget.
    fn forget_old(&mut self) {
        if self.texture_bytes <= TEXTURE_BUDGET {
            return;
        }
        let mut ages: Vec<(u64, u64)> = self
            .textures
            .iter()
            .filter(|(_, t)| t.last_used < self.tick)
            .map(|(key, t)| (t.last_used, *key))
            .collect();
        ages.sort_unstable();
        for (_, key) in ages {
            if self.texture_bytes <= TEXTURE_BUDGET / 4 * 3 {
                break;
            }
            if let Some(texture) = self.textures.remove(&key) {
                self.texture_bytes -= texture.bytes;
            }
        }
    }

    /// Images sur le GPU et octets qu'elles occupent.
    #[must_use]
    pub fn texture_usage(&self) -> (usize, usize) {
        (
            self.textures.len() + self.placed.len(),
            self.texture_bytes + self.pages.iter().map(|p| p.bytes).sum::<usize>(),
        )
    }
}

/// Groupe de liaison des seuls échantillonneurs (paramètres en « immediates »).
fn samplers_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    nearest: &wgpu::Sampler,
    bilinear: &wgpu::Sampler,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("mouflon-rapide-echantillonneurs"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(nearest),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(bilinear),
            },
        ],
    })
}

/// Tampon de paramètres pour `capacity` éléments, et son groupe de liaison
/// (une place de [`SLOT`] octets par élément, choisie par décalage dynamique).
fn params_buffer(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    nearest: &wgpu::Sampler,
    bilinear: &wgpu::Sampler,
    capacity: usize,
) -> (wgpu::Buffer, wgpu::BindGroup) {
    let params = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("mouflon-rapide-parametres"),
        size: (SLOT * capacity) as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("mouflon-rapide-parametres"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &params,
                    offset: 0,
                    size: NonZeroU64::new(SLOT as u64),
                }),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(nearest),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(bilinear),
            },
        ],
    });
    (params, group)
}

/// Les calques d'opacité de la liste, dans l'ordre : combien d'éléments
/// chacun contient, et le rectangle (pixels de la fenêtre) qui contient ce
/// qu'ils dessinent.
fn layer_spans(list: &DisplayList) -> Vec<(usize, Rect)> {
    let mut spans: Vec<(usize, Rect)> = Vec::new();
    let mut inside = false;
    for command in &list.commands {
        let drawn = match command {
            Command::PushLayer { .. } => {
                spans.push((0, Rect::ZERO));
                inside = true;
                continue;
            }
            Command::PopLayer => {
                inside = false;
                continue;
            }
            _ if !inside => continue,
            Command::Image {
                bitmap, transform, ..
            } => {
                let (tw, th) = bitmap.texture_size();
                transform.transform_rect_bbox(Rect::new(
                    -1.0,
                    -1.0,
                    f64::from(tw) - 1.0,
                    f64::from(th) - 1.0,
                ))
            }
            Command::BlurredRect {
                rect,
                std_dev,
                transform,
                ..
            } => {
                let kernel = 2.5 * std_dev;
                transform.transform_rect_bbox(rect.inflate(kernel, kernel))
            }
            Command::Disc {
                center,
                radius,
                transform,
                ..
            } => {
                let scale = transform.determinant().abs().sqrt().max(1e-9);
                let r = radius + 1.5 / scale;
                transform.transform_rect_bbox(Rect::new(
                    center.x - r,
                    center.y - r,
                    center.x + r,
                    center.y + r,
                ))
            }
            Command::Fill { .. } | Command::Stroke { .. } => continue,
        };
        if let Some((count, bounds)) = spans.last_mut() {
            *bounds = if *count == 0 {
                drawn
            } else {
                bounds.union(drawn)
            };
            *count += 1;
        }
    }
    spans
}

/// Le rectangle `bounds` en pixels entiers, dans une fenêtre de `width` ×
/// `height` : coin, largeur, hauteur ; `None` s'il n'en reste rien.
fn pixel_rect(bounds: Rect, width: u32, height: u32) -> Option<(u32, u32, u32, u32)> {
    let x0 = bounds.x0.floor().max(0.0);
    let y0 = bounds.y0.floor().max(0.0);
    let x1 = bounds.x1.ceil().min(f64::from(width));
    let y1 = bounds.y1.ceil().min(f64::from(height));
    (x1 > x0 && y1 > y0).then_some((x0 as u32, y0 as u32, (x1 - x0) as u32, (y1 - y0) as u32))
}

/// `inner` est-il entièrement dans `outer` (au demi-pixel près) ?
fn contains(outer: Rect, inner: Rect) -> bool {
    outer.x0 <= inner.x0 + 0.5
        && outer.y0 <= inner.y0 + 0.5
        && outer.x1 >= inner.x1 - 0.5
        && outer.y1 >= inner.y1 - 0.5
}

fn paint_supported(paint: &Paint) -> bool {
    match paint {
        Paint::Solid(_) => true,
        Paint::Linear { stops, .. } | Paint::Radial { stops, .. } => {
            !stops.is_empty() && stops.len() <= MAX_STOPS
        }
    }
}

/// Couleur prémultipliée, multipliée par `alpha`.
fn premultiplied(color: Color, alpha: f32) -> [f32; 4] {
    let [r, g, b, a] = color.components;
    let a = a.clamp(0.0, 1.0) * alpha;
    [r * a, g * a, b * a, a]
}

/// Paramètres d'un élément, tels que les lit le programme (voir `Draw` :
/// treize `vec4`, dans cet ordre).
#[derive(Clone, Copy, Debug)]
struct Params([f32; 52]);

/// Places de quelques paramètres isolés dans [`Params`].
const ALPHA: usize = 14;
const FLAG: usize = 15;
const STOP_COUNT: usize = 16;
const OFFSETS: usize = 32;

impl Params {
    /// Quadrilatère couvrant `rect` (dans le repère de l'élément), transformé
    /// par `transform` vers les pixels d'une fenêtre de taille `size`.
    fn quad(size: (f64, f64), transform: Affine, rect: Rect) -> Self {
        let mut p = Self([0.0; 52]);
        let origin = transform * Point::new(rect.x0, rect.y0);
        let [a, b, c, d, _, _] = transform.as_coeffs();
        let u = Vec2::new(a * rect.width(), b * rect.width());
        let v = Vec2::new(c * rect.height(), d * rect.height());
        p.vec4(
            0,
            [origin.x as f32, origin.y as f32, u.x as f32, u.y as f32],
        );
        p.vec4(1, [v.x as f32, v.y as f32, size.0 as f32, size.1 as f32]);
        p
    }

    /// Passage des pixels de la fenêtre au repère de l'élément : l'inverse de `to_pixels`.
    fn inverse(&mut self, to_pixels: Affine) {
        let [a, b, c, d, e, f] = to_pixels.inverse().as_coeffs();
        self.vec4(2, [a as f32, b as f32, c as f32, d as f32]);
        self.set(12, e as f32);
        self.set(13, f as f32);
    }

    /// Remplissage d'un disque : couleur unie (drapeau 0), dégradé linéaire
    /// (1) ou radial (2).
    fn paint(&mut self, paint: &Paint) {
        match paint {
            Paint::Solid(color) => {
                self.set(FLAG, 0.0);
                self.set(STOP_COUNT, 1.0);
                self.vec4(9, premultiplied(*color, 1.0));
            }
            Paint::Linear {
                start,
                end,
                stops,
                extend,
            } => {
                self.set(FLAG, 1.0);
                self.vec4(
                    6,
                    [start.x as f32, start.y as f32, 0.0, extend_code(*extend)],
                );
                self.vec4(7, [end.x as f32, end.y as f32, 0.0, 0.0]);
                self.stops(stops);
            }
            Paint::Radial {
                start,
                start_radius,
                end,
                end_radius,
                stops,
                extend,
            } => {
                self.set(FLAG, 2.0);
                self.vec4(
                    6,
                    [
                        start.x as f32,
                        start.y as f32,
                        *start_radius,
                        extend_code(*extend),
                    ],
                );
                self.vec4(7, [end.x as f32, end.y as f32, *end_radius, 0.0]);
                self.stops(stops);
            }
        }
    }

    /// Arrêts de couleur (au plus [`MAX_STOPS`]) : positions et couleurs prémultipliées.
    fn stops(&mut self, stops: &[Stop]) {
        let n = stops.len().min(MAX_STOPS);
        self.set(STOP_COUNT, n as f32);
        for (k, stop) in stops.iter().take(MAX_STOPS).enumerate() {
            self.set(OFFSETS + k, stop.offset);
            self.vec4(9 + k, premultiplied(stop.color, 1.0));
        }
    }

    fn vec4(&mut self, index: usize, values: [f32; 4]) {
        self.0[index * 4..index * 4 + 4].copy_from_slice(&values);
    }

    fn set(&mut self, index: usize, value: f32) {
        self.0[index] = value;
    }
}

fn extend_code(extend: Extend) -> f32 {
    match extend {
        Extend::Pad => 0.0,
        Extend::Repeat => 1.0,
        Extend::Reflect => 2.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Les places données dans une page ne se chevauchent jamais, restent
    /// dans la page, et la page se remplit bien avant de refuser.
    #[test]
    fn a_page_packs_pieces_of_all_sizes_without_overlap() {
        let mut shelves = Shelves::default();
        let mut rng = 0x2545_f491u32;
        let mut next = || {
            rng ^= rng << 13;
            rng ^= rng >> 17;
            rng ^= rng << 5;
            rng
        };
        let mut placed: Vec<(u32, u32, u32, u32)> = Vec::new();
        let mut area = 0u64;
        loop {
            // Des morceaux de mouton : de 6 à 140 pixels de côté.
            let (w, h) = (6 + next() % 135, 6 + next() % 135);
            let Some((x, y)) = shelves.place(w, h) else {
                break;
            };
            assert!(x + w <= PAGE && y + h <= PAGE, "{x},{y} {w}×{h}");
            placed.push((x, y, w, h));
            area += u64::from(w) * u64::from(h);
        }
        // Rangée par rangée : deux places de la même rangée se suivent, deux
        // rangées ne se touchent pas.
        placed.sort_unstable_by_key(|&(x, y, ..)| (y, x));
        for pair in placed.windows(2) {
            let ((x0, y0, w0, h0), (x1, y1, ..)) = (pair[0], pair[1]);
            assert!(
                (y1 == y0 && x1 >= x0 + w0) || y1 >= y0 + h0,
                "{:?} et {:?} se chevauchent",
                pair[0],
                pair[1]
            );
        }
        // Au moins la moitié de la page sert (les rangées perdent au plus
        // un tiers de leur hauteur, et la dernière de chaque hauteur reste
        // ouverte).
        let fill = area as f64 / f64::from(PAGE * PAGE);
        assert!(fill > 0.5, "page remplie à {:.0} %", fill * 100.0);
    }

    #[test]
    fn a_piece_too_tall_for_any_shelf_is_refused() {
        let mut shelves = Shelves::default();
        assert_eq!(shelves.place(16, 513), None);
        assert_eq!(shelves.place(0, 16), None);
        assert_eq!(shelves.place(PAGE + 1, 16), None);
        // Les images compressées (côtés multiples de 4) tombent sur des
        // multiples de 4.
        for _ in 0..500 {
            let (x, y) = shelves.place(36, 20).expect("place");
            assert_eq!((x % 4, y % 4), (0, 0));
        }
    }
}
