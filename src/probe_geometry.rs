//! CollisionProbe candidate order and sampled cell traversal, independent of game types.

pub type Point = [f64; 3];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    pub position: Point,
    pub normal: [i32; 3],
}

/// The shape callback clips the exact full segment, not a quantized sample point.
/// Sampling discovers cells only; thin shapes and the final partial step stay exact.
pub fn ray(
    start: Point,
    end: Point,
    mut clip: impl FnMut([i32; 3], Point, Point) -> Option<Hit>,
) -> Option<Hit> {
    let delta: Point = std::array::from_fn(|i| end[i] - start[i]);
    let length = delta.iter().map(|v| v * v).sum::<f64>().sqrt();
    if !length.is_finite() || length < 1.0e-7 {
        return None;
    }
    let steps = (length / 0.1).ceil() as usize;
    let mut previous = None;
    let mut best: Option<(f64, Hit)> = None;
    for step in 0..=steps {
        let t = (step as f64 * 0.1 / length).min(1.0);
        let at: Point = if step == steps {
            end
        } else {
            std::array::from_fn(|i| start[i] + delta[i] * t)
        };
        let cell = at.map(|v| v.floor() as i32);
        if previous == Some(cell) {
            continue;
        }
        previous = Some(cell);
        if let Some(hit) = clip(cell, start, end) {
            let distance = (0..3).map(|i| (hit.position[i] - start[i]).powi(2)).sum();
            if best.is_none_or(|(old, _)| distance < old) {
                best = Some((distance, hit));
            }
        }
    }
    best.map(|(_, hit)| hit)
}

pub fn find_collision_target(
    feet: Point,
    height: f64,
    half_width: f64,
    mut clip: impl FnMut(Point, Point) -> Option<Hit>,
    mut collides_at: impl FnMut(Point) -> bool,
) -> Option<Point> {
    let [x, y, z] = feet;
    if let Some(hit) = clip([x, y + 0.01, z], [x, y - 8.0, z]) {
        let target = [x, hit.position[1] - 0.0625, z];
        // Java deliberately selects !level.noCollision(player, movedBox).
        if collides_at(target) {
            return Some(target);
        }
    }
    let head = [x, y + height, z];
    if let Some(hit) = clip(head, [x, head[1] + 8.0, z]) {
        let target = [x, hit.position[1] - height + 0.0625, z];
        if collides_at(target) {
            return Some(target);
        }
    }
    let middle = [x, y + height / 2.0, z];
    for [dx, dz] in [[0, -1], [0, 1], [1, 0], [-1, 0]] {
        let end = [x + f64::from(dx) * 6.0, middle[1], z + f64::from(dz) * 6.0];
        if let Some(hit) = clip(middle, end) {
            let margin = half_width - 0.0625;
            let target = [
                hit.position[0] + f64::from(hit.normal[0]) * margin,
                y,
                hit.position[2] + f64::from(hit.normal[2]) * margin,
            ];
            if collides_at(target) {
                return Some(target);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floor_wins_only_when_moved_box_collides() {
        let target = find_collision_target(
            [0.5, 4.0, 0.5],
            1.8,
            0.3,
            |_, _| {
                Some(Hit {
                    position: [0.5, 1.0, 0.5],
                    normal: [0, 1, 0],
                })
            },
            |p| p == [0.5, 0.9375, 0.5],
        );
        assert_eq!(target, Some([0.5, 0.9375, 0.5]));
    }

    #[test]
    fn rejects_noncolliding_floor_then_uses_ceiling() {
        let mut calls = 0;
        let target = find_collision_target(
            [0.5, 4.0, 0.5],
            1.5,
            0.3,
            |start, end| {
                calls += 1;
                assert_eq!(start[1], if calls == 1 { 4.01 } else { 5.5 });
                assert_eq!(end[1], if calls == 1 { -4.0 } else { 13.5 });
                Some(Hit {
                    position: [0.5, if calls == 1 { 1.0 } else { 7.0 }, 0.5],
                    normal: [0, -1, 0],
                })
            },
            |p| p[1] > 4.0,
        );
        assert_eq!(target, Some([0.5, 5.5625, 0.5]));
        assert_eq!(calls, 2);
    }

    #[test]
    fn walls_follow_north_south_east_west_and_face_normal() {
        for selected in 0..4 {
            let mut calls = 0;
            let target = find_collision_target(
                [0.5, 4.0, 0.5],
                1.8,
                0.3,
                |start, end| {
                    calls += 1;
                    if calls <= 2 {
                        return None;
                    }
                    let [dx, dz] = [[0, -1], [0, 1], [1, 0], [-1, 0]][calls - 3];
                    assert_eq!(start, [0.5, 4.9, 0.5]);
                    assert_eq!(
                        end,
                        [0.5 + f64::from(dx) * 6.0, 4.9, 0.5 + f64::from(dz) * 6.0]
                    );
                    Some(Hit {
                        position: end,
                        normal: [-dx, 0, -dz],
                    })
                },
                |_| false,
            );
            // Exercise acceptance order separately below without sharing mutable borrows.
            assert!(target.is_none());
            assert_eq!(calls, 6);
            let mut accepted = 0;
            let target = find_collision_target(
                [0.5, 4.0, 0.5],
                1.8,
                0.3,
                |start, end| {
                    (start[1] == end[1]).then_some(Hit {
                        position: end,
                        normal: [
                            if start[0] == end[0] {
                                0
                            } else {
                                (start[0] - end[0]).signum() as i32
                            },
                            0,
                            if start[2] == end[2] {
                                0
                            } else {
                                (start[2] - end[2]).signum() as i32
                            },
                        ],
                    })
                },
                |_| {
                    let yes = accepted == selected;
                    accepted += 1;
                    yes
                },
            )
            .unwrap();
            let [dx, dz] = [[0, -1], [0, 1], [1, 0], [-1, 0]][selected];
            assert_eq!(
                target,
                [
                    0.5 + f64::from(dx) * (6.0 - 0.2375),
                    4.0,
                    0.5 + f64::from(dz) * (6.0 - 0.2375)
                ]
            );
        }
    }

    #[test]
    fn traversal_floors_negative_cells_and_includes_exact_endpoint() {
        let mut cells = Vec::new();
        let start = [-0.01, 2.25, 0.5];
        let end = [-6.01, 2.25, 0.5];
        assert!(
            ray(start, end, |cell, a, b| {
                assert_eq!((a, b), (start, end));
                cells.push(cell);
                None
            })
            .is_none()
        );
        assert_eq!(
            cells,
            (-7..=-1).rev().map(|x| [x, 2, 0]).collect::<Vec<_>>()
        );
        let mut cells = Vec::new();
        ray([0.99, 0.5, 0.5], [1.04, 0.5, 0.5], |cell, _, _| {
            cells.push(cell);
            None
        });
        assert_eq!(cells, [[0, 0, 0], [1, 0, 0]]);
    }

    #[test]
    fn ray_returns_exact_nearest_hit_not_sample_position() {
        let expected = Hit {
            position: [0.5, -0.03125, 0.5],
            normal: [0, 1, 0],
        };
        let hit = ray(
            [0.5, 1.5, 0.5],
            [0.5, -6.51, 0.5],
            |cell, _, _| match cell[1] {
                -1 => Some(expected),
                -2 => Some(Hit {
                    position: [0.5, -1.5, 0.5],
                    normal: [0, 1, 0],
                }),
                _ => None,
            },
        );
        assert_eq!(hit, Some(expected));
    }
}
