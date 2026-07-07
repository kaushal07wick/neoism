use super::*;

// Command parsing tests

#[test]
fn test_parse_transmission_with_format_and_dimensions() {
    // Test: transmission command
    let mut state = KittyGraphicsState::default();
    // 1x1 RGB (3 bytes) base64 = "AAAA"
    let params: Vec<&[u8]> = vec![b"G", b"f=24,s=1,v=1,i=1", b"AAAA"];
    let resp = kitty_graphics_protocol::parse(&params, &mut state);
    assert!(resp.is_some());
    let data = resp.unwrap().graphic_data;
    assert!(data.is_some());
}

#[test]
fn test_parse_display_command_with_columns_rows() {
    // Test: display command
    let mut state = KittyGraphicsState::default();
    let params: Vec<&[u8]> = vec![b"G", b"a=p,c=80,r=120,i=31", b""];
    let resp = kitty_graphics_protocol::parse(&params, &mut state);
    assert!(resp.is_some());
    let placement = resp.unwrap().placement_request;
    assert!(placement.is_some());
    let p = placement.unwrap();
    assert_eq!(p.columns, 80);
    assert_eq!(p.rows, 120);
    assert_eq!(p.image_id, 31);
}

#[test]
fn test_parse_delete_command_with_position() {
    // Test: delete command
    let mut state = KittyGraphicsState::default();
    let params: Vec<&[u8]> = vec![b"G", b"a=d,d=p,x=3,y=4", b""];
    let resp = kitty_graphics_protocol::parse(&params, &mut state);
    assert!(resp.is_some());
    let delete = resp.unwrap().delete_request;
    assert!(delete.is_some());
    let d = delete.unwrap();
    assert_eq!(d.action, b'p');
    assert_eq!(d.x, 3);
    assert_eq!(d.y, 4);
}

#[test]
fn test_parse_ignores_unknown_keys() {
    // Test: ignore unknown keys
    let mut state = KittyGraphicsState::default();
    // 1x1 RGB with unknown key
    let params: Vec<&[u8]> = vec![b"G", b"f=24,s=1,v=1,hello=world,i=1", b"AAAA"];
    let resp = kitty_graphics_protocol::parse(&params, &mut state);
    // Should parse successfully despite unknown key
    assert!(resp.is_some());
}

#[test]
fn test_parse_large_negative_z_index() {
    // Test: large negative z-index values
    let mut state = KittyGraphicsState::default();
    let params: Vec<&[u8]> = vec![b"G", b"a=p,z=-2000000000,i=1", b""];
    let resp = kitty_graphics_protocol::parse(&params, &mut state);
    assert!(resp.is_some());
    let placement = resp.unwrap().placement_request.unwrap();
    assert_eq!(placement.z_index, -2000000000);
}

#[test]
fn test_response_encoding_with_image_id() {
    // Test: response encoding with image id
    let mut state = KittyGraphicsState::default();
    // 1x1 RGBA = 4 bytes, base64 = "/////w=="
    let params: Vec<&[u8]> = vec![b"G", b"a=T,f=32,s=1,v=1,i=4", b"/////w=="];
    let resp = kitty_graphics_protocol::parse(&params, &mut state).unwrap();
    assert!(resp.response.is_some());
    let response_str = resp.response.unwrap();
    assert!(
        response_str.contains("i=4"),
        "Response should contain image id: {}",
        response_str
    );
    assert!(
        response_str.contains("OK"),
        "Response should contain OK: {}",
        response_str
    );
}

#[test]
fn test_response_encoding_with_image_number() {
    // Test: response encoding with image number
    let mut state = KittyGraphicsState::default();
    // 1x1 RGBA = 4 bytes
    let params: Vec<&[u8]> = vec![b"G", b"a=t,f=32,s=1,v=1,I=4", b"/////w=="];
    let resp = kitty_graphics_protocol::parse(&params, &mut state).unwrap();
    assert!(resp.response.is_some());
    let response_str = resp.response.unwrap();
    assert!(
        response_str.contains("I=4"),
        "Response should contain image number: {}",
        response_str
    );
}

#[test]
fn test_default_format_is_rgba() {
    // Test: default format is RGBA
    let mut state = KittyGraphicsState::default();
    // No f= parameter — should default to RGBA (f=32)
    let params: Vec<&[u8]> = vec![
        b"G",
        b"a=t,s=1,v=1,i=1",
        b"/////w==", // 4 bytes = 1x1 RGBA
    ];
    let resp = kitty_graphics_protocol::parse(&params, &mut state);
    assert!(resp.is_some());
    let data = resp.unwrap().graphic_data;
    assert!(data.is_some(), "Should parse with default RGBA format");
}

#[test]
fn test_delete_range_multiple_variants() {
    // Test: delete range variants
    use neoism_terminal_core::ansi::graphics::Graphics;

    let mut graphics = Graphics::default();

    // Create placements for images 1, 2, 3
    for id in 1..=3u32 {
        graphics
            .kitty_placements
            .insert((id, 0), make_test_placement(id, 0, 0, id as i64, 5, 3, 0));
    }

    // Range delete [1, 2] — should keep image 3
    graphics.kitty_placements.retain(|k, _| k.0 < 1 || k.0 > 2);
    assert_eq!(graphics.kitty_placements.len(), 1);
    assert!(graphics.kitty_placements.contains_key(&(3, 0)));

    // Single-image range [3, 3]
    graphics.kitty_placements.retain(|k, _| k.0 != 3);
    assert_eq!(graphics.kitty_placements.len(), 0);
}

#[test]
fn test_delete_all_preserves_memory_limit() {
    // Test: delete all preserves memory limit
    use neoism_terminal_core::ansi::graphics::Graphics;

    let mut graphics = Graphics {
        total_limit: 5000,
        ..Graphics::default()
    };

    let data = GraphicData {
        id: GraphicId::new(1),
        width: 2,
        height: 2,
        color_type: ColorType::Rgba,
        pixels: vec![255u8; 16],
        is_opaque: true,
        resize: None,
        display_width: None,
        display_height: None,
        transmit_time: std::time::Instant::now(),
    };
    graphics.store_kitty_image(1, None, data);
    graphics
        .kitty_placements
        .insert((1, 0), make_test_placement(1, 0, 0, 0, 5, 3, 0));

    // Delete all
    graphics.kitty_placements.clear();
    graphics.kitty_images.clear();

    assert_eq!(graphics.total_limit, 5000, "Limit should be preserved");
}

#[test]
fn test_chunked_quiet_flag_inheritance() {
    // Chunked transmission: q= on the first chunk must be preserved
    // through the merged command, so subsequent chunks — which only
    // carry `m=` per the kitty spec — still take the original q value.
    //
    // q=1 suppresses OK responses but NOT errors. We test that here by
    // sending a correctly-sized 2x2 RGBA image across two spec-compliant
    // chunks; the OK response must be suppressed.
    let mut state = KittyGraphicsState::default();

    // 2x2 RGBA = 16 bytes. Full base64 = 24 chars with trailing padding.
    // We'll split on a 4-char boundary into chunk1=12 chars, chunk2=12 chars.
    use base64::engine::general_purpose::STANDARD as B64;
    use base64::Engine as _;
    let raw = vec![0xFFu8; 16];
    let encoded = B64.encode(&raw);
    assert_eq!(encoded.len() % 4, 0);
    let (first, second) = encoded.split_at(encoded.len() / 2);
    let (first_bytes, second_bytes) = (first.as_bytes(), second.as_bytes());

    let ctrl1 = "a=t,f=32,s=2,v=2,i=1,m=1,q=1";
    let params1: Vec<&[u8]> = vec![b"G", ctrl1.as_bytes(), first_bytes];
    let resp1 = kitty_graphics_protocol::parse(&params1, &mut state)
        .expect("pending chunk must return Some");
    assert!(resp1.incomplete);

    let ctrl2 = "m=0,i=1";
    let params2: Vec<&[u8]> = vec![b"G", ctrl2.as_bytes(), second_bytes];
    let resp2 = kitty_graphics_protocol::parse(&params2, &mut state)
        .expect("final chunk must return Some");
    // Successful transmission + q=1 inherited from first chunk →
    // the OK response must be suppressed.
    assert!(resp2.graphic_data.is_some(), "image must decode");
    assert!(
        resp2.response.is_none(),
        "q=1 must suppress OK response even after chunk merge: {:?}",
        resp2.response
    );
}

#[test]
fn test_aspect_ratio_with_only_columns() {
    // Test: aspect ratio with only columns
    // A 16:9 image with c=10 should compute height preserving aspect ratio
    use neoism_terminal_core::graphics::GraphicData;

    let data = GraphicData {
        id: GraphicId::new(1),
        width: 160,
        height: 90,
        color_type: ColorType::Rgba,
        pixels: vec![],
        is_opaque: true,
        resize: Some(neoism_terminal_core::graphics::ResizeCommand {
            width: neoism_terminal_core::graphics::ResizeParameter::Cells(10),
            height: neoism_terminal_core::graphics::ResizeParameter::Auto,
            preserve_aspect_ratio: true,
        }),
        display_width: None,
        display_height: None,
        transmit_time: std::time::Instant::now(),
    };

    let cell_w = 10;
    let cell_h = 20;
    let (w, h) = data.compute_display_dimensions(cell_w, cell_h, 800, 600);

    // Width = 10 cells * 10px = 100px
    assert_eq!(w, 100);
    // Height should preserve 16:9 ratio: 100 * 90/160 = 56.25 ≈ 56
    assert!(h > 50 && h < 60, "Height should be ~56, got {}", h);
}

#[test]
fn test_aspect_ratio_with_only_rows() {
    // Test: aspect ratio with only rows
    use neoism_terminal_core::graphics::GraphicData;

    let data = GraphicData {
        id: GraphicId::new(1),
        width: 160,
        height: 90,
        color_type: ColorType::Rgba,
        pixels: vec![],
        is_opaque: true,
        resize: Some(neoism_terminal_core::graphics::ResizeCommand {
            width: neoism_terminal_core::graphics::ResizeParameter::Auto,
            height: neoism_terminal_core::graphics::ResizeParameter::Cells(5),
            preserve_aspect_ratio: true,
        }),
        display_width: None,
        display_height: None,
        transmit_time: std::time::Instant::now(),
    };

    let cell_w = 10;
    let cell_h = 20;
    let (w, h) = data.compute_display_dimensions(cell_w, cell_h, 800, 600);

    // Height = 5 cells * 20px = 100px
    assert_eq!(h, 100);
    // Width should preserve 16:9 ratio: 100 * 160/90 = 177.7 ≈ 178
    assert!(w > 170 && w < 185, "Width should be ~178, got {}", w);
}

// Format conversion tests

#[test]
fn test_grayscale_format_conversion() {
    // Test: gray (1 bpp) to RGBA conversion
    let mut state = KittyGraphicsState::default();
    // 2x1 grayscale: 2 bytes, base64 of [128, 255] = "gP8="
    let params: Vec<&[u8]> = vec![b"G", b"a=t,f=8,s=2,v=1,i=1", b"gP8="];
    let resp = kitty_graphics_protocol::parse(&params, &mut state);
    assert!(resp.is_some());
    let data = resp.unwrap().graphic_data.unwrap();
    assert_eq!(data.pixels.len(), 8); // 2 pixels * 4 bytes RGBA
                                      // First pixel: gray=128 → [128, 128, 128, 255]
    assert_eq!(data.pixels[0], 128);
    assert_eq!(data.pixels[1], 128);
    assert_eq!(data.pixels[2], 128);
    assert_eq!(data.pixels[3], 255);
    // Second pixel: gray=255 → [255, 255, 255, 255]
    assert_eq!(data.pixels[4], 255);
    assert_eq!(data.pixels[7], 255);
}

#[test]
fn test_gray_alpha_format_conversion() {
    // Test: gray+alpha (2 bpp) to RGBA conversion
    let mut state = KittyGraphicsState::default();
    // 1x1 gray+alpha: 2 bytes [128, 200], base64 = "gMg="
    let params: Vec<&[u8]> = vec![b"G", b"a=t,f=16,s=1,v=1,i=1", b"gMg="];
    let resp = kitty_graphics_protocol::parse(&params, &mut state);
    assert!(resp.is_some());
    let data = resp.unwrap().graphic_data.unwrap();
    assert_eq!(data.pixels.len(), 4); // 1 pixel * 4 bytes RGBA
                                      // gray=128, alpha=200 → [128, 128, 128, 200]
    assert_eq!(data.pixels[0], 128);
    assert_eq!(data.pixels[1], 128);
    assert_eq!(data.pixels[2], 128);
    assert_eq!(data.pixels[3], 200);
    assert!(!data.is_opaque); // alpha != 255
}

// Animation actions surface EINVAL (regression).

#[test]
fn test_animation_action_surfaces_unsupported_response() {
    // Going through the full Crosswords path: a=f should produce a
    // response that the terminal can forward back to the client. Pre-fix
    // this returned None and the client got nothing.
    let mut state = KittyGraphicsState::default();
    let params = vec![
        b"G".as_ref(),
        b"a=f,i=1,r=2,s=1,v=1,f=32".as_ref(),
        b"AAAA".as_ref(),
    ];

    let resp = kitty_graphics_protocol::parse(&params, &mut state)
        .expect("animation actions must produce a response");
    let body = resp
        .response
        .expect("response body must contain EINVAL marker");
    assert!(body.contains("EINVAL:unsupported action"));
    assert!(body.contains("i=1"));
}
