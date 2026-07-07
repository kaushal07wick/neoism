use super::*;

// Integration Tests

#[test]
fn test_direct_parse_transmit() {
    let mut handler = TestHandler::default();
    let mut state = KittyGraphicsState::default();

    // Parse kitty graphics directly through the protocol parser
    // 1x1 RGBA pixel (4 bytes) - base64 encoded [255, 0, 0, 255] (red pixel)
    let params = vec![
        b"G".as_ref(),
        b"a=t,f=32,s=1,v=1,i=1".as_ref(),
        b"/wAA/w==".as_ref(),
    ];

    if let Some(response) = kitty_graphics_protocol::parse(&params, &mut state) {
        if let Some(graphic_data) = response.graphic_data {
            handler.insert_graphic(graphic_data, None, Some(0));
        }
    }

    // Verify graphic was captured
    assert_eq!(handler.graphics.len(), 1, "Should capture one graphic");

    let graphic = &handler.graphics[0];
    assert_eq!(graphic.width, 1);
    assert_eq!(graphic.height, 1);
    assert_eq!(graphic.pixels.len(), 4); // 1x1x4 bytes (RGBA)
    assert_eq!(graphic.id.get(), 1);
}

#[test]
fn test_parse_png_format() {
    let mut handler = TestHandler::default();
    let mut state = KittyGraphicsState::default();

    // 1x1 red PNG image, base64 encoded
    // This is a complete, valid PNG file
    let png_base64 = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8/5+hHgAHggJ/PchI7wAAAABJRU5ErkJggg==";

    // Parse with f=100 (PNG format)
    let params = vec![
        b"G".as_ref(),
        b"a=t,f=100,i=2".as_ref(),
        png_base64.as_bytes(),
    ];

    if let Some(response) = kitty_graphics_protocol::parse(&params, &mut state) {
        if let Some(graphic_data) = response.graphic_data {
            handler.insert_graphic(graphic_data, None, Some(0));
        }
    }

    // Verify PNG was decoded and captured
    assert_eq!(handler.graphics.len(), 1, "Should capture one PNG graphic");

    let graphic = &handler.graphics[0];
    assert_eq!(graphic.width, 1, "PNG should be decoded to 1x1");
    assert_eq!(graphic.height, 1, "PNG should be decoded to 1x1");
    assert_eq!(graphic.id.get(), 2);
    // PNG should be decoded to RGBA pixels
    assert!(
        graphic.pixels.len() >= 4,
        "PNG should decode to at least 4 bytes (RGBA)"
    );
}

#[test]
fn test_png_transmit_and_display() {
    let mut term: Crosswords = Crosswords::new(
        neoism_terminal_core::crosswords::CrosswordsSize::new(80, 24),
        neoism_terminal_core::ansi::CursorShape::Block,
        neoism_terminal_core::TerminalId::new(0),
        10_000,
    );

    // Set proper cell dimensions
    term.graphics.cell_width = 10.0;
    term.graphics.cell_height = 20.0;

    // 1x1 red PNG image
    let png_base64 = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8/5+hHgAHggJ/PchI7wAAAABJRU5ErkJggg==";

    // Test a=T (transmit and display) with PNG format
    let params = vec![
        b"G".as_ref(),
        b"a=T,f=100,r=1,C=0,i=10".as_ref(),
        png_base64.as_bytes(),
    ];

    let mut state = KittyGraphicsState::default();
    if let Some(response) = kitty_graphics_protocol::parse(&params, &mut state) {
        if let Some(graphic_data) = response.graphic_data {
            if let Some(placement) = response.placement_request {
                // Store and place the graphic
                term.store_graphic(graphic_data.clone());
                term.place_graphic(placement);
            } else {
                // Direct display without placement request
                term.insert_graphic(graphic_data, None, Some(0));
            }
        }
    }

    let final_row = term.grid.cursor.pos.row.0;

    // For 1-row PNG, cursor should stay on row 0 (last row of image)
    assert_eq!(
        final_row, 0,
        "PNG with r=1 should place cursor on row 0, got row {}",
        final_row
    );
}

#[test]
fn test_png_format_support() {
    let mut handler = TestHandler::default();
    let mut state = KittyGraphicsState::default();

    // Test f=100 (PNG format) with a 1x1 PNG
    let png_base64 = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8/5+hHgAHggJ/PchI7wAAAABJRU5ErkJggg==";

    let params = vec![
        b"G".as_ref(),
        b"a=t,f=100,i=100".as_ref(),
        png_base64.as_bytes(),
    ];

    if let Some(response) = kitty_graphics_protocol::parse(&params, &mut state) {
        if let Some(graphic_data) = response.graphic_data {
            handler.insert_graphic(graphic_data, None, Some(0));

            let graphic = &handler.graphics[0];
            assert_eq!(graphic.width, 1, "PNG should decode to 1x1");
            assert_eq!(graphic.height, 1, "PNG should decode to 1x1");
            assert_eq!(graphic.id.get(), 100);
        } else {
            panic!("PNG failed to decode");
        }
    } else {
        panic!("PNG failed to parse");
    }
}

#[test]
fn test_placement_request() {
    let mut handler = TestHandler::default();
    let mut state = KittyGraphicsState::default();

    // Parse placement request (a=p is Put action, x and y are source coordinates)
    let params = vec![b"G".as_ref(), b"a=p,i=1,x=5,y=10,c=3,r=2".as_ref()];

    if let Some(response) = kitty_graphics_protocol::parse(&params, &mut state) {
        if let Some(placement) = response.placement_request {
            handler.place_graphic(placement);
        }
    }

    // Verify placement was captured
    assert_eq!(handler.placements.len(), 1, "Should capture one placement");

    let placement = &handler.placements[0];
    assert_eq!(placement.image_id, 1);
    assert_eq!(placement.x, 5);
    assert_eq!(placement.y, 10);
    assert_eq!(placement.columns, 3);
    assert_eq!(placement.rows, 2);
}

#[test]
fn test_delete_request() {
    let mut handler = TestHandler::default();
    let mut state = KittyGraphicsState::default();

    // Parse delete request (a=d is Delete action, d=a means delete all)
    let params = vec![b"G".as_ref(), b"a=d,d=a".as_ref()];

    if let Some(response) = kitty_graphics_protocol::parse(&params, &mut state) {
        if let Some(delete) = response.delete_request {
            handler.delete_graphics(delete);
        }
    }

    // Verify deletion was captured
    assert_eq!(handler.deletions.len(), 1, "Should capture one deletion");
    assert_eq!(handler.deletions[0].action, b'a');
}

#[test]
fn test_query_response() {
    let mut handler = TestHandler::default();
    let mut state = KittyGraphicsState::default();

    // Parse query request
    let params = vec![b"G".as_ref(), b"a=q,i=1".as_ref()];

    if let Some(response) = kitty_graphics_protocol::parse(&params, &mut state) {
        if let Some(response_str) = response.response {
            handler.kitty_graphics_response(response_str);
        }
    }

    // Verify response was generated
    assert_eq!(handler.responses.len(), 1, "Should generate one response");
    assert!(handler.responses[0].contains("Gi=1;OK"));
}

#[test]
fn test_chunked_transfer() {
    let mut handler = TestHandler::default();
    let mut state = KittyGraphicsState::default();

    // Total base64 for 1x1 RGBA pixel [255, 0, 0, 255] is "/wAA/w==".
    // Each chunk is decoded independently now (matching ghostty / chafa
    // style), so each must be a valid base64 on its own — either a
    // multiple of 4 chars per kitty spec, or an independently padded
    // chunk. Here we use two spec-compliant chunks.

    // Chunk 1 (m=1): 4 chars → 3 decoded bytes [0xFF, 0x00, 0x00]
    let params1 = vec![
        b"G".as_ref(),
        b"a=t,f=32,s=1,v=1,m=1,i=100".as_ref(),
        b"/wAA".as_ref(),
    ];
    let result1 = kitty_graphics_protocol::parse(&params1, &mut state)
        .expect("intermediate chunks must produce a Some response");
    assert!(result1.incomplete);
    assert!(result1.graphic_data.is_none());

    // Chunk 2 (m=0): 4 chars with padding → 1 decoded byte [0xFF]
    let params3 = vec![
        b"G".as_ref(),
        b"a=t,f=32,s=1,v=1,m=0,i=100".as_ref(),
        b"/w==".as_ref(),
    ];
    if let Some(response) = kitty_graphics_protocol::parse(&params3, &mut state) {
        if let Some(graphic_data) = response.graphic_data {
            handler.insert_graphic(graphic_data, None, Some(0));
        }
    }

    // Now graphic should be created
    assert_eq!(handler.graphics.len(), 1);
    assert_eq!(handler.graphics[0].id.get(), 100);
    assert_eq!(handler.graphics[0].width, 1);
    assert_eq!(handler.graphics[0].height, 1);
}

#[test]
fn test_multiple_graphics_in_sequence() {
    let mut handler = TestHandler::default();
    let mut state = KittyGraphicsState::default();

    // Send multiple graphics (1x1 RGBA pixels with different IDs)
    // Base64 for [255, 0, 0, 255] = "/wAA/w=="
    let graphics_params = [
        (
            vec![
                b"G".as_ref(),
                b"a=t,f=32,s=1,v=1,i=1".as_ref(),
                b"/wAA/w==".as_ref(),
            ],
            1u64,
        ),
        (
            vec![
                b"G".as_ref(),
                b"a=t,f=32,s=1,v=1,i=2".as_ref(),
                b"/wAA/w==".as_ref(),
            ],
            2u64,
        ),
        (
            vec![
                b"G".as_ref(),
                b"a=t,f=32,s=1,v=1,i=3".as_ref(),
                b"/wAA/w==".as_ref(),
            ],
            3u64,
        ),
    ];

    for (params, _) in &graphics_params {
        if let Some(response) = kitty_graphics_protocol::parse(params, &mut state) {
            if let Some(graphic_data) = response.graphic_data {
                handler.insert_graphic(graphic_data, None, Some(0));
            }
        }
    }

    // Should have 3 graphics
    assert_eq!(handler.graphics.len(), 3);

    // Verify IDs
    assert_eq!(handler.graphics[0].id.get(), 1);
    assert_eq!(handler.graphics[1].id.get(), 2);
    assert_eq!(handler.graphics[2].id.get(), 3);
}
