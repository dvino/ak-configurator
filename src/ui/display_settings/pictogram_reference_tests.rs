use super::*;

const INK: Color32 = Color32::from_rgb(19, 83, 157);

fn capture<R>(paint: impl FnOnce(&mut egui::Ui) -> R) -> (R, Vec<egui::Shape>) {
    let ctx = egui::Context::default();
    let clip = egui::Rect::from_min_max(egui::pos2(-500.0, -500.0), egui::pos2(500.0, 500.0));
    let mut paint = Some(paint);
    let mut result = None;
    let output = ctx.run_ui(egui::RawInput::default(), |ui| {
        ui.set_clip_rect(clip);
        ui.visuals_mut().override_text_color = Some(INK);
        result = Some(paint.take().unwrap()(ui));
    });
    assert!(output.platform_output.commands.is_empty());
    let shapes = output
        .shapes
        .into_iter()
        .filter(|shape| shape.clip_rect == clip)
        .map(|shape| shape.shape)
        .collect();
    (result.unwrap(), shapes)
}

#[test]
fn reference_toolbar_variants_preserve_literal_geometry_and_primitive_counts() {
    for (icon, count) in [
        (PictogramToolbarIcon::Choose, 7),
        (PictogramToolbarIcon::Pencil, 4),
        (PictogramToolbarIcon::Eraser, 6),
        (PictogramToolbarIcon::Clear, 4),
        (PictogramToolbarIcon::Delete, 6),
        (PictogramToolbarIcon::Save, 4),
    ] {
        for (scale, side) in [(1.0_f32, 34.0_f32), (2.0_f32, 68.0_f32)] {
            for dark in [false, true] {
                let (response, shapes) = capture(|ui| {
                    pictogram_toolbar_button(ui, dark, scale, icon, "reference", true, false)
                });
                assert_eq!(response.rect.size(), egui::vec2(side, side));
                assert_eq!(response.sense, Sense::click());
                assert!(!response.clicked());
                assert_eq!(shapes.len(), count);
                let egui::Shape::Rect(frame) = &shapes[0] else {
                    panic!("expected toolbar frame")
                };
                assert_eq!(frame.rect, response.rect);
                assert_eq!(frame.fill, Color32::TRANSPARENT);
                assert_eq!(frame.stroke.width, 1.0_f32);
                assert!(shapes
                    .iter()
                    .skip(1)
                    .all(|shape| response.rect.contains_rect(shape.visual_bounding_rect())));
            }
        }
    }
}

#[test]
fn reference_toolbar_disabled_and_selected_states_preserve_sense_and_ink() {
    for selected in [false, true] {
        let mut enabled_ink = None;
        for enabled in [true, false] {
            let (response, shapes) = capture(|ui| {
                pictogram_toolbar_button(
                    ui,
                    true,
                    1.0_f32,
                    PictogramToolbarIcon::Pencil,
                    "reference",
                    enabled,
                    selected,
                )
            });
            assert_eq!(
                response.sense,
                if enabled {
                    Sense::click()
                } else {
                    Sense::hover()
                }
            );
            assert!(!response.clicked());
            let egui::Shape::Rect(frame) = &shapes[0] else {
                panic!("expected toolbar frame")
            };
            let egui::Shape::LineSegment { points, stroke } = &shapes[1] else {
                panic!("expected pencil line")
            };
            assert_eq!(points[0], response.rect.center() + egui::vec2(-5.5, 5.5));
            assert_eq!(points[1], response.rect.center() + egui::vec2(5.0, -5.0));
            assert_eq!(stroke.width, 1.7_f32);
            if selected {
                assert_eq!(frame.stroke.color, app_accent());
                assert_ne!(frame.fill, Color32::TRANSPARENT);
            } else {
                assert_eq!(frame.fill, Color32::TRANSPARENT);
                if enabled {
                    assert_eq!(stroke.color, INK);
                }
            }
            if enabled {
                enabled_ink = Some(stroke.color);
            } else {
                let previous = enabled_ink.unwrap();
                assert_ne!(stroke.color, previous);
                assert!(stroke.color.a() < previous.a());
            }
        }
    }
}

#[test]
fn reference_assigned_preview_preserves_absent_slash_and_literal_first_pixel_mesh() {
    for dark in [false, true] {
        let (_, absent) = capture(|ui| {
            draw_assigned_pictogram_preview(ui, dark, 1.0_f32, None, [19, 83, 157], "reference");
        });
        assert_eq!(absent.len(), 2);
        let egui::Shape::Rect(frame) = &absent[0] else {
            panic!("expected assigned-preview frame")
        };
        assert_eq!(frame.rect.size(), egui::vec2(34.0, 34.0));
        let egui::Shape::LineSegment { points, stroke } = &absent[1] else {
            panic!("expected absent-preview slash")
        };
        assert_eq!(
            *points,
            [
                frame.rect.min + egui::vec2(10.0, 10.0),
                frame.rect.min + egui::vec2(24.0, 24.0)
            ]
        );
        assert_eq!(stroke.width, 1.5_f32);

        // A literal 35x35 bitmap: only its first (MSB-first) pixel is set.
        let mut bitmap = [0u8; 154];
        bitmap[0] = 0x80;
        let (_, present) = capture(|ui| {
            draw_assigned_pictogram_preview(
                ui,
                dark,
                1.0_f32,
                Some(&bitmap),
                [19, 83, 157],
                "reference",
            );
        });
        assert_eq!(present.len(), 2);
        let egui::Shape::Rect(frame) = &present[0] else {
            panic!("expected assigned-preview frame")
        };
        let egui::Shape::Mesh(mesh) = &present[1] else {
            panic!("expected one-pixel bitmap mesh, not absent-preview slash")
        };
        assert_eq!(mesh.vertices.len(), 4);
        assert_eq!(mesh.indices.len(), 6);
        for relative in [(-0.5, -0.5), (0.5, -0.5), (-0.5, 0.5), (0.5, 0.5)] {
            let expected = frame.rect.min + egui::vec2(relative.0, relative.1);
            assert!(mesh.vertices.iter().any(|vertex| vertex.pos == expected));
        }
        assert!(mesh.vertices.iter().all(|vertex| vertex.color == INK));
    }
}
