//! Images d'un animal en sprites : planches découpées en cases, réduites
//! d'avance à la finesse de l'écran et rognées à leur contenu.

use std::io::Cursor;
use std::sync::atomic::{AtomicUsize, Ordering};

use mouflon_core::{Facings, SpritePage, SpriteSheet};

use crate::display::Bitmap;

#[derive(Debug, thiserror::Error)]
pub enum SpriteError {
    #[error("PNG illisible : {0}")]
    Png(#[from] png::DecodingError),
    #[error("planche introuvable : {0}")]
    Io(#[from] std::io::Error),
    #[error("planche de sprites invalide : {0}")]
    Invalid(String),
}

/// Pixels d'image par point visés (écrans Retina) : une planche plus fine est
/// réduite d'avance, avec un vrai filtre, plutôt qu'à chaque image par le GPU.
const PIXELS_PER_POINT: f32 = 2.0;

/// Une case, rognée à ce qu'elle contient.
#[derive(Clone, Debug)]
pub struct SpriteFrame {
    pub bitmap: Bitmap,
    /// Coin haut gauche de l'image dans sa case, en pixels de la planche.
    pub x: f32,
    pub y: f32,
    /// Pixels de la planche par pixel de l'image (plus de 1 : image réduite).
    pub texel: f32,
    /// Haut de son dos depuis le haut de sa case (pixels de la planche) : là
    /// où un autre animal se pose (voir [`Image::back_row`]). 0 : case vide.
    pub back: f32,
}

/// Case à dessiner pour un sens donné.
#[derive(Clone, Copy, Debug)]
pub struct Pick<'a> {
    pub frame: &'a SpriteFrame,
    /// Abscisse du point d'appui dans la case (pixels de la planche).
    pub anchor_x: f32,
    /// L'image est tournée vers la droite (dessinée ainsi, ou à retourner).
    pub faces_right: bool,
}

#[derive(Clone, Debug)]
pub struct SpriteFrames {
    /// Taille d'une case, en pixels de la planche.
    pub tile_width: u32,
    pub tile_height: u32,
    /// Points par pixel de la planche.
    pub scale: f32,
    pub frames: Vec<SpriteFrame>,
    pub facings: Facings,
    /// Point d'appui dans une case, image tournée vers la gauche (pixels).
    pub anchor: (f32, f32),
    /// Cases d'une variante de couleur, les deux sens compris.
    skin_block: u32,
    /// Cases de la planche principale : les images intermédiaires suivent.
    regular: u32,
}

impl SpriteFrames {
    pub fn decode(sheet: &SpriteSheet) -> Result<Self, SpriteError> {
        if sheet.pages.is_empty() {
            return Err(SpriteError::Invalid("aucune planche".into()));
        }
        // Les images intermédiaires suivent celles de la planche principale.
        let pages: Vec<&SpritePage> = sheet
            .pages
            .iter()
            .chain(sheet.inbetweens.iter().flat_map(|i| i.pages.iter()))
            .collect();
        let decoded = decode_pages(&pages, sheet);
        let mut tile: Option<(u32, u32)> = None;
        let mut frames =
            Vec::with_capacity(pages.iter().map(|p| p.tiles_x * p.tiles_y).sum::<u32>() as usize);
        for page in decoded {
            let page = page?;
            if tile.is_some_and(|t| t != page.tile) {
                return Err(SpriteError::Invalid(format!(
                    "cases de {} × {} pixels, différentes des autres planches",
                    page.tile.0, page.tile.1
                )));
            }
            tile = Some(page.tile);
            frames.extend(page.frames);
        }
        Ok(Self::assemble(sheet, tile.unwrap_or((1, 1)), frames))
    }

    /// Les images d'une planche aux cases de `tile` pixels, déjà découpées.
    pub(crate) fn assemble(
        sheet: &SpriteSheet,
        tile: (u32, u32),
        frames: Vec<SpriteFrame>,
    ) -> Self {
        let (tile_width, tile_height) = tile;
        let regular = sheet.tile_count();
        let [ax, ay] = sheet
            .anchor
            .unwrap_or([tile_width as f32 * 0.5, tile_height as f32]);
        Self {
            tile_width,
            tile_height,
            scale: sheet.scale,
            skin_block: (regular / sheet.skins.max(1)).max(1),
            regular,
            frames,
            facings: sheet.facings,
            anchor: (ax, ay),
        }
    }

    /// Octets qu'occupent les images.
    #[must_use]
    pub fn byte_size(&self) -> usize {
        self.frames.iter().map(|f| f.bitmap.byte_size()).sum()
    }

    /// Reste-t-il des images à compresser ? Toutes, sauf le pixel art
    /// agrandi : quand chaque pixel de la planche devient un carré net de
    /// plusieurs pixels de l'écran (voir [`Self::crisp`]), les petits écarts
    /// de la compression se verraient agrandis eux aussi.
    #[must_use]
    pub fn compressible(&self) -> bool {
        !self.crisp() && self.frames.iter().any(compressible)
    }

    /// Pixel art agrandi d'un nombre entier de fois (deux au moins) sur un
    /// écran Retina : ses images s'y posent pixel pour pixel, sans lissage.
    #[must_use]
    pub fn crisp(&self) -> bool {
        let k = self.scale * PIXELS_PER_POINT;
        k > 1.999 && (k - k.round()).abs() < 0.01
    }

    /// Les mêmes images, compressées en BC7 (voir [`crate::bc7`]) : quatre
    /// fois moins de mémoire, en mémoire comme sur le GPU. Le travail se
    /// partage entre `threads` fils, qui appellent d'abord `setup` (pour
    /// baisser leur priorité, par exemple).
    #[must_use]
    pub fn compressed(&self, threads: usize, setup: &(dyn Fn() + Sync)) -> Self {
        let next = AtomicUsize::new(0);
        let workers = threads.clamp(1, 16).min(self.frames.len().max(1));
        let done: Vec<Vec<(usize, Bitmap)>> = std::thread::scope(|scope| {
            let jobs: Vec<_> = (0..workers)
                .map(|_| {
                    scope.spawn(|| {
                        setup();
                        let mut done = Vec::new();
                        loop {
                            let i = next.fetch_add(1, Ordering::Relaxed);
                            let Some(frame) = self.frames.get(i) else {
                                break;
                            };
                            if !self.crisp() && compressible(frame) {
                                done.push((i, frame.bitmap.compressed()));
                            }
                        }
                        done
                    })
                })
                .collect();
            jobs.into_iter()
                .map(|job| job.join().unwrap_or_default())
                .collect()
        });
        let mut frames = self.frames.clone();
        for (i, bitmap) in done.into_iter().flatten() {
            frames[i].bitmap = bitmap;
        }
        Self {
            frames,
            ..self.clone()
        }
    }

    /// Hauteur de son dos au-dessus du point d'appui (points), image par
    /// image (par rang dans la planche) : là où un autre animal se pose.
    /// 0 pour une image vide.
    #[must_use]
    pub fn tops(&self) -> Vec<f32> {
        self.frames
            .iter()
            .map(|frame| {
                if frame.bitmap.width <= 1 || frame.back <= 0.0 {
                    0.0
                } else {
                    ((self.anchor.1 - frame.back) * self.scale).max(0.0)
                }
            })
            .collect()
    }

    /// Place du dessin autour de son point d'appui, devant et derrière
    /// (points) : la médiane sur les images de la planche principale. Les
    /// cases desktopPet ont souvent de larges marges vides : c'est le
    /// dessin qui compte quand deux animaux se touchent.
    #[must_use]
    pub fn reach(&self) -> Option<(f32, f32)> {
        let mut fronts = Vec::new();
        let mut backs = Vec::new();
        for index in 0..self.regular.min(self.frames.len() as u32) {
            let frame = &self.frames[index as usize];
            if frame.bitmap.width <= 1 {
                continue;
            }
            let faces_right = match self.facings {
                Facings::Left => false,
                Facings::Right => true,
                Facings::Both => index % self.skin_block >= self.skin_block / 2,
            };
            let anchor = if self.facings == Facings::Both && faces_right {
                self.tile_width as f32 - self.anchor.0
            } else {
                self.anchor.0
            };
            let left = frame.x;
            let right = frame.x + frame.bitmap.width as f32 * frame.texel;
            let (front, back) = if faces_right {
                (right - anchor, anchor - left)
            } else {
                (anchor - left, right - anchor)
            };
            fronts.push(front * self.scale);
            backs.push(back * self.scale);
        }
        let median = |values: &mut Vec<f32>| {
            values.sort_by(f32::total_cmp);
            values.get(values.len() / 2).copied()
        };
        let (front, back) = (median(&mut fronts)?, median(&mut backs)?);
        (front > 0.0 && back > 0.0).then_some((front, back))
    }

    #[must_use]
    pub fn frame(&self, index: u32) -> Option<&SpriteFrame> {
        self.frames.get(index as usize)
    }

    /// Case à dessiner pour l'image `index` (désignée dans le premier sens)
    /// quand l'animal regarde dans le sens `facing` (1 : vers la droite).
    #[must_use]
    pub fn pick(&self, index: u32, facing: f32) -> Option<Pick<'_>> {
        let right = facing > 0.0;
        let (ax, _) = self.anchor;
        match self.facings {
            Facings::Left | Facings::Right => Some(Pick {
                frame: self.frame(index)?,
                anchor_x: ax,
                faces_right: self.facings == Facings::Right,
            }),
            Facings::Both => {
                // Images de la planche principale, par variante de couleur ;
                // images intermédiaires à la suite, elles aussi dans les deux sens.
                let (base, block) = if index >= self.regular {
                    (
                        self.regular,
                        (self.frames.len() as u32 - self.regular).max(1),
                    )
                } else {
                    (index / self.skin_block * self.skin_block, self.skin_block)
                };
                let half = block / 2;
                let within = index - base;
                let tile = if right && within < half {
                    base + half + within
                } else {
                    index
                };
                // Les images tournées vers la droite sont dessinées en miroir des
                // autres : leur point d'appui aussi.
                let faces_right = tile - base >= half;
                Some(Pick {
                    frame: self.frame(tile)?,
                    anchor_x: if faces_right {
                        self.tile_width as f32 - ax
                    } else {
                        ax
                    },
                    faces_right,
                })
            }
        }
    }
}

/// Une image à compresser : pas vide, pas déjà compressée.
fn compressible(frame: &SpriteFrame) -> bool {
    frame.bitmap.width > 1 && !frame.bitmap.is_compressed()
}

/// Une planche découpée.
struct DecodedPage {
    tile: (u32, u32),
    frames: Vec<SpriteFrame>,
}

/// Décode les planches en parallèle, résultats dans l'ordre des planches.
///
/// Elles sont souvent des dizaines (une bande par animation), de tailles
/// variées. Chaque fil prend la plus grande de celles qui restent : les plus
/// longues partent d'abord et aucun fil ne finit seul sur une grosse planche
/// pendant que les autres attendent (ordonnancement « plus long d'abord »).
fn decode_pages(
    pages: &[&SpritePage],
    sheet: &SpriteSheet,
) -> Vec<Result<DecodedPage, SpriteError>> {
    let mut order: Vec<usize> = (0..pages.len()).collect();
    order.sort_by_key(|&i| {
        let (w, h) = pages[i].png.size().unwrap_or((0, 0));
        std::cmp::Reverse(u64::from(w) * u64::from(h))
    });
    let workers = std::thread::available_parallelism()
        .map_or(1, std::num::NonZero::get)
        .clamp(1, 8)
        .min(pages.len().max(1));
    let next = AtomicUsize::new(0);
    let mut slots: Vec<Option<Result<DecodedPage, SpriteError>>> =
        std::iter::repeat_with(|| None).take(pages.len()).collect();
    let done: Vec<Vec<(usize, Result<DecodedPage, SpriteError>)>> = std::thread::scope(|scope| {
        let jobs: Vec<_> = (0..workers)
            .map(|_| {
                scope.spawn(|| {
                    let mut done = Vec::new();
                    while let Some(&i) = order.get(next.fetch_add(1, Ordering::Relaxed)) {
                        done.push((i, decode_page(pages[i], sheet)));
                    }
                    done
                })
            })
            .collect();
        jobs.into_iter()
            .map(|job| job.join().unwrap_or_default())
            .collect()
    });
    for (i, page) in done.into_iter().flatten() {
        slots[i] = Some(page);
    }
    // Une planche sans résultat : son fil s'est arrêté en route.
    slots
        .into_iter()
        .map(|slot| slot.unwrap_or_else(|| Err(SpriteError::Invalid("décodage interrompu".into()))))
        .collect()
}

/// Décode une planche et découpe ses cases, une rangée de cases à la fois :
/// seule une bande de la hauteur d'une case est en mémoire. Une grande
/// planche desktopPet (2880 × 6480 pixels) occupait 75 Mo une fois décodée
/// entière ; une bande en occupe moins de 3, et elle est encore dans le
/// cache du processeur quand on la découpe.
fn decode_page(page: &SpritePage, sheet: &SpriteSheet) -> Result<DecodedPage, SpriteError> {
    if page.tiles_x == 0 || page.tiles_y == 0 {
        return Err(SpriteError::Invalid("nombre de cases nul".into()));
    }
    let bytes = page.png.bytes()?;
    let mut rows = PngRows::open(&bytes)?;
    let (width, height) = (rows.width, rows.height);
    let (tile_w, tile_h) = (width / page.tiles_x, height / page.tiles_y);
    if tile_w == 0 || tile_h == 0 {
        return Err(SpriteError::Invalid(format!(
            "image {width} × {height} trop petite pour {} × {} cases",
            page.tiles_x, page.tiles_y
        )));
    }
    // Filtres de réduction : les mêmes pour toutes les cases de la planche.
    let reduction = reduced_size(tile_w, tile_h, sheet.scale).map(|(w, h)| Reduction {
        columns: coverage(tile_w, w),
        rows: coverage(tile_h, h),
        texel: tile_w as f32 / w as f32,
    });
    let texel = reduction.as_ref().map_or(1.0, |r| r.texel);
    let mut scratch = Scratch::default();
    let mut band = Vec::new();
    let mut frames = Vec::with_capacity((page.tiles_x * page.tiles_y) as usize);
    for _ in 0..page.tiles_y {
        rows.read_band(tile_h as usize, &mut band)?;
        if let Some([kr, kg, kb]) = sheet.key_color {
            for px in band.as_chunks_mut::<4>().0 {
                if px[0] == kr && px[1] == kg && px[2] == kb {
                    *px = [0, 0, 0, 0];
                }
            }
        }
        let image = Image {
            rgba: &band,
            width: width as usize,
        };
        for tx in 0..page.tiles_x {
            let tile = Area {
                x0: (tx * tile_w) as usize,
                y0: 0,
                x1: ((tx + 1) * tile_w) as usize,
                y1: tile_h as usize,
            };
            // Seuls les pixels visibles de la case comptent : on réduit et on
            // rogne le rectangle qui les contient, pas toute la case.
            let frame = match image.visible(tile) {
                None => empty_frame(texel),
                Some(visible) => match &reduction {
                    None => image.frame(visible, tile),
                    Some(reduction) => reduction.frame(&image, tile, visible, &mut scratch),
                },
            };
            frames.push(frame);
        }
    }
    Ok(DecodedPage {
        tile: (tile_w, tile_h),
        frames,
    })
}

/// Les lignes d'un PNG en RGBA 8 bits, lues bande par bande.
struct PngRows<'a> {
    reader: png::Reader<Cursor<&'a [u8]>>,
    width: u32,
    height: u32,
    color: png::ColorType,
    /// Un PNG entrelacé ne se lit pas ligne à ligne : il est décodé entier
    /// (avec le numéro de la prochaine ligne à rendre).
    whole: Option<(Vec<u8>, usize)>,
    /// Une ligne dans les couleurs du PNG, avant passage en RGBA.
    line: Vec<u8>,
}

impl<'a> PngRows<'a> {
    fn open(bytes: &'a [u8]) -> Result<Self, SpriteError> {
        let mut decoder = png::Decoder::new(Cursor::new(bytes));
        decoder.set_transformations(png::Transformations::normalize_to_color8());
        let mut reader = decoder.read_info()?;
        let (width, height) = (reader.info().width, reader.info().height);
        let (color, _) = reader.output_color_type();
        let whole = if reader.info().interlaced {
            Some((frame_rgba(&mut reader)?.2, 0))
        } else {
            None
        };
        let line = vec![
            0;
            reader
                .output_line_size(width)
                .ok_or_else(|| SpriteError::Invalid("image trop grande".into()))?
        ];
        Ok(Self {
            reader,
            width,
            height,
            color,
            whole,
            line,
        })
    }

    /// Les `count` lignes suivantes, en RGBA, à la place du contenu de `band`.
    fn read_band(&mut self, count: usize, band: &mut Vec<u8>) -> Result<(), SpriteError> {
        let stride = self.width as usize * 4;
        band.clear();
        if let Some((rgba, next)) = &mut self.whole {
            let start = *next * stride;
            band.extend_from_slice(&rgba[start..start + count * stride]);
            *next += count;
            return Ok(());
        }
        let truncated = || SpriteError::Invalid("image tronquée".into());
        band.resize(count * stride, 0);
        for row in band.chunks_exact_mut(stride) {
            // En RGBA, la ligne s'écrit droit dans la bande.
            if self.color == png::ColorType::Rgba {
                self.reader.read_row(row)?.ok_or_else(truncated)?;
            } else {
                self.reader
                    .read_row(&mut self.line)?
                    .ok_or_else(truncated)?;
                expand_to_rgba(&self.line, self.color, row)?;
            }
        }
        Ok(())
    }
}

/// Pixels `src` aux couleurs `color` (8 bits) passés en RGBA dans `dst`.
fn expand_to_rgba(src: &[u8], color: png::ColorType, dst: &mut [u8]) -> Result<(), SpriteError> {
    let out = dst.as_chunks_mut::<4>().0;
    match color {
        png::ColorType::Rgba => dst.copy_from_slice(&src[..dst.len()]),
        png::ColorType::Rgb => {
            for (d, &[r, g, b]) in out.iter_mut().zip(src.as_chunks::<3>().0) {
                *d = [r, g, b, 255];
            }
        }
        png::ColorType::GrayscaleAlpha => {
            for (d, &[g, a]) in out.iter_mut().zip(src.as_chunks::<2>().0) {
                *d = [g, g, g, a];
            }
        }
        png::ColorType::Grayscale => {
            for (d, &g) in out.iter_mut().zip(src) {
                *d = [g, g, g, 255];
            }
        }
        png::ColorType::Indexed => {
            return Err(SpriteError::Invalid("palette non développée".into()));
        }
    }
    Ok(())
}

/// Taille des images réduites d'avance, ou rien si la planche n'est pas plus
/// fine que l'écran (le pixel art agrandi garde alors ses pixels francs).
fn reduced_size(tile_w: u32, tile_h: u32, scale: f32) -> Option<(u32, u32)> {
    let k = scale * PIXELS_PER_POINT;
    if !(k > 0.0 && k < 0.999) {
        return None;
    }
    let w = ((tile_w as f32 * k).round() as u32).max(1);
    let h = ((tile_h as f32 * k).round() as u32).max(1);
    Some((w, h))
}

/// Rectangle de pixels, bornes de fin exclues.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Area {
    x0: usize,
    y0: usize,
    x1: usize,
    y1: usize,
}

/// Une planche décodée (RGBA, alpha non prémultiplié).
struct Image<'a> {
    rgba: &'a [u8],
    width: usize,
}

impl Image<'_> {
    /// Rectangle des pixels visibles (alpha non nul) de `area`, en un seul
    /// passage ligne par ligne.
    fn visible(&self, area: Area) -> Option<Area> {
        let mut bounds: Option<Area> = None;
        for y in area.y0..area.y1 {
            let row = &self.rgba[(y * self.width + area.x0) * 4..(y * self.width + area.x1) * 4];
            let pixels = row.as_chunks::<4>().0;
            let Some(first) = pixels.iter().position(|px| px[3] != 0) else {
                continue;
            };
            let last = pixels.iter().rposition(|px| px[3] != 0).unwrap_or(first);
            let (left, right) = (area.x0 + first, area.x0 + last + 1);
            bounds = Some(match bounds {
                None => Area {
                    x0: left,
                    y0: y,
                    x1: right,
                    y1: y + 1,
                },
                Some(b) => Area {
                    x0: b.x0.min(left),
                    y0: b.y0,
                    x1: b.x1.max(right),
                    y1: y + 1,
                },
            });
        }
        bounds
    }

    /// Image telle quelle (pas de réduction) : les pixels de `visible`,
    /// placés dans leur case `tile`.
    fn frame(&self, visible: Area, tile: Area) -> SpriteFrame {
        let mut out = Vec::with_capacity((visible.x1 - visible.x0) * (visible.y1 - visible.y0) * 4);
        for y in visible.y0..visible.y1 {
            out.extend_from_slice(
                &self.rgba[(y * self.width + visible.x0) * 4..(y * self.width + visible.x1) * 4],
            );
        }
        SpriteFrame {
            bitmap: Bitmap::from_rgba(
                (visible.x1 - visible.x0) as u32,
                (visible.y1 - visible.y0) as u32,
                out,
            ),
            x: (visible.x0 - tile.x0) as f32,
            y: (visible.y0 - tile.y0) as f32,
            texel: 1.0,
            back: self.back_row(visible) - tile.y0 as f32,
        }
    }

    /// Haut du dos dans `visible` : la médiane, sur la moitié centrale de
    /// ses colonnes, de la première ligne bien opaque (pixels de l'image).
    /// Ce qui dépasse un peu (les « z » du sommeil au-dessus de la tête, une
    /// oreille, une queue dressée) n'y compte pas.
    fn back_row(&self, visible: Area) -> f32 {
        let width = visible.x1 - visible.x0;
        let (c0, c1) = (visible.x0 + width / 4, visible.x0 + (width * 3).div_ceil(4));
        let mut rows: Vec<usize> = (c0..c1.max(c0 + 1).min(visible.x1))
            .filter_map(|x| {
                (visible.y0..visible.y1).find(|&y| self.rgba[(y * self.width + x) * 4 + 3] >= 128)
            })
            .collect();
        if rows.is_empty() {
            return visible.y0 as f32;
        }
        let middle = rows.len() / 2;
        *rows.select_nth_unstable(middle).1 as f32
    }
}

/// Case vide : un pixel transparent.
fn empty_frame(texel: f32) -> SpriteFrame {
    SpriteFrame {
        bitmap: Bitmap::from_rgba(1, 1, vec![0u8; 4]),
        x: 0.0,
        y: 0.0,
        texel,
        back: 0.0,
    }
}

/// Réduction d'une case : pour chaque pixel réduit, les pixels de la case
/// qu'il couvre et leur part (filtre de surface), par colonne et par ligne.
struct Reduction {
    columns: Vec<Vec<(usize, f32)>>,
    rows: Vec<Vec<(usize, f32)>>,
    /// Pixels de la planche par pixel réduit.
    texel: f32,
}

/// Mémoire de travail d'une réduction, gardée d'une case à l'autre.
#[derive(Default)]
struct Scratch {
    premultiplied: Vec<[f32; 4]>,
    wide: Vec<[f32; 4]>,
    out: Vec<u8>,
}

impl Reduction {
    /// Pixels réduits dont le filtre touche `[from, to)` : seuls ceux-là
    /// peuvent être visibles.
    fn span(filters: &[Vec<(usize, f32)>], from: usize, to: usize) -> (usize, usize) {
        let first = filters
            .iter()
            .position(|taps| taps.last().is_some_and(|&(s, _)| s >= from))
            .unwrap_or(filters.len());
        let end = filters
            .iter()
            .rposition(|taps| taps.first().is_some_and(|&(s, _)| s < to))
            .map_or(first, |i| i + 1);
        (first, end.max(first))
    }

    /// La case `tile` réduite, rognée à ses pixels visibles. Le résultat est
    /// exactement celui de la réduction de toute la case, rognée ensuite :
    /// hors de `visible`, les pixels sont transparents et ne comptent pas.
    fn frame(&self, image: &Image<'_>, tile: Area, visible: Area, s: &mut Scratch) -> SpriteFrame {
        let local = |area: Area| {
            (
                area.x0 - tile.x0,
                area.y0 - tile.y0,
                area.x1 - tile.x0,
                area.y1 - tile.y0,
            )
        };
        let (vx0, vy0, vx1, vy1) = local(visible);
        let (ox0, ox1) = Self::span(&self.columns, vx0, vx1);
        let (oy0, oy1) = Self::span(&self.rows, vy0, vy1);
        let (ow, oh) = (ox1 - ox0, oy1 - oy0);
        if ow == 0 || oh == 0 {
            return empty_frame(self.texel);
        }
        // Pixels de la case lus par ces filtres.
        let sx0 = self.columns[ox0].first().map_or(0, |t| t.0);
        let sx1 = self.columns[ox1 - 1].last().map_or(sx0, |t| t.0 + 1);
        let sy0 = self.rows[oy0].first().map_or(0, |t| t.0);
        let sy1 = self.rows[oy1 - 1].last().map_or(sy0, |t| t.0 + 1);
        let (sw, sh) = (sx1 - sx0, sy1 - sy0);
        // Couleurs pondérées par leur opacité : pas de liseré sombre.
        s.premultiplied.clear();
        for y in sy0..sy1 {
            let start = ((tile.y0 + y) * image.width + tile.x0 + sx0) * 4;
            for p in image.rgba[start..start + sw * 4].as_chunks::<4>().0 {
                let a = f32::from(p[3]) / 255.0;
                s.premultiplied.push([
                    f32::from(p[0]) * a,
                    f32::from(p[1]) * a,
                    f32::from(p[2]) * a,
                    a,
                ]);
            }
        }
        // Passe horizontale, puis verticale.
        s.wide.clear();
        s.wide.resize(ow * sh, [0.0; 4]);
        for y in 0..sh {
            for (i, taps) in self.columns[ox0..ox1].iter().enumerate() {
                let mut acc = [0.0f32; 4];
                for &(sx, weight) in taps {
                    let p = s.premultiplied[y * sw + sx - sx0];
                    for c in 0..4 {
                        acc[c] += p[c] * weight;
                    }
                }
                s.wide[y * ow + i] = acc;
            }
        }
        s.out.clear();
        for taps in &self.rows[oy0..oy1] {
            for i in 0..ow {
                let mut acc = [0.0f32; 4];
                for &(sy, weight) in taps {
                    let p = s.wide[(sy - sy0) * ow + i];
                    for c in 0..4 {
                        acc[c] += p[c] * weight;
                    }
                }
                s.out.extend_from_slice(&unpremultiply(acc));
            }
        }
        let out = Image {
            rgba: &s.out,
            width: ow,
        };
        let all = Area {
            x0: 0,
            y0: 0,
            x1: ow,
            y1: oh,
        };
        let Some(visible) = out.visible(all) else {
            return empty_frame(self.texel);
        };
        let mut frame = out.frame(visible, all);
        // Coin de l'image dans la case, en pixels de la planche.
        frame.x = (ox0 + visible.x0) as f32 * self.texel;
        frame.y = (oy0 + visible.y0) as f32 * self.texel;
        frame.back = (oy0 as f32 + frame.back) * self.texel;
        frame.texel = self.texel;
        frame
    }
}

/// Pixel réduit, de prémultiplié à RGBA 8 bits non prémultiplié.
fn unpremultiply(acc: [f32; 4]) -> [u8; 4] {
    let a = acc[3];
    if a <= 1e-6 {
        return [0, 0, 0, 0];
    }
    let channel = |v: f32| (v / a).round().clamp(0.0, 255.0) as u8;
    [
        channel(acc[0]),
        channel(acc[1]),
        channel(acc[2]),
        (a * 255.0).round().clamp(0.0, 255.0) as u8,
    ]
}

/// Part de chaque pixel source dans chaque pixel réduit (filtre de surface).
pub(crate) fn coverage(source: u32, target: u32) -> Vec<Vec<(usize, f32)>> {
    let ratio = source as f32 / target as f32;
    (0..target)
        .map(|o| {
            let start = o as f32 * ratio;
            let end = start + ratio;
            let mut taps = Vec::with_capacity(ratio.ceil() as usize + 1);
            let mut s = start.floor() as usize;
            while (s as f32) < end && s < source as usize {
                let cover = end.min(s as f32 + 1.0) - start.max(s as f32);
                if cover > 1e-6 {
                    taps.push((s, cover / ratio));
                }
                s += 1;
            }
            taps
        })
        .collect()
}

/// Réduit une image RGBA (alpha non prémultiplié) en moyennant les surfaces
/// couvertes, couleurs pondérées par leur opacité : pas de liseré sombre.
#[cfg(test)]
fn resample(rgba: &[u8], width: u32, height: u32, out_w: u32, out_h: u32) -> Vec<u8> {
    let reduction = Reduction {
        columns: coverage(width, out_w),
        rows: coverage(height, out_h),
        texel: width as f32 / out_w as f32,
    };
    let image = Image {
        rgba,
        width: width as usize,
    };
    let all = Area {
        x0: 0,
        y0: 0,
        x1: width as usize,
        y1: height as usize,
    };
    let mut out = vec![0u8; (out_w * out_h * 4) as usize];
    let Some(visible) = image.visible(all) else {
        return out;
    };
    let frame = reduction.frame(&image, all, visible, &mut Scratch::default());
    let (fx, fy) = (
        (frame.x / reduction.texel).round() as usize,
        (frame.y / reduction.texel).round() as usize,
    );
    let fw = frame.bitmap.width as usize;
    for (row, line) in frame.bitmap.to_rgba().chunks_exact(fw * 4).enumerate() {
        let start = ((fy + row) * out_w as usize + fx) * 4;
        out[start..start + fw * 4].copy_from_slice(line);
    }
    out
}

/// Décode un PNG quelconque en RGBA 8 bits.
pub fn decode_png_rgba(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), SpriteError> {
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    frame_rgba(&mut decoder.read_info()?)
}

/// L'image entière d'un PNG ouvert, en RGBA 8 bits : largeur, hauteur, pixels.
fn frame_rgba(reader: &mut png::Reader<Cursor<&[u8]>>) -> Result<(u32, u32, Vec<u8>), SpriteError> {
    let size = reader
        .output_buffer_size()
        .ok_or_else(|| SpriteError::Invalid("image trop grande".into()))?;
    let mut buf = vec![0; size];
    let info = reader.next_frame(&mut buf)?;
    let pixels = info.width as usize * info.height as usize;
    // Déjà en RGBA : le tampon sert tel quel, sans copie.
    if info.color_type == png::ColorType::Rgba {
        buf.truncate(pixels * 4);
        return Ok((info.width, info.height, buf));
    }
    let mut rgba = vec![0; pixels * 4];
    expand_to_rgba(&buf, info.color_type, &mut rgba)?;
    Ok((info.width, info.height, rgba))
}

/// Encode une image RGBA en PNG (utilisé par l'aperçu SVG).
#[must_use]
pub fn encode_png_rgba(bitmap: &Bitmap) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, bitmap.width, bitmap.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        if let Ok(mut writer) = encoder.write_header() {
            let _ = writer.write_image_data(&bitmap.to_rgba());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use mouflon_core::{Inbetweens, PngData};
    use std::collections::HashMap;
    use std::sync::Arc;

    fn png(width: u32, height: u32, pixel: impl Fn(u32, u32) -> [u8; 4]) -> Vec<u8> {
        let mut rgba = Vec::new();
        for y in 0..height {
            for x in 0..width {
                rgba.extend_from_slice(&pixel(x, y));
            }
        }
        encode_png_rgba(&Bitmap::from_rgba(width, height, rgba))
    }

    /// Une planche encodée autrement qu'en RGBA (couleurs seules, gris).
    fn png_as(
        color: png::ColorType,
        width: u32,
        height: u32,
        pixel: impl Fn(u32, u32) -> [u8; 4],
    ) -> Vec<u8> {
        let mut data = Vec::new();
        for y in 0..height {
            for x in 0..width {
                let [r, g, b, a] = pixel(x, y);
                match color {
                    png::ColorType::Rgb => data.extend_from_slice(&[r, g, b]),
                    png::ColorType::GrayscaleAlpha => data.extend_from_slice(&[r, a]),
                    _ => data.extend_from_slice(&[r, g, b, a]),
                }
            }
        }
        let mut out = Vec::new();
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(color);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .and_then(|mut writer| writer.write_image_data(&data))
            .expect("PNG");
        out
    }

    /// Images d'une planche de 3 × 4 cases de 4 × 5 pixels.
    fn sliced(png: Vec<u8>) -> Vec<(u32, u32, Vec<u8>, f32, f32)> {
        let frames = SpriteFrames::decode(&SpriteSheet::single(PngData::from(png), 3, 4))
            .expect("planche lisible");
        frames
            .frames
            .iter()
            .map(|f| {
                let b = &f.bitmap;
                (b.width, b.height, b.to_rgba().into_owned(), f.x, f.y)
            })
            .collect()
    }

    #[test]
    fn every_png_layout_gives_the_same_frames() {
        // Celle d'où vient `testdata/planche-entrelacee.png` (même planche,
        // enregistrée entrelacée par ImageMagick).
        let pixel = |x: u32, y: u32| {
            let alpha = if (x + y).is_multiple_of(5) { 0 } else { 255 };
            [
                (x * 20 % 256) as u8,
                (y * 30 % 256) as u8,
                ((x + y) * 7 % 256) as u8,
                alpha,
            ]
        };
        let expected = sliced(png(12, 20, pixel));
        assert_eq!(expected.len(), 12);
        // Chaque rangée de cases vient de ses propres lignes (le vert suit y).
        assert_ne!(expected[0].2, expected[3].2);
        assert_eq!(
            sliced(include_bytes!("testdata/planche-entrelacee.png").to_vec()),
            expected,
            "entrelacée"
        );
        let opaque = |x, y| {
            let [r, g, b, _] = pixel(x, y);
            [r, g, b, 255]
        };
        assert_eq!(
            sliced(png_as(png::ColorType::Rgb, 12, 20, opaque)),
            sliced(png(12, 20, opaque)),
            "sans alpha"
        );
        let grey = |x, y| {
            let [r, _, _, a] = pixel(x, y);
            [r, r, r, a]
        };
        assert_eq!(
            sliced(png_as(png::ColorType::GrayscaleAlpha, 12, 20, grey)),
            sliced(png(12, 20, grey)),
            "gris"
        );
    }

    #[test]
    fn a_fine_sheet_is_reduced_without_dark_fringes() {
        // Bloc 2×2 : deux pixels rouges opaques, deux transparents (noirs).
        let rgba = [
            255, 0, 0, 255, 0, 0, 0, 0, //
            0, 0, 0, 0, 255, 0, 0, 255,
        ];
        assert_eq!(resample(&rgba, 2, 2, 1, 1), vec![255, 0, 0, 128]);
    }

    #[test]
    fn reduction_by_a_fraction_keeps_coverage() {
        // Trois colonnes réduites à deux : celle du milieu se partage.
        let taps = coverage(3, 2);
        assert_eq!(taps[0].len(), 2);
        let total: f32 = taps.iter().flatten().map(|(_, w)| w).sum();
        assert!((total - 2.0).abs() < 1e-5, "{total}");
        // Un blanc opaque reste blanc et opaque.
        let white = vec![255u8; 3 * 3 * 4];
        assert!(resample(&white, 3, 3, 2, 2).iter().all(|&v| v == 255));
    }

    #[test]
    fn slices_tiles_trims_them_and_applies_key_color() {
        // Planche 4×2 pixels, 2×1 cases : la case de gauche est magenta,
        // celle de droite n'a qu'une colonne visible.
        let sheet = SpriteSheet {
            key_color: Some([255, 0, 255]),
            ..SpriteSheet::single(
                PngData::from(png(4, 2, |x, _| match x {
                    0 | 1 => [255, 0, 255, 255],
                    3 => [10, 20, 30, 255],
                    _ => [0, 0, 0, 0],
                })),
                2,
                1,
            )
        };
        let frames = SpriteFrames::decode(&sheet).unwrap();
        assert_eq!(frames.frames.len(), 2);
        assert_eq!((frames.tile_width, frames.tile_height), (2, 2));
        let empty = &frames.frames[0];
        assert_eq!((empty.bitmap.width, empty.bitmap.height), (1, 1));
        let column = &frames.frames[1];
        assert_eq!((column.bitmap.width, column.bitmap.height), (1, 2));
        assert_eq!((column.x, column.y), (1.0, 0.0));
        assert_eq!(&column.bitmap.to_rgba()[..4], &[10, 20, 30, 255]);
    }

    #[test]
    fn both_facings_pick_their_own_frames() {
        // Deux bandes de 2 images de 8 px : vers la gauche, puis vers la droite.
        let strip = |shade: u8| SpritePage {
            png: png(16, 8, move |x, _| [shade, (x / 8) as u8, 0, 255]).into(),
            tiles_x: 2,
            tiles_y: 1,
        };
        let sheet = SpriteSheet {
            pages: vec![strip(10), strip(200)],
            facings: Facings::Both,
            anchor: Some([2.0, 7.0]),
            ..SpriteSheet::single(Vec::new().into(), 1, 1)
        };
        let frames = SpriteFrames::decode(&sheet).unwrap();
        let left = frames.pick(1, -1.0).unwrap();
        assert!(!left.faces_right);
        assert_eq!(&left.frame.bitmap.to_rgba()[..2], &[10, 1]);
        assert_eq!(left.anchor_x, 2.0);
        let right = frames.pick(1, 1.0).unwrap();
        assert!(right.faces_right);
        assert_eq!(&right.frame.bitmap.to_rgba()[..2], &[200, 1]);
        assert_eq!(right.anchor_x, 6.0, "point d'appui en miroir");
    }

    #[test]
    fn inbetweens_follow_the_sheet_in_both_facings() {
        let strip = |shade: u8| SpritePage {
            png: png(16, 8, move |x, _| [shade, (x / 8) as u8, 0, 255]).into(),
            tiles_x: 2,
            tiles_y: 1,
        };
        let sheet = SpriteSheet {
            pages: vec![strip(10), strip(200)],
            facings: Facings::Both,
            inbetweens: Some(Arc::new(Inbetweens {
                pages: vec![strip(30), strip(230)],
                pairs: HashMap::from([((0, 1), (4, 1)), ((1, 0), (5, 1))]),
            })),
            ..SpriteSheet::single(Vec::new().into(), 1, 1)
        };
        let frames = SpriteFrames::decode(&sheet).unwrap();
        assert_eq!(frames.frames.len(), 8);
        // Seconde image intermédiaire : vers la gauche, puis vers la droite.
        let left = frames.pick(5, -1.0).unwrap();
        assert!(!left.faces_right);
        assert_eq!(&left.frame.bitmap.to_rgba()[..2], &[30, 1]);
        let right = frames.pick(5, 1.0).unwrap();
        assert!(right.faces_right);
        assert_eq!(&right.frame.bitmap.to_rgba()[..2], &[230, 1]);
        // La planche principale ne change pas.
        let main = frames.pick(1, 1.0).unwrap();
        assert_eq!(&main.frame.bitmap.to_rgba()[..2], &[200, 1]);
    }

    #[test]
    fn hd_frames_are_reduced_to_two_pixels_per_point() {
        // Case de 8 px affichée à 0,375 point par pixel : 6 px à l'écran Retina.
        let sheet = SpriteSheet {
            scale: 0.375,
            ..SpriteSheet::single(png(8, 8, |_, _| [90, 90, 90, 255]).into(), 1, 1)
        };
        let frames = SpriteFrames::decode(&sheet).unwrap();
        let f = &frames.frames[0];
        assert_eq!((f.bitmap.width, f.bitmap.height), (6, 6));
        assert!((f.texel - 8.0 / 6.0).abs() < 1e-6);
        assert_eq!((frames.tile_width, frames.tile_height), (8, 8));
    }
}
