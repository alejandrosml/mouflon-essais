//! La cuisson garde les traits complets, même lorsque leur transformation
//! SVG étire beaucoup un axe ou que leur jointure forme une pointe longue ;
//! cuite ou non, la lumière du corps ne tombe que sur ce qui est dessiné.

use mouflon_core::{PetView, PoseSignals, Support, Vec2, parse_pet_toml};
use mouflon_render::bake::BakedRig;
use mouflon_render::display::{Bitmap, Command, Paint, Pixels};
use mouflon_render::peniko::{Color, Mix};
use mouflon_render::raster::rasterize;
use mouflon_render::rig::{Item, Rig};
use mouflon_render::{DisplayList, Frame, PetArt, Style, paint_pet};

fn assert_baking_preserves_strokes(svg: &str) {
    let rig = Rig::parse(svg.as_bytes()).expect("dessin d'essai");
    // 0,4 point par unité donne exactement un pixel cuit par unité : aucun
    // redimensionnement ne brouille la comparaison des contours.
    let baked = BakedRig::bake(&rig, 0.4, 0.0).expect("cuisson");
    let mut direct = DisplayList::new(200, 160);
    let mut images = DisplayList::new(200, 160);
    for (index, item) in rig.items.iter().enumerate() {
        if let Item::Stroke {
            path,
            transform,
            paint,
            brush_transform,
            style,
        } = item
        {
            direct.push(Command::Stroke {
                path: path.clone(),
                transform: *transform,
                paint: paint.clone(),
                brush_transform: *brush_transform,
                style: style.clone(),
            });
        }
        if let Some(piece) = baked.piece(index) {
            images.push(Command::Image {
                bitmap: piece.bitmap.clone(),
                transform: piece.placement,
                alpha: 1.0,
                smooth: false,
            });
        }
    }
    let direct = rasterize(&direct).expect("traits");
    let images = rasterize(&images).expect("images cuites");
    let mut opaque = 0;
    for (index, (original, baked)) in direct.pixels().iter().zip(images.pixels()).enumerate() {
        if original.alpha() > 10 {
            opaque += 1;
            assert!(
                baked.alpha().abs_diff(original.alpha()) <= 2,
                "trait coupé en ({}, {}) : alpha {} au lieu de {}",
                index % 200,
                index / 200,
                baked.alpha(),
                original.alpha()
            );
        }
    }
    assert!(opaque > 100, "le tracé de référence doit être visible");
}

#[test]
fn baking_keeps_the_full_width_of_a_non_uniformly_scaled_stroke() {
    assert_baking_preserves_strokes(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="160">
        <path d="M10 20 V100" transform="translate(40 40) scale(10 .1)"
              fill="none" stroke="#222" stroke-width="4" stroke-linecap="round"/>
        </svg>"##,
    );
}

#[test]
fn baking_keeps_a_long_miter_join_inside_the_image() {
    assert_baking_preserves_strokes(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="160">
        <path d="M50 100 L60 20 L70 100" fill="none" stroke="#222"
              stroke-width="4" stroke-linejoin="miter" stroke-miterlimit="20"/>
        </svg>"##,
    );
}

/// L'opacité de chaque pixel d'une image cuite.
fn coverage(bitmap: &Bitmap) -> Vec<u8> {
    match &bitmap.pixels {
        Pixels::Rgba(blob) => blob
            .data()
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| p[3])
            .collect(),
        Pixels::Bc7(_) => panic!("une image cuite est en RGBA"),
    }
}

fn colours(bitmap: &Bitmap) -> Vec<u8> {
    match &bitmap.pixels {
        Pixels::Rgba(blob) => blob.data().to_vec(),
        Pixels::Bc7(_) => panic!("une image cuite est en RGBA"),
    }
}

#[test]
fn the_body_light_changes_the_colours_of_a_baked_piece_but_never_what_it_covers() {
    // La lumière douce est découpée dans la silhouette de toute la toison
    // (corps et encolure), qui déborde de chaque morceau : posée sur le vide,
    // elle y peignait un voile, que l'encolure emportait en descendant
    // brouter (le coin clair au garrot de l'Awassi et du Dorper, le halo
    // autour du cou du Bleu du Maine et du Mérinos d'Arles).
    let drawing = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 120 100">
        <g id="neck"><path d="M70 30L86 26L84 58L66 60Z" fill="#e8e0d0"/>
            <circle id="pivot-neck" cx="72" cy="55" r="0.5"/></g>
        <g id="body"><ellipse cx="48" cy="55" rx="30" ry="23" fill="#eee"/></g>
        <g id="head"><ellipse cx="92" cy="32" rx="12" ry="8" fill="#888"/>
            <circle id="pivot-head" cx="84" cy="30" r="0.5"/></g>
        <circle id="pivot-ground" cx="50" cy="96" r="0.5"/></svg>"##;
    for (name, svg) in [
        ("essai", drawing.as_bytes()),
        ("awassi", include_bytes!("../../../pets/awassi/awassi.svg")),
        ("dorper", include_bytes!("../../../pets/dorper/dorper.svg")),
        (
            "bleu-du-maine",
            include_bytes!("../../../pets/bleu-du-maine/bleu-du-maine.svg"),
        ),
        (
            "merinos-d-arles",
            include_bytes!("../../../pets/merinos-d-arles/merinos-d-arles.svg"),
        ),
        (
            "badger-face-welsh",
            include_bytes!("../../../pets/badger-face-welsh/badger-face-welsh.svg"),
        ),
    ] {
        let rig = Rig::parse(svg).expect("dessin");
        let k = 54.0 / rig.bounds.height();
        let lit = BakedRig::bake(&rig, k, 0.6).expect("cuisson éclairée");
        let plain = BakedRig::bake(&rig, k, 0.0).expect("cuisson sans lumière");
        let mut lit_pieces = 0;
        for index in 0..rig.items.len() {
            let (Some(a), Some(b)) = (lit.piece(index), plain.piece(index)) else {
                continue;
            };
            assert!(
                coverage(&a.bitmap) == coverage(&b.bitmap),
                "{name}, morceau {index}"
            );
            if let Some(mirrored) = &a.mirrored {
                assert!(
                    coverage(mirrored) == coverage(&b.bitmap),
                    "{name}, morceau {index}, retourné"
                );
                // Un petit morceau peut tomber là où la lumière ne change rien.
                lit_pieces += usize::from(colours(&a.bitmap) != colours(&b.bitmap));
            }
        }
        assert!(
            lit_pieces >= 2,
            "{name} : le corps et l'encolure sont éclairés"
        );
    }
}

#[test]
fn the_vector_light_falls_only_on_what_is_drawn_even_when_the_neck_grazes() {
    // L'aperçu dessine les tracés. Sa lumière était un seul calque découpé
    // dans la silhouette de toute la toison au repos, posé avec le corps :
    // l'encolure descendue pour brouter, il éclairait le vide où elle était.
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 120 100">
        <g id="neck"><path d="M70 30L86 26L84 58L66 60Z" fill="#e8e0d0"/>
            <circle id="pivot-neck" cx="72" cy="55" r="0.5"/></g>
        <g id="body"><ellipse cx="48" cy="55" rx="30" ry="23" fill="#eee"/></g>
        <g id="head"><ellipse cx="92" cy="32" rx="12" ry="8" fill="#888"/>
            <circle id="pivot-head" cx="84" cy="30" r="0.5"/></g>
        <g id="leg-front-near"><rect x="66" y="70" width="5" height="26" fill="#555"/></g>
        <g id="leg-back-near"><rect x="26" y="70" width="5" height="26" fill="#555"/></g>
        <circle id="pivot-ground" cx="50" cy="96" r="0.5"/></svg>"##;
    let def = parse_pet_toml(
        r#"
[pet]
id = "essai"
name = "Essai"
category = "sheep"
[appearance]
rig = "essai.svg"
height = 100.0
[[behavior]]
name = "graze"
motion = "graze"
duration = "forever"
"#,
        |_: &str| Ok(svg.as_bytes().to_vec()),
    )
    .expect("pet.toml d'essai");
    let mut art = PetArt::load(&def).expect("dessin d'essai");
    art.baked = None;
    assert!(art.lighting > 0.0);
    for facing in [1.0, -1.0] {
        let view = PetView {
            id: 0,
            ground: Vec2::new(110.0, 140.0),
            facing,
            pose: PoseSignals {
                graze: 1.0,
                ..PoseSignals::default()
            },
            shadow: None,
            sprite_frame: None,
            sprite_offset_y: 0.0,
            opacity: 1.0,
            particles: &[],
            support: Support::Air,
            behavior: "graze",
            grip: None,
        };
        let mut list = DisplayList::new(220, 160);
        let frame = Frame {
            origin: Vec2::new(0.0, 0.0),
            scale: 1.0,
        };
        paint_pet(&art, &view, &frame, &Style::default(), &mut list);
        // Ce qui est dessiné, et la découpe de chaque calque de lumière.
        let mut drawn = DisplayList::new(220, 160);
        let mut lights = Vec::new();
        let mut in_light = false;
        for command in &list.commands {
            match command {
                Command::PushLayer {
                    clip,
                    transform,
                    mix: Mix::SoftLight,
                    ..
                } => {
                    let mut light = DisplayList::new(220, 160);
                    light.fill(clip.clone(), *transform, Paint::Solid(Color::BLACK));
                    lights.push(rasterize(&light).expect("découpe"));
                    in_light = true;
                }
                Command::PopLayer => in_light = false,
                Command::Fill { .. } if !in_light => drawn.push(command.clone()),
                _ => {}
            }
        }
        assert!(lights.len() >= 2, "le corps et l'encolure sont éclairés");
        let drawn = rasterize(&drawn).expect("dessin");
        for light in &lights {
            for (index, (lit, under)) in light.pixels().iter().zip(drawn.pixels()).enumerate() {
                assert!(
                    lit.alpha() < 200 || under.alpha() > 0,
                    "sens {facing} : lumière sur le vide en ({}, {})",
                    index % 220,
                    index / 220
                );
            }
        }
    }
}
