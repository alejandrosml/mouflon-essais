//! L'icône de la barre des menus, dessinée par le code (image « modèle » :
//! macOS la teinte lui-même en clair ou en sombre).

/// Image RGBA noire de `size`×`size` pixels, opaque là où `inside` (en
/// coordonnées normalisées 0–1) est vrai, avec des bords adoucis
/// (suréchantillonnage 4×4).
fn rasterize(size: u32, inside: impl Fn(f32, f32) -> bool) -> Vec<u8> {
    let s = size as f32;
    let mut rgba = vec![0u8; (size * size * 4) as usize];
    for y in 0..size {
        for x in 0..size {
            let mut coverage = 0.0f32;
            for sy in 0..4 {
                for sx in 0..4 {
                    let px = (x as f32 + (sx as f32 + 0.5) / 4.0) / s;
                    let py = (y as f32 + (sy as f32 + 0.5) / 4.0) / s;
                    if inside(px, py) {
                        coverage += 1.0 / 16.0;
                    }
                }
            }
            let i = ((y * size + x) * 4) as usize;
            rgba[i..i + 4].copy_from_slice(&[0, 0, 0, (coverage * 255.0).round() as u8]);
        }
    }
    rgba
}

/// Petite silhouette d'animal en RGBA, `size`×`size` pixels.
#[must_use]
#[cfg_attr(not(any(target_os = "macos", target_os = "windows")), allow(dead_code))]
pub fn tray_icon(size: u32) -> Vec<u8> {
    // Formes en coordonnées normalisées (0–1) : ellipses (cx, cy, rx, ry) et pattes.
    let ellipses = [
        (0.46, 0.55, 0.30, 0.21),
        (0.73, 0.40, 0.17, 0.16),
        (0.78, 0.18, 0.05, 0.11),
    ];
    let legs = [(0.30, 0.62, 0.075, 0.28), (0.56, 0.62, 0.075, 0.28)];
    rasterize(size, |px, py| {
        ellipses.iter().any(|&(cx, cy, rx, ry)| {
            let dx = (px - cx) / rx;
            let dy = (py - cy) / ry;
            dx * dx + dy * dy <= 1.0
        }) || legs
            .iter()
            .any(|&(lx, ly, w, h)| (lx..=lx + w).contains(&px) && (ly..=ly + h).contains(&py))
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn icon_has_ink_and_transparency() {
        let icon = super::tray_icon(36);
        let alphas: Vec<u8> = icon.chunks(4).map(|p| p[3]).collect();
        assert!(alphas.contains(&255));
        assert!(alphas.contains(&0));
    }
}
