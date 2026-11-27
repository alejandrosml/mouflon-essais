//! Le dessin livré, son squelette et le peintre sont vérifiés ensemble :
//! articulations couvertes, appuis, broutage et morceaux reproductibles.

use std::io::Cursor;
use std::path::{Path, PathBuf};

use mouflon_core::{BoneRole, Piece, PoseSignals, Skeleton, Vec2, load_pet};
use mouflon_render::display::Command as Drawing;
use mouflon_render::kurbo::{Affine, Point, Rect};
use mouflon_render::skeleton::{SkeletonArt, paint_skeleton};
use mouflon_render::{DisplayList, Look, PetArt};

fn pets() -> PathBuf {
    Path::new(env!("MOUFLON_REPOSITORY")).join("pets")
}

fn solognote() -> PetArt {
    let def = load_pet(&pets().join("solognote/pet.toml")).expect("pet.toml de la Solognote");
    assert_eq!(def.meta.id, "solognote");
    assert_eq!(def.meta.name, "Solognote");
    assert_eq!(def.meta.category.as_deref(), Some("sheep"));
    PetArt::load(&def).expect("morceaux de la Solognote")
}

fn skeleton(art: &PetArt) -> &SkeletonArt {
    let Look::Skeleton(skeleton) = &art.look else {
        panic!("la Solognote doit rester un squelette peint");
    };
    skeleton
}

fn role(sk: &Skeleton, wanted: BoneRole) -> usize {
    sk.bones
        .iter()
        .position(|bone| bone.role == Some(wanted))
        .unwrap_or_else(|| panic!("pas d'os {wanted:?}"))
}

fn point(p: Vec2) -> Point {
    Point::new(f64::from(p.x), f64::from(p.y))
}

struct Image {
    width: usize,
    height: usize,
    rgba: Vec<u8>,
}

impl Image {
    fn of(piece: &Piece) -> Self {
        let bytes = piece.png.bytes().expect("PNG du morceau");
        let mut decoder = png::Decoder::new(Cursor::new(bytes.as_ref()));
        decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
        let mut reader = decoder.read_info().expect("en-tête PNG");
        let mut rgba = vec![0; reader.output_buffer_size().expect("taille PNG")];
        let info = reader.next_frame(&mut rgba).expect("pixels du morceau");
        assert_eq!(info.color_type, png::ColorType::Rgba);
        rgba.truncate(info.buffer_size());
        Self {
            width: info.width as usize,
            height: info.height as usize,
            rgba,
        }
    }

    fn alpha(&self, x: f64, y: f64) -> u8 {
        let (x, y) = (x.floor() as isize, y.floor() as isize);
        if x < 0 || y < 0 || x >= self.width as isize || y >= self.height as isize {
            return 0;
        }
        self.rgba[(y as usize * self.width + x as usize) * 4 + 3]
    }

    /// Milieu du bas du sabot, mesuré sur les pixels réellement peints.
    fn hoof(&self, at: Vec2) -> Point {
        let y = (0..self.height)
            .rev()
            .find(|&y| (0..self.width).any(|x| self.rgba[(y * self.width + x) * 4 + 3] >= 128))
            .expect("un sabot peint");
        let xs: Vec<_> = (0..self.width)
            .filter(|&x| self.rgba[(y * self.width + x) * 4 + 3] >= 128)
            .collect();
        point(at) + ((xs[0] + xs[xs.len() - 1] + 1) as f64 * 0.5, (y + 1) as f64)
    }
}

#[test]
fn the_delivered_solognote_has_covered_joints_and_a_chained_tail() {
    let art = solognote();
    let painted = skeleton(&art);
    let sk = &painted.skeleton;
    let body = role(sk, BoneRole::Body);
    let neck = role(sk, BoneRole::Neck);
    let head = role(sk, BoneRole::Head);
    assert_eq!(sk.bones[neck].parent, Some(body));
    assert_eq!(sk.bones[head].parent, Some(neck));
    for wanted in [BoneRole::Jaw, BoneRole::Lip, BoneRole::Eye] {
        assert_eq!(sk.bones[role(sk, wanted)].parent, Some(head));
    }
    assert_eq!(
        sk.bones[role(sk, BoneRole::Pupil)].parent,
        Some(role(sk, BoneRole::Eye))
    );
    assert_eq!(
        sk.bones
            .iter()
            .filter(|bone| bone.role == Some(BoneRole::Ear))
            .count(),
        2
    );
    let tail: Vec<_> = sk
        .bones
        .iter()
        .enumerate()
        .filter(|(_, bone)| bone.role == Some(BoneRole::Tail))
        .map(|(i, _)| i)
        .collect();
    assert_eq!(tail.len(), 3, "la queue entière doit pouvoir onduler");
    assert_eq!(sk.bones[tail[0]].parent, Some(body));
    for pair in tail.windows(2) {
        assert_eq!(sk.bones[pair[1]].parent, Some(pair[0]));
    }
    let locks: Vec<_> = sk
        .bones
        .iter()
        .filter(|bone| bone.role == Some(BoneRole::Sway))
        .collect();
    assert_eq!(locks.len(), 3);
    assert!(locks.iter().all(|bone| bone.parent == Some(body)));

    for front in [false, true] {
        for near in [false, true] {
            let upper = role(sk, BoneRole::Leg { front, near });
            let lower = role(sk, BoneRole::LegLower { front, near });
            assert_eq!(sk.bones[upper].parent, Some(body));
            assert_eq!(sk.bones[lower].parent, Some(upper));
            let position = |bone| {
                sk.pieces
                    .iter()
                    .position(|p| p.bone == bone)
                    .expect("morceau")
            };
            assert!(
                position(lower) < position(upper),
                "le haut doit recouvrir le bas"
            );
            let center = point(sk.bones[lower].at);
            // Une zone commune opaque, centrée sur le pivot, évite le jour
            // à l'articulation ; le rayon est rapporté à la taille de l'animal.
            let radius = painted.bounds.height() * 0.005;
            for bone in [upper, lower] {
                let piece = &sk.pieces[position(bone)];
                let image = Image::of(piece);
                for (dx, dy) in [(0., 0.), (1., 0.), (-1., 0.), (0., 1.), (0., -1.)] {
                    let p = center + (dx * radius, dy * radius) - point(piece.at).to_vec2();
                    assert!(
                        image.alpha(p.x, p.y) > 240,
                        "{} : pivot découvert à {p:?}",
                        sk.bones[bone].name
                    );
                }
            }
        }
    }
    assert!(
        sk.pieces
            .iter()
            .all(|piece| !piece.image.contains("toupet"))
    );
    assert!(
        painted.byte_size() < 2 << 20,
        "morceaux trop lourds après réduction"
    );
}

#[test]
fn solognote_stands_on_her_hooves_grazes_and_keeps_horizontal_pupils() {
    let art = solognote();
    let painted = skeleton(&art);
    let sk = &painted.skeleton;
    let h = painted.bounds.height();
    let ground = point(sk.ground);
    let head = role(sk, BoneRole::Head);
    let pupil = role(sk, BoneRole::Pupil);
    let muzzle = point(sk.muzzle.expect("un museau explicite"));
    let stride = f64::from(art.stride) * h / f64::from(art.height);
    for facing in [-1., 1.] {
        let standing = painted.pose(&PoseSignals::default(), facing, stride);
        for front in [false, true] {
            for near in [false, true] {
                let lower = role(sk, BoneRole::LegLower { front, near });
                let piece = sk
                    .pieces
                    .iter()
                    .find(|piece| piece.bone == lower)
                    .expect("bas peint");
                let hoof = Image::of(piece).hoof(piece.at);
                let posed = standing.root * standing.bones[lower] * hoof;
                assert!(
                    (posed.y - ground.y).abs() < h * 0.01,
                    "{} : sabot {posed:?}, sol {ground:?}",
                    sk.bones[lower].name
                );
            }
        }
        let grazing = painted.pose(
            &PoseSignals {
                graze: 1.,
                ..Default::default()
            },
            facing,
            stride,
        );
        let m = grazing.root * grazing.bones[head] * muzzle;
        assert!(
            (m.y - ground.y).abs() < 0.02 * h,
            "museau {m:?}, sol {ground:?}"
        );
        let fore = point(
            sk.bones[role(
                sk,
                BoneRole::LegLower {
                    front: true,
                    near: true,
                },
            )]
            .at,
        );
        let fore = grazing.root * fore;
        assert!(
            (m.x - fore.x) * f64::from(facing) > 0.,
            "museau derrière les antérieurs"
        );
        let eye = point(sk.bones[pupil].at);
        let transform = grazing.root * grazing.bones[pupil];
        let direction = transform * (eye + (10., 0.)) - transform * eye;
        assert!(
            direction.y.abs() < direction.x.abs() * 0.02,
            "pupille inclinée en broutant : {direction:?}"
        );
    }
}

#[test]
fn solognote_paints_with_images_inside_her_window_in_both_directions() {
    let art = solognote();
    let painted = skeleton(&art);
    let canvas = art.canvas();
    let k = f64::from(art.height) / painted.bounds.height();
    let stride = f64::from(art.stride) / k;
    let mut poses = vec![
        PoseSignals::default(),
        PoseSignals {
            graze: 1.,
            ..Default::default()
        },
        PoseSignals {
            sit: 1.,
            ..Default::default()
        },
        PoseSignals {
            sit: 1.,
            sleep: 1.,
            sleep_low: 1.,
            ..Default::default()
        },
        PoseSignals {
            air: 1.,
            rise: 1.,
            ..Default::default()
        },
        PoseSignals {
            dangle: 1.,
            swing: 0.4,
            ..Default::default()
        },
        PoseSignals {
            jaw: 1.,
            ..Default::default()
        },
    ];
    for phase in 0..8 {
        poses.push(PoseSignals {
            walk: 1.,
            gait_phase: phase as f32 * std::f32::consts::TAU / 8.,
            ..Default::default()
        });
        poses.push(PoseSignals {
            run: 1.,
            gait_phase: phase as f32 * std::f32::consts::TAU / 8.,
            fleece: 0.15,
            ..Default::default()
        });
    }
    for facing in [-1., 1.] {
        for signals in &poses {
            let pose = painted.pose(signals, facing, stride);
            let mut list =
                DisplayList::new((canvas.width * 2.) as u32, (canvas.height * 2.) as u32);
            paint_skeleton(
                painted,
                &pose,
                signals.blink,
                Affine::scale(2.),
                point(canvas.anchor),
                k,
                &mut list,
            );
            assert!(list.commands.len() >= 20);
            for command in &list.commands {
                let Drawing::Image {
                    bitmap, transform, ..
                } = command
                else {
                    panic!("un squelette doit se poser en images seulement");
                };
                let b = transform.transform_rect_bbox(Rect::new(
                    0.,
                    0.,
                    f64::from(bitmap.width),
                    f64::from(bitmap.height),
                ));
                assert!(
                    b.x0 > -4.
                        && b.y0 > -4.
                        && b.x1 < f64::from(list.width) + 4.
                        && b.y1 < f64::from(list.height) + 4.,
                    "morceau hors de la fenêtre : {b:?}, {signals:?}, sens {facing}"
                );
            }
        }
    }
}
