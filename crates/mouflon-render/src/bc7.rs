//! Compression BC7 (« BPTC ») des images des animaux.
//!
//! Une image BC7 pèse 8 bits par pixel au lieu de 32 : le GPU la garde telle
//! quelle et la décode lui-même à la lecture. Le format découpe l'image en
//! blocs de 4 × 4 pixels ; chaque bloc garde les deux bouts d'un segment de
//! couleurs (ou de deux segments, un par partie du bloc) et, pour chaque
//! pixel, sa place sur son segment.
//!
//! Le codeur n'emploie que trois des huit modes du format, ceux qui
//! conviennent à des images peintes, cernées et détourées :
//!
//! - mode 6 : un segment RGBA, 16 places (dégradés, aplats, bords doux) ;
//! - mode 7 : deux segments RGBA, 4 places (le contour sombre entre le vide
//!   et le pelage) ;
//! - mode 1 : deux segments RGB opaques, 8 places (deux matières voisines).
//!
//! Chaque segment est ajusté sur l'axe principal des couleurs (analyse en
//! composantes principales), puis par moindres carrés une fois les places
//! choisies ; les partitions ne sont essayées que si le mode 6 laisse une
//! erreur visible. Les couleurs arrivent prémultipliées par leur opacité : un
//! bord détouré est alors un segment vers le noir transparent, que le format
//! représente exactement.

// Le format range composantes, bouts et pixels par leur rang : des boucles
// indicées disent mieux ce qui va où que des itérateurs entrelacés.
#![allow(clippy::needless_range_loop)]

/// Version du codeur : à changer quand ses choix changent, pour que les
/// images gardées sur le disque (voir [`crate::cache`]) soient refaites.
pub const VERSION: u32 = 1;

/// Poids des places sur un segment, en 64es (spécification BC7).
const WEIGHTS2: [u32; 4] = [0, 21, 43, 64];
const WEIGHTS3: [u32; 8] = [0, 9, 18, 27, 37, 46, 55, 64];
const WEIGHTS4: [u32; 16] = [0, 4, 9, 13, 17, 21, 26, 30, 34, 38, 43, 47, 51, 55, 60, 64];

/// Les 64 découpes d'un bloc en deux parties : bit `i` = partie du pixel `i`
/// (lignes de haut en bas, pixels de gauche à droite).
const PARTITIONS: [u16; 64] = [
    0xcccc, 0x8888, 0xeeee, 0xecc8, 0xc880, 0xfeec, 0xfec8, 0xec80, 0xc800, 0xffec, 0xfe80, 0xe800,
    0xffe8, 0xff00, 0xfff0, 0xf000, 0xf710, 0x008e, 0x7100, 0x08ce, 0x008c, 0x7310, 0x3100, 0x8cce,
    0x088c, 0x3110, 0x6666, 0x366c, 0x17e8, 0x0ff0, 0x718e, 0x399c, 0xaaaa, 0xf0f0, 0x5a5a, 0x33cc,
    0x3c3c, 0x55aa, 0x9696, 0xa55a, 0x73ce, 0x13c8, 0x324c, 0x3bdc, 0x6996, 0xc33c, 0x9966, 0x0660,
    0x0272, 0x04e4, 0x4e40, 0x2720, 0xc936, 0x936c, 0x39c6, 0x639c, 0x9336, 0x9cc6, 0x817e, 0xe718,
    0xccf0, 0x0fcc, 0x7744, 0xee22,
];

/// Pixel « d'ancrage » de la seconde partie de chaque découpe (celui de la
/// première est toujours le pixel 0) : sa place s'écrit avec un bit de moins.
const ANCHORS: [u8; 64] = [
    15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 2, 8, 2, 2, 8, 8, 15, 2, 8,
    2, 2, 8, 8, 2, 2, 15, 15, 6, 8, 2, 8, 15, 15, 2, 8, 2, 2, 2, 15, 15, 6, 6, 2, 6, 8, 15, 15, 2,
    2, 15, 15, 15, 15, 15, 2, 2, 15,
];

/// Erreur (somme des carrés des écarts sur les 64 composantes du bloc) sous
/// laquelle le mode 6 suffit : 2 niveaux d'écart moyen par composante,
/// invisible ; au-delà, on essaie les partitions. Mesuré sur les vraies
/// images des animaux : deux fois plus rapide qu'avec 1,5 niveau, pour 0,3 dB
/// de moins.
const GOOD_ENOUGH: u32 = 256;
/// Découpes qui ressemblent le plus aux deux groupes de couleurs du bloc,
/// puis celles d'entre elles essayées en entier (les mieux classées par
/// l'estimation de leur erreur).
const SHORTLIST: usize = 8;
const CANDIDATES: usize = 2;

/// Bloc entièrement transparent : mode 6, tout à zéro.
const EMPTY: [u8; 16] = [0x40, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

/// Code une image RGBA prémultipliée dont les côtés sont des multiples de 4 :
/// 16 octets par bloc, les blocs ligne par ligne.
///
/// # Panics
///
/// Si les côtés ne sont pas des multiples de 4 ou si `rgba` est trop court.
#[must_use]
pub fn encode(rgba: &[u8], width: usize, height: usize) -> Vec<u8> {
    assert!(
        width.is_multiple_of(4) && height.is_multiple_of(4),
        "côtés multiples de 4"
    );
    assert!(rgba.len() >= width * height * 4, "image trop courte");
    let mut out = Vec::with_capacity(width * height);
    let mut block = [[0u8; 4]; 16];
    for by in (0..height).step_by(4) {
        for bx in (0..width).step_by(4) {
            for (i, px) in block.iter_mut().enumerate() {
                let at = ((by + i / 4) * width + bx + i % 4) * 4;
                px.copy_from_slice(&rgba[at..at + 4]);
            }
            out.extend_from_slice(&encode_block(&block));
        }
    }
    out
}

/// Décode des blocs produits par [`encode`] (modes 1, 6 et 7) en RGBA
/// prémultiplié ; un bloc d'un autre mode devient transparent.
#[must_use]
pub fn decode(blocks: &[u8], width: usize, height: usize) -> Vec<u8> {
    let mut out = vec![0u8; width * height * 4];
    let per_row = width / 4;
    for (b, block) in blocks
        .as_chunks::<16>()
        .0
        .iter()
        .enumerate()
        .take(per_row * (height / 4))
    {
        let pixels = decode_block(block);
        let (bx, by) = ((b % per_row) * 4, (b / per_row) * 4);
        for (i, px) in pixels.iter().enumerate() {
            let at = ((by + i / 4) * width + bx + i % 4) * 4;
            out[at..at + 4].copy_from_slice(px);
        }
    }
    out
}

/// Code un bloc de 4 × 4 pixels RGBA prémultipliés.
#[must_use]
pub fn encode_block(px: &[[u8; 4]; 16]) -> [u8; 16] {
    encode_block_tuned(px, GOOD_ENOUGH, SHORTLIST, CANDIDATES)
}

/// [`encode_block`] avec d'autres réglages (pour les mesures) : seuil
/// d'essai des partitions, découpes retenues d'après les deux groupes de
/// couleurs du bloc, puis essayées en entier.
#[doc(hidden)]
#[must_use]
pub fn encode_block_tuned(
    px: &[[u8; 4]; 16],
    good_enough: u32,
    shortlist: usize,
    tried: usize,
) -> [u8; 16] {
    if px.iter().all(|p| *p == [0, 0, 0, 0]) {
        return EMPTY;
    }
    let points = px.map(|p| p.map(f32::from));
    let (mut best, mut best_err) = mode6(px, &points);
    if best_err > good_enough && tried > 0 {
        let opaque = px.iter().all(|p| p[3] == 255);
        for partition in candidates(&points, opaque, shortlist, tried) {
            let (bytes, err) = if opaque {
                two_subsets(px, &points, partition, &MODE1)
            } else {
                two_subsets(px, &points, partition, &MODE7)
            };
            if err < best_err {
                best = bytes;
                best_err = err;
            }
        }
    }
    best
}

// --- Écriture et lecture des 128 bits d'un bloc, bit de poids faible d'abord.

#[derive(Default)]
struct BitWriter {
    bits: u128,
    len: u32,
}

impl BitWriter {
    fn put(&mut self, value: u32, count: u32) {
        debug_assert!(count == 32 || value < (1 << count));
        self.bits |= u128::from(value) << self.len;
        self.len += count;
    }

    fn finish(self) -> [u8; 16] {
        debug_assert_eq!(self.len, 128);
        self.bits.to_le_bytes()
    }
}

struct BitReader {
    bits: u128,
}

impl BitReader {
    fn take(&mut self, count: u32) -> u32 {
        let value = (self.bits & ((1u128 << count) - 1)) as u32;
        self.bits >>= count;
        value
    }
}

// --- Les modes : précision des bouts, bits de parité, places.

/// Ce qui distingue les modes employés.
struct Mode {
    /// Numéro du mode.
    number: u32,
    /// Bits de chaque composante d'un bout (sans le bit de parité).
    bits: u32,
    /// Composantes codées : 3 (RGB, alpha opaque) ou 4 (RGBA).
    channels: usize,
    /// Un bit de parité par bout (sinon un par partie, partagé).
    unique_parity: bool,
    weights: &'static [u32],
    /// Arrondi des composantes, par bit de parité (voir [`quantize_table`]).
    quantized: &'static [[[u8; 2]; 256]; 2],
}

const MODE6: Mode = Mode {
    number: 6,
    bits: 7,
    channels: 4,
    unique_parity: true,
    weights: &WEIGHTS4,
    quantized: &QUANTIZE7,
};

const MODE1: Mode = Mode {
    number: 1,
    bits: 6,
    channels: 3,
    unique_parity: false,
    weights: &WEIGHTS3,
    quantized: &QUANTIZE6,
};

const MODE7: Mode = Mode {
    number: 7,
    bits: 5,
    channels: 4,
    unique_parity: true,
    weights: &WEIGHTS2,
    quantized: &QUANTIZE5,
};

/// Valeur sur 8 bits d'un bout de `bits + 1` bits (bit de parité compris),
/// comme le décodeur l'étend : ses bits de poids fort recopiés en bas.
const fn expand(value: u32, total: u32) -> u8 {
    if total >= 8 {
        return value as u8;
    }
    ((value << (8 - total)) | (value >> (2 * total - 8))) as u8
}

/// Pour chaque valeur visée (0 à 255) : la valeur de `bits` bits qui
/// l'approche le mieux avec ce bit de parité, et ce qu'elle redonne une fois
/// décodée. Calculée à la compilation : l'arrondi d'un bout ne coûte plus
/// qu'une lecture.
const fn quantize_table(bits: u32, parity: u32) -> [[u8; 2]; 256] {
    let total = bits + 1;
    let max = (1u32 << bits) - 1;
    let mut table = [[0u8; 2]; 256];
    let mut target = 0;
    while target < 256 {
        let mut best = [0u8; 2];
        let mut best_err = u32::MAX;
        let mut q = 0;
        while q <= max {
            let value = expand((q << 1) | parity, total);
            let err = (value as i32 - target as i32).unsigned_abs();
            if err < best_err {
                best = [q as u8, value];
                best_err = err;
            }
            q += 1;
        }
        table[target] = best;
        target += 1;
    }
    table
}

const QUANTIZE7: [[[u8; 2]; 256]; 2] = [quantize_table(7, 0), quantize_table(7, 1)];
const QUANTIZE6: [[[u8; 2]; 256]; 2] = [quantize_table(6, 0), quantize_table(6, 1)];
const QUANTIZE5: [[[u8; 2]; 256]; 2] = [quantize_table(5, 0), quantize_table(5, 1)];

/// Meilleure valeur d'une composante d'un bout pour approcher `target` (0 à
/// 255) avec ce bit de parité, dans ce mode ; et ce qu'elle redonne.
fn quantize(target: f32, parity: u32, mode: &Mode) -> (u32, u8) {
    let [q, value] = mode.quantized[parity as usize][target.round().clamp(0.0, 255.0) as usize];
    (u32::from(q), value)
}

fn interpolate(e0: u8, e1: u8, weight: u32) -> u8 {
    ((u32::from(e0) * (64 - weight) + u32::from(e1) * weight + 32) >> 6) as u8
}

/// Places des pixels `members` sur le segment `e0`–`e1` (couleurs décodées)
/// et erreur totale. La place se devine par projection sur le segment (les
/// poids sont presque régulièrement espacés), puis se vérifie sur ses deux
/// voisines avec les vraies couleurs de la palette.
fn assign(
    px: &[[u8; 4]; 16],
    members: u16,
    e0: [u8; 4],
    e1: [u8; 4],
    weights: &[u32],
    indices: &mut [u8; 16],
) -> u32 {
    let n = weights.len();
    let mut palette = [[0i32; 4]; 16];
    for (entry, &w) in palette.iter_mut().zip(weights) {
        for c in 0..4 {
            entry[c] = i32::from(interpolate(e0[c], e1[c], w));
        }
    }
    let axis = [0, 1, 2, 3].map(|c| i32::from(e1[c]) - i32::from(e0[c]));
    let length2: i32 = axis.iter().map(|v| v * v).sum();
    let scale = if length2 > 0 {
        (n - 1) as f32 / length2 as f32
    } else {
        0.0
    };
    let mut total = 0u32;
    for (i, p) in px.iter().enumerate() {
        if members & (1 << i) == 0 {
            continue;
        }
        let along: i32 = (0..4)
            .map(|c| (i32::from(p[c]) - i32::from(e0[c])) * axis[c])
            .sum();
        let guess = ((along as f32 * scale).round() as i32).clamp(0, n as i32 - 1) as usize;
        let mut best = (0usize, u32::MAX);
        for k in guess.saturating_sub(1)..(guess + 2).min(n) {
            let entry = palette[k];
            let mut err = 0u32;
            for c in 0..4 {
                let d = entry[c] - i32::from(p[c]);
                err += (d * d) as u32;
            }
            if err < best.1 {
                best = (k, err);
            }
        }
        indices[i] = best.0 as u8;
        total += best.1;
    }
    total
}

/// Segment qui passe au plus près des points `members` : leur axe principal
/// (méthode de la puissance sur la covariance), borné par leurs projections.
fn principal_segment(points: &[[f32; 4]; 16], members: u16) -> ([f32; 4], [f32; 4]) {
    let count = members.count_ones() as f32;
    let mut mean = [0.0f32; 4];
    for (i, p) in points.iter().enumerate() {
        if members & (1 << i) != 0 {
            for c in 0..4 {
                mean[c] += p[c];
            }
        }
    }
    for m in &mut mean {
        *m /= count.max(1.0);
    }
    let mut cov = [[0.0f32; 4]; 4];
    let mut lo = [f32::INFINITY; 4];
    let mut hi = [f32::NEG_INFINITY; 4];
    for (i, p) in points.iter().enumerate() {
        if members & (1 << i) == 0 {
            continue;
        }
        let d = [
            p[0] - mean[0],
            p[1] - mean[1],
            p[2] - mean[2],
            p[3] - mean[3],
        ];
        for r in 0..4 {
            for c in r..4 {
                cov[r][c] += d[r] * d[c];
            }
            lo[r] = lo[r].min(p[r]);
            hi[r] = hi[r].max(p[r]);
        }
    }
    for r in 0..4 {
        for c in 0..r {
            cov[r][c] = cov[c][r];
        }
    }
    // Départ : la diagonale de la boîte englobante (orientée par la
    // covariance), puis quelques itérations de la méthode de la puissance.
    let mut axis = [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2], hi[3] - lo[3]];
    for c in 1..4 {
        if cov[0][c] < 0.0 {
            axis[c] = -axis[c];
        }
    }
    for _ in 0..6 {
        let mut next = [0.0f32; 4];
        for r in 0..4 {
            for c in 0..4 {
                next[r] += cov[r][c] * axis[c];
            }
        }
        let norm = next.iter().map(|v| v * v).sum::<f32>().sqrt();
        if norm < 1e-6 {
            break;
        }
        axis = next.map(|v| v / norm);
    }
    let norm = axis.iter().map(|v| v * v).sum::<f32>().sqrt();
    if norm < 1e-6 {
        return (mean, mean);
    }
    let axis = axis.map(|v| v / norm);
    let (mut tmin, mut tmax) = (f32::INFINITY, f32::NEG_INFINITY);
    for (i, p) in points.iter().enumerate() {
        if members & (1 << i) == 0 {
            continue;
        }
        let t: f32 = (0..4).map(|c| (p[c] - mean[c]) * axis[c]).sum();
        tmin = tmin.min(t);
        tmax = tmax.max(t);
    }
    let at = |t: f32| [0, 1, 2, 3].map(|c| (mean[c] + axis[c] * t).clamp(0.0, 255.0));
    (at(tmin), at(tmax))
}

/// Bouts qui minimisent l'erreur quadratique une fois les places choisies
/// (moindres carrés), ou rien si toutes les places sont égales.
fn refine(
    points: &[[f32; 4]; 16],
    members: u16,
    indices: &[u8; 16],
    weights: &[u32],
) -> Option<([f32; 4], [f32; 4])> {
    let (mut a00, mut a01, mut a11) = (0.0f32, 0.0f32, 0.0f32);
    let mut b0 = [0.0f32; 4];
    let mut b1 = [0.0f32; 4];
    for (i, p) in points.iter().enumerate() {
        if members & (1 << i) == 0 {
            continue;
        }
        let w = weights[usize::from(indices[i])] as f32 / 64.0;
        let v = 1.0 - w;
        a00 += v * v;
        a01 += v * w;
        a11 += w * w;
        for c in 0..4 {
            b0[c] += v * p[c];
            b1[c] += w * p[c];
        }
    }
    let det = a00 * a11 - a01 * a01;
    if det.abs() < 1e-4 {
        return None;
    }
    let e0 = [0, 1, 2, 3].map(|c| ((a11 * b0[c] - a01 * b1[c]) / det).clamp(0.0, 255.0));
    let e1 = [0, 1, 2, 3].map(|c| ((a00 * b1[c] - a01 * b0[c]) / det).clamp(0.0, 255.0));
    Some((e0, e1))
}

// --- Ajustement d'un segment dans un mode donné.

/// Un segment ajusté : erreur, bouts quantifiés, bits de parité, places.
struct Fit {
    err: u32,
    q: [[u32; 4]; 2],
    parity: [u32; 2],
    indices: [u8; 16],
}

/// Tours d'ajustement : places, puis bouts par moindres carrés, et ainsi de
/// suite ; tous les bits de parité ne sont essayés qu'au dernier tour.
const ROUNDS: usize = 3;

/// Écart d'un bout à sa valeur arrondie avec ce bit de parité.
fn rounding_error(end: [f32; 4], parity: u32, mode: &Mode) -> f32 {
    let mut err = 0.0;
    for c in 0..mode.channels {
        let (_, value) = quantize(end[c], parity, mode);
        err += (f32::from(value) - end[c]).powi(2);
    }
    err
}

/// Bit de parité qui approche le mieux ce bout, une fois arrondi.
fn best_parity(end: [f32; 4], mode: &Mode) -> u32 {
    u32::from(rounding_error(end, 1, mode) < rounding_error(end, 0, mode))
}

/// Ajuste un segment sur les pixels `members` dans ce mode.
fn fit(px: &[[u8; 4]; 16], points: &[[f32; 4]; 16], members: u16, mode: &Mode) -> Fit {
    let opaque = mode.channels == 3;
    let (mut e0, mut e1) = principal_segment(points, members);
    let mut best: Option<Fit> = None;
    for round in 0..ROUNDS {
        if opaque {
            e0[3] = 255.0;
            e1[3] = 255.0;
        }
        let last = round + 1 == ROUNDS;
        let guess = if mode.unique_parity {
            (best_parity(e0, mode), best_parity(e1, mode))
        } else {
            // Une parité pour les deux bouts : celle qui les approche le mieux ensemble.
            let shared = u32::from(
                rounding_error(e0, 1, mode) + rounding_error(e1, 1, mode)
                    < rounding_error(e0, 0, mode) + rounding_error(e1, 0, mode),
            );
            (shared, shared)
        };
        let all: &[(u32, u32)] = match (last, mode.unique_parity) {
            (false, _) => &[guess],
            (true, true) => &[(0, 0), (0, 1), (1, 0), (1, 1)],
            (true, false) => &[(0, 0), (1, 1)],
        };
        let candidates = if last {
            all
        } else {
            std::slice::from_ref(&guess)
        };
        for &(p0, p1) in candidates {
            let mut q = [[0u32; 4]; 2];
            let mut d0 = [255u8; 4];
            let mut d1 = [255u8; 4];
            for c in 0..mode.channels {
                (q[0][c], d0[c]) = quantize(e0[c], p0, mode);
                (q[1][c], d1[c]) = quantize(e1[c], p1, mode);
            }
            let mut indices = [0u8; 16];
            let err = assign(px, members, d0, d1, mode.weights, &mut indices);
            if best.as_ref().is_none_or(|b| err < b.err) {
                best = Some(Fit {
                    err,
                    q,
                    parity: [p0, p1],
                    indices,
                });
            }
        }
        if last || best.as_ref().is_some_and(|b| b.err == 0) {
            break;
        }
        let Some(b) = best.as_ref() else { break };
        match refine(points, members, &b.indices, mode.weights) {
            Some((n0, n1)) => (e0, e1) = (n0, n1),
            None => break,
        }
    }
    best.unwrap_or(Fit {
        err: u32::MAX,
        q: [[0; 4]; 2],
        parity: [0; 2],
        indices: [0; 16],
    })
}

// --- Mode 6 : un segment RGBA, bouts sur 7 bits plus un bit de parité chacun.

fn mode6(px: &[[u8; 4]; 16], points: &[[f32; 4]; 16]) -> ([u8; 16], u32) {
    let Fit {
        err,
        mut q,
        mut parity,
        mut indices,
    } = fit(px, points, 0xffff, &MODE6);
    if err == u32::MAX {
        return (EMPTY, err);
    }
    // Le pixel 0 s'écrit sur 3 bits : sa place doit être dans la première moitié.
    if indices[0] >= 8 {
        q.swap(0, 1);
        parity.swap(0, 1);
        for index in &mut indices {
            *index = 15 - *index;
        }
    }
    let mut w = BitWriter::default();
    w.put(1 << 6, 7);
    for c in 0..4 {
        w.put(q[0][c], 7);
        w.put(q[1][c], 7);
    }
    w.put(parity[0], 1);
    w.put(parity[1], 1);
    for (i, &index) in indices.iter().enumerate() {
        w.put(u32::from(index), if i == 0 { 3 } else { 4 });
    }
    (w.finish(), err)
}

// --- Modes à deux parties (1 et 7).

/// Les découpes les plus prometteuses. D'abord celles qui ressemblent le
/// plus aux deux groupes de couleurs du bloc (au plus `shortlist`) ; parmi
/// elles, celles dont les deux parties tiennent le mieux sur une droite
/// chacune (reste de la variance hors de l'axe principal de chaque partie).
fn candidates(points: &[[f32; 4]; 16], opaque: bool, shortlist: usize, tried: usize) -> Vec<usize> {
    let channels = if opaque { 3 } else { 4 };
    let groups = two_clusters(points, channels);
    // Pixels rangés autrement que les deux groupes, dans un sens ou dans l'autre.
    let mut near: [(u32, usize); 64] = std::array::from_fn(|n| {
        let mask = PARTITIONS[n];
        (
            (mask ^ groups)
                .count_ones()
                .min((mask ^ !groups).count_ones()),
            n,
        )
    });
    near.sort_unstable();
    let mut scored: Vec<(f32, usize)> = near
        .iter()
        .take(shortlist.max(tried))
        .map(|&(_, n)| {
            let mask = PARTITIONS[n];
            let residual =
                line_residual(points, !mask, channels) + line_residual(points, mask, channels);
            (residual, n)
        })
        .collect();
    scored.sort_by(|a, b| a.0.total_cmp(&b.0));
    scored.iter().take(tried).map(|s| s.1).collect()
}

/// Les deux groupes de couleurs du bloc, par la méthode des k-moyennes à deux
/// centres : bit `i` levé si le pixel `i` est dans le second.
fn two_clusters(points: &[[f32; 4]; 16], channels: usize) -> u16 {
    let distance = |a: &[f32; 4], b: &[f32; 4]| -> f32 {
        let mut d = 0.0;
        for c in 0..channels {
            d += (a[c] - b[c]).powi(2);
        }
        d
    };
    let farthest = |from: &[f32; 4]| -> usize {
        let mut best = (0, -1.0f32);
        for (i, p) in points.iter().enumerate() {
            let d = distance(p, from);
            if d > best.1 {
                best = (i, d);
            }
        }
        best.0
    };
    // Départ : le pixel le plus loin de la moyenne, puis le plus loin de lui.
    let mut mean = [0.0f32; 4];
    for p in points {
        for c in 0..4 {
            mean[c] += p[c] / 16.0;
        }
    }
    let first = farthest(&mean);
    let mut centers = [points[first], points[farthest(&points[first])]];
    let mut groups = 0u16;
    for _ in 0..4 {
        let mut next = 0u16;
        let mut sums = [[0.0f32; 4]; 2];
        let mut counts = [0.0f32; 2];
        for (i, p) in points.iter().enumerate() {
            let side = usize::from(distance(p, &centers[1]) < distance(p, &centers[0]));
            next |= (side as u16) << i;
            counts[side] += 1.0;
            for c in 0..4 {
                sums[side][c] += p[c];
            }
        }
        if next == groups {
            break;
        }
        groups = next;
        for side in 0..2 {
            if counts[side] > 0.0 {
                centers[side] = sums[side].map(|s| s / counts[side]);
            }
        }
    }
    groups
}

/// Ce qui reste de la variance des points `members` hors de leur axe
/// principal : l'erreur d'un segment idéal, sans arrondi.
fn line_residual(points: &[[f32; 4]; 16], members: u16, channels: usize) -> f32 {
    let count = members.count_ones();
    if count <= 1 {
        return 0.0;
    }
    let mut mean = [0.0f32; 4];
    for (i, p) in points.iter().enumerate() {
        if members & (1 << i) != 0 {
            for c in 0..channels {
                mean[c] += p[c];
            }
        }
    }
    for m in &mut mean {
        *m /= count as f32;
    }
    let mut cov = [[0.0f32; 4]; 4];
    for (i, p) in points.iter().enumerate() {
        if members & (1 << i) == 0 {
            continue;
        }
        for r in 0..channels {
            let dr = p[r] - mean[r];
            for c in r..channels {
                cov[r][c] += dr * (p[c] - mean[c]);
            }
        }
    }
    for r in 0..channels {
        for c in 0..r {
            cov[r][c] = cov[c][r];
        }
    }
    let trace: f32 = (0..channels).map(|c| cov[c][c]).sum();
    let mut axis = [1.0f32, 1.0, 1.0, 1.0];
    let mut lambda = 0.0;
    for _ in 0..5 {
        let mut next = [0.0f32; 4];
        for r in 0..channels {
            for c in 0..channels {
                next[r] += cov[r][c] * axis[c];
            }
        }
        let norm = next[..channels].iter().map(|v| v * v).sum::<f32>().sqrt();
        if norm < 1e-6 {
            return 0.0;
        }
        lambda = norm;
        axis = next.map(|v| v / norm);
    }
    (trace - lambda).max(0.0)
}

fn two_subsets(
    px: &[[u8; 4]; 16],
    points: &[[f32; 4]; 16],
    partition: usize,
    mode: &Mode,
) -> ([u8; 16], u32) {
    let mask = PARTITIONS[partition];
    let subsets = [!mask, mask];
    let mut q = [[[0u32; 4]; 2]; 2];
    let mut parity = [[0u32; 2]; 2];
    let mut indices = [0u8; 16];
    let mut total = 0u32;
    for (s, &members) in subsets.iter().enumerate() {
        let f = fit(px, points, members, mode);
        if f.err == u32::MAX {
            return (EMPTY, u32::MAX);
        }
        total = total.saturating_add(f.err);
        q[s] = f.q;
        parity[s] = f.parity;
        for i in 0..16 {
            if members & (1 << i) != 0 {
                indices[i] = f.indices[i];
            }
        }
    }
    // Les pixels d'ancrage s'écrivent avec un bit de moins : leur place doit
    // être dans la première moitié, quitte à échanger les bouts de la partie.
    let top = mode.weights.len() as u8;
    let anchors = [0usize, usize::from(ANCHORS[partition])];
    for (s, &anchor) in anchors.iter().enumerate() {
        if indices[anchor] >= top / 2 {
            q[s].swap(0, 1);
            parity[s].swap(0, 1);
            for i in 0..16 {
                if subsets[s] & (1 << i) != 0 {
                    indices[i] = top - 1 - indices[i];
                }
            }
        }
    }
    let index_bits = mode.weights.len().trailing_zeros();
    let mut w = BitWriter::default();
    w.put(1 << mode.number, mode.number + 1);
    w.put(partition as u32, 6);
    for c in 0..mode.channels {
        for subset in &q {
            w.put(subset[0][c], mode.bits);
            w.put(subset[1][c], mode.bits);
        }
    }
    if mode.unique_parity {
        for subset in &parity {
            w.put(subset[0], 1);
            w.put(subset[1], 1);
        }
    } else {
        w.put(parity[0][0], 1);
        w.put(parity[1][0], 1);
    }
    for (i, &index) in indices.iter().enumerate() {
        let bits = if anchors.contains(&i) {
            index_bits - 1
        } else {
            index_bits
        };
        w.put(u32::from(index), bits);
    }
    (w.finish(), total)
}

/// Décode un bloc des modes 1, 6 ou 7 (ceux du codeur).
#[must_use]
pub fn decode_block(block: &[u8; 16]) -> [[u8; 4]; 16] {
    let mut r = BitReader {
        bits: u128::from_le_bytes(*block),
    };
    let mode = (r.bits as u32).trailing_zeros();
    r.take(mode + 1);
    let mut out = [[0u8; 4]; 16];
    match mode {
        6 => {
            let mut q = [[0u32; 4]; 2];
            for c in 0..4 {
                q[0][c] = r.take(7);
                q[1][c] = r.take(7);
            }
            let p = [r.take(1), r.take(1)];
            let ends = [0, 1].map(|e| q[e].map(|v| expand((v << 1) | p[e], 8)));
            for (i, px) in out.iter_mut().enumerate() {
                let index = r.take(if i == 0 { 3 } else { 4 });
                let w = WEIGHTS4[index as usize];
                *px = [0, 1, 2, 3].map(|c| interpolate(ends[0][c], ends[1][c], w));
            }
        }
        1 | 7 => {
            let spec = if mode == 1 { &MODE1 } else { &MODE7 };
            let partition = r.take(6) as usize;
            let mut q = [[[0u32; 4]; 2]; 2];
            for c in 0..spec.channels {
                for subset in &mut q {
                    subset[0][c] = r.take(spec.bits);
                    subset[1][c] = r.take(spec.bits);
                }
            }
            let mut parity = [[0u32; 2]; 2];
            if spec.unique_parity {
                for subset in &mut parity {
                    *subset = [r.take(1), r.take(1)];
                }
            } else {
                let shared = [r.take(1), r.take(1)];
                parity = [[shared[0]; 2], [shared[1]; 2]];
            }
            let total = spec.bits + 1;
            let mut ends = [[[255u8; 4]; 2]; 2];
            for s in 0..2 {
                for e in 0..2 {
                    for c in 0..spec.channels {
                        ends[s][e][c] = expand((q[s][e][c] << 1) | parity[s][e], total);
                    }
                }
            }
            let mask = PARTITIONS[partition];
            let anchors = [0usize, usize::from(ANCHORS[partition])];
            let index_bits = spec.weights.len().trailing_zeros();
            for (i, px) in out.iter_mut().enumerate() {
                let bits = if anchors.contains(&i) {
                    index_bits - 1
                } else {
                    index_bits
                };
                let w = spec.weights[r.take(bits) as usize];
                let s = usize::from(mask & (1 << i) != 0);
                *px = [0, 1, 2, 3].map(|c| interpolate(ends[s][0][c], ends[s][1][c], w));
            }
        }
        _ => {}
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Décodage de référence, indépendant du nôtre.
    fn reference(block: &[u8; 16]) -> [[u8; 4]; 16] {
        let mut out = [0u8; 64];
        bcdec_rs::bc7(block, &mut out, 16);
        let mut px = [[0u8; 4]; 16];
        for (i, p) in px.iter_mut().enumerate() {
            p.copy_from_slice(&out[i * 4..i * 4 + 4]);
        }
        px
    }

    fn premultiply(p: [u8; 4]) -> [u8; 4] {
        let a = u16::from(p[3]);
        let m = |c: u8| ((u16::from(c) * a + 127) / 255) as u8;
        [m(p[0]), m(p[1]), m(p[2]), p[3]]
    }

    /// Petit générateur déterministe (PCG) pour des blocs variés.
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u32 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let x = (((self.0 >> 18) ^ self.0) >> 27) as u32;
            x.rotate_right((self.0 >> 59) as u32)
        }
        fn byte(&mut self) -> u8 {
            (self.next() >> 24) as u8
        }
    }

    /// Blocs qui ressemblent à ceux des animaux : aplats, dégradés, bords
    /// détourés, contours sombres, deux matières, bruit.
    fn blocks() -> Vec<[[u8; 4]; 16]> {
        let mut rng = Rng(7);
        let mut out = Vec::new();
        for kind in 0..6 {
            for _ in 0..200 {
                let c0 = [rng.byte(), rng.byte(), rng.byte(), 255];
                let c1 = [rng.byte(), rng.byte(), rng.byte(), 255];
                let mask = PARTITIONS[(rng.next() % 64) as usize];
                let mut b = [[0u8; 4]; 16];
                for (i, p) in b.iter_mut().enumerate() {
                    let (x, y) = ((i % 4) as f32, (i / 4) as f32);
                    *p = match kind {
                        0 => c0,
                        1 => {
                            let t = (x + y) / 6.0;
                            [0, 1, 2, 3].map(|c| {
                                (f32::from(c0[c]) * (1.0 - t) + f32::from(c1[c]) * t).round() as u8
                            })
                        }
                        2 => {
                            // Bord détouré : opacité qui monte.
                            let a = ((x * 0.35 + y * 0.2) * 255.0).min(255.0) as u8;
                            premultiply([c0[0], c0[1], c0[2], a])
                        }
                        3 => {
                            // Vide, contour sombre, pelage clair.
                            let d = x + y * 0.5;
                            if d < 1.0 {
                                [0, 0, 0, 0]
                            } else if d < 2.2 {
                                premultiply([40, 30, 25, 255])
                            } else {
                                premultiply([245, 240, 230, 255])
                            }
                        }
                        4 => {
                            if mask & (1 << i) == 0 {
                                c0
                            } else {
                                c1
                            }
                        }
                        _ => premultiply([rng.byte(), rng.byte(), rng.byte(), rng.byte()]),
                    };
                }
                out.push(b);
            }
        }
        out
    }

    #[test]
    fn our_decoder_agrees_with_the_reference_decoder() {
        for b in blocks() {
            let block = encode_block(&b);
            assert_eq!(decode_block(&block), reference(&block), "bloc {b:?}");
        }
    }

    #[test]
    fn empty_and_solid_blocks_come_back_almost_exactly() {
        assert_eq!(decode_block(&encode_block(&[[0; 4]; 16])), [[0; 4]; 16]);
        let mut rng = Rng(3);
        for _ in 0..500 {
            let c = premultiply([rng.byte(), rng.byte(), rng.byte(), rng.byte()]);
            let back = reference(&encode_block(&[c; 16]));
            for p in back {
                for k in 0..4 {
                    assert!(p[k].abs_diff(c[k]) <= 1, "{c:?} → {p:?}");
                }
            }
        }
    }

    /// Rapport signal sur bruit crête (dB) sur toutes les composantes.
    fn psnr(blocks: &[[[u8; 4]; 16]]) -> f64 {
        let mut sum = 0.0f64;
        let mut n = 0.0f64;
        for b in blocks {
            let back = reference(&encode_block(b));
            for (p, q) in b.iter().zip(back.iter()) {
                for k in 0..4 {
                    let d = f64::from(p[k]) - f64::from(q[k]);
                    sum += d * d;
                    n += 1.0;
                }
            }
        }
        10.0 * (255.0f64 * 255.0 / (sum / n).max(1e-9)).log10()
    }

    #[test]
    fn painted_blocks_keep_their_look() {
        let all = blocks();
        // Aplats, dégradés, bords, contours, deux matières (sans le bruit).
        let regular = &all[..1000];
        let quality = psnr(regular);
        assert!(quality > 44.0, "{quality:.1} dB");
    }

    #[test]
    fn whole_images_round_trip_through_the_reference_decoder() {
        let (w, h) = (8, 12);
        let mut rgba = vec![0u8; w * h * 4];
        for y in 0..h {
            for x in 0..w {
                let p = premultiply([
                    (x * 30) as u8,
                    (y * 20) as u8,
                    128,
                    (x * y * 4).min(255) as u8,
                ]);
                rgba[(y * w + x) * 4..(y * w + x) * 4 + 4].copy_from_slice(&p);
            }
        }
        let blocks = encode(&rgba, w, h);
        assert_eq!(blocks.len(), w * h);
        let ours = decode(&blocks, w, h);
        let mut theirs = vec![0u8; w * h * 4];
        for (b, block) in blocks.as_chunks::<16>().0.iter().enumerate() {
            let (bx, by) = ((b % 2) * 4, (b / 2) * 4);
            let mut px = [0u8; 64];
            bcdec_rs::bc7(block, &mut px, 16);
            for row in 0..4 {
                let at = ((by + row) * w + bx) * 4;
                theirs[at..at + 16].copy_from_slice(&px[row * 16..row * 16 + 16]);
            }
        }
        assert_eq!(ours, theirs);
        // Un dégradé dans deux sens à la fois, sous une opacité qui croît comme
        // x·y : rien d'aussi dur dans un dessin, et pourtant bien rendu.
        let sum: f64 = rgba
            .iter()
            .zip(&ours)
            .map(|(a, b)| (f64::from(*a) - f64::from(*b)).powi(2))
            .sum();
        let psnr = 10.0 * (255.0f64.powi(2) / (sum / rgba.len() as f64)).log10();
        assert!(psnr > 33.0, "{psnr:.1} dB");
    }
}
