//! Uses the pinned prebuilt horizon_core, without compiling Horizon.
#[path = "../src/probe_geometry.rs"]
mod probe_geometry;

use horizon_core::{
    geometry::{Aabb, Vec3, VoxelShape},
    position::BlockPos,
};
use probe_geometry::{Hit, Point};
use std::collections::HashMap;

#[derive(Default)]
struct Scene(HashMap<[i32; 3], Vec<Aabb>>);

fn vec(p: Point) -> Vec3 {
    Vec3::new(p[0], p[1], p[2])
}

impl Scene {
    fn add(&mut self, cell: [i32; 3], bounds: [f64; 6]) {
        self.0.entry(cell).or_default().push(Aabb::new(
            Vec3::new(bounds[0], bounds[1], bounds[2]),
            Vec3::new(bounds[3], bounds[4], bounds[5]),
        ));
    }

    fn ray(&self, start: Point, end: Point) -> Option<Hit> {
        probe_geometry::ray(start, end, |cell, a, b| {
            let boxes = self.0.get(&cell)?;
            let hit = VoxelShape::clip_boxes(
                boxes.iter().copied(),
                vec(a),
                vec(b),
                BlockPos::new(cell[0], cell[1], cell[2]),
            )?;
            Some(Hit {
                position: hit.location.components(),
                normal: hit.direction.normal(),
            })
        })
    }

    fn target(&self, feet: Point, width: f32, height: f32) -> Option<Point> {
        let at = vec(feet);
        let half = f64::from(width / 2.0);
        let bounds = Aabb::new(
            Vec3::new(at.x - half, at.y, at.z - half),
            Vec3::new(at.x + half, at.y + f64::from(height), at.z + half),
        );
        probe_geometry::find_collision_target(
            feet,
            f64::from(height),
            half,
            |a, b| self.ray(a, b),
            |target| {
                let moved = bounds.move_by(vec(target).subtract(at));
                self.0.iter().any(|(cell, boxes)| {
                    let offset =
                        Vec3::new(f64::from(cell[0]), f64::from(cell[1]), f64::from(cell[2]));
                    boxes.iter().any(|b| b.move_by(offset).intersects(moved))
                })
            },
        )
    }
}

#[test]
fn thin_floor_between_samples_has_exact_top_and_overlaps_target_box() {
    let mut scene = Scene::default();
    scene.add([0, 0, 0], [0.0, 0.0, 0.0, 1.0, 0.03125, 1.0]);
    let hit = scene.ray([0.5, 3.01, 0.5], [0.5, -5.0, 0.5]).unwrap();
    assert_eq!(hit.position, [0.5, 0.03125, 0.5]);
    assert_eq!(hit.normal, [0, 1, 0]);
    assert_eq!(
        scene.target([0.5, 3.0, 0.5], 0.6, 1.8),
        Some([0.5, -0.03125, 0.5])
    );
}

#[test]
fn ceiling_uses_actual_pose_height_and_downward_face() {
    let mut scene = Scene::default();
    scene.add([0, 5, 0], [0.0, 0.5, 0.0, 1.0, 1.0, 1.0]);
    let height = f64::from(1.5_f32);
    let hit = scene
        .ray([0.5, 3.0 + height, 0.5], [0.5, 11.0 + height, 0.5])
        .unwrap();
    assert_eq!(hit.position, [0.5, 5.5, 0.5]);
    assert_eq!(hit.normal, [0, -1, 0]);
    assert_eq!(
        scene.target([0.5, 3.0, 0.5], 0.6, 1.5),
        Some([0.5, 4.0625, 0.5])
    );
}

#[test]
fn every_wall_normal_moves_the_player_into_the_shape_by_one_sixteenth() {
    let half = f64::from(0.6_f32 / 2.0);
    for (cell, point, normal) in [
        ([0, 3, -3], [0.5, 3.9, -2.0], [0, 0, 1]),
        ([0, 3, 3], [0.5, 3.9, 3.0], [0, 0, -1]),
        ([3, 3, 0], [3.0, 3.9, 0.5], [-1, 0, 0]),
        ([-3, 3, 0], [-2.0, 3.9, 0.5], [1, 0, 0]),
    ] {
        let mut scene = Scene::default();
        scene.add(cell, [0.0, 0.0, 0.0, 1.0, 1.0, 1.0]);
        let target = scene.target([0.5, 3.0, 0.5], 0.6, 1.8).unwrap();
        assert_eq!(
            target,
            [
                point[0] + f64::from(normal[0]) * (half - 0.0625),
                3.0,
                point[2] + f64::from(normal[2]) * (half - 0.0625),
            ]
        );
    }
}

#[test]
fn six_direction_priority_is_floor_ceiling_north_south_east_west() {
    let mut scene = Scene::default();
    for cell in [
        [0, 0, 0],
        [0, 7, 0],
        [0, 4, -3],
        [0, 4, 3],
        [3, 4, 0],
        [-3, 4, 0],
    ] {
        scene.add(cell, [0.0, 0.0, 0.0, 1.0, 1.0, 1.0]);
    }
    let feet = [0.5, 3.5, 0.5];
    let half = f64::from(0.6_f32 / 2.0);
    assert_eq!(scene.target(feet, 0.6, 1.8).unwrap()[1], 0.9375);
    scene.0.remove(&[0, 0, 0]);
    assert_eq!(
        scene.target(feet, 0.6, 1.8).unwrap()[1],
        7.0 - f64::from(1.8_f32) + 0.0625
    );
    scene.0.remove(&[0, 7, 0]);
    assert_eq!(
        scene.target(feet, 0.6, 1.8).unwrap()[2],
        -2.0 + half - 0.0625
    );
    scene.0.remove(&[0, 4, -3]);
    assert_eq!(
        scene.target(feet, 0.6, 1.8).unwrap()[2],
        3.0 - half + 0.0625
    );
    scene.0.remove(&[0, 4, 3]);
    assert_eq!(
        scene.target(feet, 0.6, 1.8).unwrap()[0],
        3.0 - half + 0.0625
    );
    scene.0.remove(&[3, 4, 0]);
    assert_eq!(
        scene.target(feet, 0.6, 1.8).unwrap()[0],
        -2.0 + half - 0.0625
    );
    scene.0.remove(&[-3, 4, 0]);
    assert_eq!(scene.target(feet, 0.6, 1.8), None);
}

#[test]
fn multi_box_shape_selects_nearest_surface_and_empty_scene_has_no_probe() {
    let mut scene = Scene::default();
    scene.add([0, 0, 0], [0.0, 0.0, 0.0, 1.0, 0.25, 1.0]);
    scene.add([0, 0, 0], [0.0, 0.5, 0.0, 1.0, 0.75, 1.0]);
    assert_eq!(
        scene
            .ray([0.5, 3.01, 0.5], [0.5, -5.0, 0.5])
            .unwrap()
            .position[1],
        0.75
    );
    assert_eq!(Scene::default().target([0.5, 3.0, 0.5], 0.6, 1.8), None);
}

#[test]
fn inside_shape_preserves_java_full_ray_nudge_and_opposite_normal() {
    let mut scene = Scene::default();
    scene.add([0, 0, 0], [0.0, 0.0, 0.0, 1.0, 1.0, 1.0]);
    let hit = scene.ray([0.5, 0.5, 0.5], [0.5, -7.51, 0.5]).unwrap();
    assert_eq!(hit.position[1], 0.5 - 8.01 * 0.001);
    assert_eq!(hit.normal, [0, 1, 0]);
}
