mod legacy_reference_tests {
    use super::*;

    const INK: Color32 = Color32::from_rgb(19, 83, 157);

    fn shapes(paint: impl FnOnce(&egui::Ui)) -> Vec<egui::Shape> {
        let ctx = egui::Context::default();
        ctx.set_fonts(egui::FontDefinitions::default());
        let clip = egui::Rect::from_min_max(egui::pos2(-500.0, -500.0), egui::pos2(500.0, 500.0));
        let mut paint = Some(paint);
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            ui.set_clip_rect(clip);
            paint.take().unwrap()(ui);
        });
        output
            .shapes
            .into_iter()
            .filter(|s| s.clip_rect == clip)
            .map(|s| s.shape)
            .collect()
    }

    fn near(actual: egui::Pos2, expected: egui::Pos2) {
        assert!(
            (actual.x - expected.x).abs() < 0.001,
            "{actual:?} != {expected:?}"
        );
        assert!(
            (actual.y - expected.y).abs() < 0.001,
            "{actual:?} != {expected:?}"
        );
    }

    #[test]
    fn chevrons_reflect_and_enforce_minimum_stroke() {
        for (right, sign) in [(true, 1.0), (false, -1.0)] {
            let output = shapes(|ui| paint_chevrons(ui, egui::pos2(100.0, 50.0), 10.0, right, INK));
            let expected = [
                [(96.4, 47.2), (99.2, 50.0)],
                [(99.2, 50.0), (96.4, 52.8)],
                [(100.0, 47.2), (102.8, 50.0)],
                [(102.8, 50.0), (100.0, 52.8)],
            ];
            assert_eq!(output.len(), 4);
            for (shape, endpoints) in output.iter().zip(expected) {
                let egui::Shape::LineSegment { points, stroke } = shape else {
                    panic!("unexpected shape {shape:?}")
                };
                assert_eq!(*stroke, Stroke::new(1.3_f32, INK));
                for (point, (x, y)) in points.iter().zip(endpoints) {
                    near(*point, egui::pos2(100.0 + (x - 100.0) * sign, y));
                }
            }
        }
        let output = shapes(|ui| paint_chevrons(ui, egui::pos2(5.0, 7.0), 2.0, true, INK));
        assert_eq!(output.len(), 4);
        for shape in output {
            let egui::Shape::LineSegment { stroke, .. } = shape else {
                panic!("unexpected shape")
            };
            assert_eq!(stroke, Stroke::new(1.0_f32, INK));
        }
    }

    #[test]
    fn masks_and_segment_geometry() {
        for (digit, mask) in [0x3f, 0x06, 0x5b, 0x4f, 0x66, 0x6d, 0x7d, 0x07, 0x7f, 0x6f]
            .into_iter()
            .enumerate()
        {
            assert_eq!(digit_mask(digit as u8), mask);
        }
        assert_eq!(digit_mask(255), 0x6f);
        let rect = egui::Rect::from_min_max(egui::pos2(10.0, 20.0), egui::pos2(30.0, 60.0));
        let one = shapes(|ui| paint_segment_digit(ui, 1, rect, true, INK, 2.5));
        let expected = [[(34.8, 20.0), (32.4, 40.0)], [(32.4, 40.0), (30.0, 60.0)]];
        assert_eq!(one.len(), 2);
        for (shape, endpoints) in one.iter().zip(expected) {
            let egui::Shape::LineSegment { points, stroke } = shape else {
                panic!("unexpected shape {shape:?}")
            };
            assert_eq!(*stroke, Stroke::new(2.5_f32, INK));
            for (point, (x, y)) in points.iter().zip(endpoints) {
                near(*point, egui::pos2(x, y));
            }
        }
        let eight = shapes(|ui| paint_segment_digit(ui, 8, rect, false, INK, 3.0));
        assert_eq!(eight.len(), 7);
        let egui::Shape::LineSegment { points, stroke } = &eight[6] else {
            panic!("unexpected shape")
        };
        assert_eq!(*stroke, Stroke::new(3.0_f32, INK));
        assert_eq!(*points, [egui::pos2(10.0, 40.0), egui::pos2(30.0, 40.0)]);
        assert!(eight.iter().all(|s| matches!(s, egui::Shape::LineSegment { stroke, .. } if *stroke == Stroke::new(3.0_f32, INK))));
    }

    #[test]
    fn hex_polygons_and_italic_digit() {
        let output = shapes(|ui| {
            paint_hex_segment(
                ui,
                egui::pos2(10.0, 20.0),
                egui::pos2(30.0, 20.0),
                5.0,
                true,
                INK,
            );
            paint_hex_segment(
                ui,
                egui::pos2(10.0, 20.0),
                egui::pos2(10.0, 40.0),
                1.0,
                false,
                INK,
            );
        });
        let expected = [
            [
                (12.5, 17.5),
                (27.5, 17.5),
                (30.0, 20.0),
                (27.5, 22.5),
                (12.5, 22.5),
                (10.0, 20.0),
            ],
            [
                (10.0, 20.0),
                (11.0, 21.0),
                (11.0, 39.0),
                (10.0, 40.0),
                (9.0, 39.0),
                (9.0, 21.0),
            ],
        ];
        assert_eq!(output.len(), 2);
        for (shape, points) in output.iter().zip(expected) {
            let egui::Shape::Path(path) = shape else {
                panic!("unexpected shape {shape:?}")
            };
            assert!(path.closed);
            assert_eq!(path.fill, INK);
            assert!(path.stroke.is_empty());
            assert_eq!(path.points.len(), 6);
            for (actual, (x, y)) in path.points.iter().zip(points) {
                near(*actual, egui::pos2(x, y));
            }
        }
        let rect = egui::Rect::from_min_max(egui::pos2(10.0, 20.0), egui::pos2(50.0, 56.0));
        let digit = shapes(|ui| paint_hex_segment_digit(ui, 1, rect, true, INK));
        assert_eq!(digit.len(), 2);
        let egui::Shape::Path(path) = &digit[0] else {
            panic!("unexpected shape")
        };
        assert_eq!(path.fill, INK);
        near(path.points[0], egui::pos2(52.142857, 23.5));
        near(path.points[1], egui::pos2(54.642857, 26.0));
        assert!(digit
            .iter()
            .all(|s| matches!(s, egui::Shape::Path(p) if p.fill == INK && p.points.len() == 6)));
    }

    #[test]
    fn dot_patterns_positions_radius_and_clamp() {
        let rect = egui::Rect::from_min_max(egui::pos2(10.0, 20.0), egui::pos2(22.0, 50.0));
        for (digit, positions) in [
            (
                0,
                vec![
                    (13., 25.),
                    (16., 25.),
                    (19., 25.),
                    (13., 30.),
                    (19., 30.),
                    (13., 35.),
                    (19., 35.),
                    (13., 40.),
                    (19., 40.),
                    (13., 45.),
                    (16., 45.),
                    (19., 45.),
                ],
            ),
            (
                1,
                vec![
                    (16., 25.),
                    (13., 30.),
                    (16., 30.),
                    (16., 35.),
                    (16., 40.),
                    (13., 45.),
                    (16., 45.),
                    (19., 45.),
                ],
            ),
        ] {
            let output = shapes(|ui| paint_dot_digit(ui, digit, rect, INK));
            assert_eq!(output.len(), positions.len());
            for (shape, (x, y)) in output.iter().zip(positions) {
                let egui::Shape::Circle(circle) = shape else {
                    panic!("unexpected shape {shape:?}")
                };
                near(circle.center, egui::pos2(x, y));
                assert!((circle.radius - 4.0 / 3.0).abs() < 0.001);
                assert_eq!(circle.fill, INK);
                assert_eq!(circle.stroke, Stroke::NONE);
            }
        }
        let small = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(4.0, 6.0));
        let nine = shapes(|ui| paint_dot_digit(ui, 9, small, INK));
        let clamped = shapes(|ui| paint_dot_digit(ui, 255, small, INK));
        assert_eq!(nine, clamped);
        assert!(nine
            .iter()
            .all(|s| matches!(s, egui::Shape::Circle(c) if c.radius == 1.0 && c.fill == INK)));
    }

    #[test]
    fn clock_shapes_colon_alignment_and_clamps() {
        let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(240.0, 280.0));
        for (style, count, colon_index) in [(1, 16, 7), (2, 16, 7), (5, 39, 19), (6, 32, 14)] {
            let render = |visible, center_y| {
                shapes(|ui| {
                    paint_clock(
                        ui,
                        screen,
                        style,
                        0,
                        0,
                        INK,
                        Color32::BLACK,
                        visible,
                        [1, 2, 3, 4],
                        center_y,
                    )
                })
            };
            let off = render(false, 0.0);
            let mut on = render(true, 999.0);
            assert_eq!(off.len(), count);
            assert_eq!(on.len(), count + 2);
            for (index, y) in [(colon_index, 63.333333), (colon_index + 1, 72.666667)] {
                let egui::Shape::Circle(circle) = &on[index] else {
                    panic!("unexpected colon")
                };
                near(circle.center, egui::pos2(46.64, y));
                assert!((circle.radius - 28.0 / 18.0).abs() < 0.001);
                assert_eq!(circle.fill, INK);
            }
            on.drain(colon_index..colon_index + 2);
            assert_eq!(on, off);
            for shape in &off {
                match (style, shape) {
                    (1 | 2, egui::Shape::Path(p)) => assert_eq!(p.fill, INK),
                    (5, egui::Shape::Circle(c)) => assert_eq!(c.fill, INK),
                    (6, egui::Shape::LineSegment { stroke, .. }) => {
                        assert!(stroke.color == INK || stroke.color == INK.gamma_multiply(0.25))
                    }
                    _ => panic!("unexpected clock shape {shape:?}"),
                }
            }
            if style == 5 {
                let egui::Shape::Circle(circle) = &off[0] else {
                    panic!("unexpected dot")
                };
                near(circle.center, egui::pos2(15.28, 58.666667));
            }
        }
        let render = |size, alignment| {
            shapes(|ui| {
                paint_clock(
                    ui,
                    screen,
                    5,
                    size,
                    alignment,
                    INK,
                    Color32::BLACK,
                    false,
                    [1, 2, 3, 4],
                    0.0,
                )
            })
        };
        assert_eq!(render(3, 2), render(255, 255));
        let left = render(0, 0);
        let right = render(0, 2);
        let (egui::Shape::Circle(l), egui::Shape::Circle(r)) = (&left[0], &right[0]) else {
            panic!("unexpected dots")
        };
        near(r.center, l.center + egui::vec2(146.72, 0.0));
        let half = egui::Rect::from_min_size(egui::pos2(20.0, 30.0), egui::vec2(120.0, 140.0));
        let scaled = shapes(|ui| {
            paint_clock(
                ui,
                half,
                5,
                0,
                0,
                INK,
                Color32::BLACK,
                false,
                [1, 2, 3, 4],
                0.0,
            )
        });
        let egui::Shape::Circle(first) = &scaled[0] else {
            panic!("unexpected dot")
        };
        near(first.center, egui::pos2(27.64, 59.333333));
        assert_eq!(first.radius, 1.0);
    }

    #[test]
    fn clock_font_text_and_colon_layout() {
        let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(240.0, 280.0));
        let render = |visible| {
            shapes(|ui| {
                paint_clock(
                    ui,
                    screen,
                    0,
                    0,
                    0,
                    INK,
                    Color32::BLACK,
                    visible,
                    [1, 2, 3, 4],
                    0.0,
                )
            })
        };
        let off = render(false);
        let on = render(true);
        assert_eq!(off.len(), 2);
        assert_eq!(on.len(), 3);
        let (egui::Shape::Text(left), egui::Shape::Text(colon), egui::Shape::Text(right)) =
            (&on[0], &on[1], &on[2])
        else {
            panic!("unexpected font shapes")
        };
        assert_eq!(left.galley.job.text, "12");
        assert_eq!(colon.galley.job.text, ":");
        assert_eq!(right.galley.job.text, "34");
        for text in [left, colon, right] {
            assert_eq!(text.fallback_color, INK);
        }
        assert!((left.pos.x - 8.0).abs() < 0.001);
        assert!((colon.pos.x - (left.pos.x + left.galley.size().x)).abs() < 0.001);
        assert!((right.pos.x - (colon.pos.x + colon.galley.size().x)).abs() < 0.001);
        assert_eq!(off[0], on[0]);
        assert_eq!(off[1], on[2]);
    }
}
