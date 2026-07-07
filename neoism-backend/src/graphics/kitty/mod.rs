// Kitty Graphics Protocol Tests
// Combined test suite for Kitty graphics functionality

use neoism_terminal_core::ansi::graphics::KittyPlacement;
use neoism_terminal_core::ansi::kitty_graphics_protocol::{
    self, DeleteRequest, KittyGraphicsState, PlacementRequest,
};
use neoism_terminal_core::crosswords::Crosswords;
use neoism_terminal_core::graphics::{
    ColorType, GraphicData, GraphicId, ResizeCommand, ResizeParameter,
};
use neoism_terminal_core::handler::Handler;

mod cursor;
mod icat;
mod integration;
mod overlay;
mod parsing;
mod placement_mgmt;
mod regression;
mod resize;
mod screen_state;

// Common test utilities

/// Test handler that captures graphics operations
#[derive(Default)]
struct TestHandler {
    graphics: Vec<GraphicData>,
    placements: Vec<PlacementRequest>,
    deletions: Vec<DeleteRequest>,
    responses: Vec<String>,
}

impl Handler for TestHandler {
    fn insert_graphic(
        &mut self,
        data: GraphicData,
        _palette: Option<Vec<neoism_terminal_core::colors::ColorRgb>>,
        _cursor_movement: Option<u8>,
    ) {
        self.graphics.push(data);
    }

    fn place_graphic(&mut self, placement: PlacementRequest) {
        self.placements.push(placement);
    }

    fn delete_graphics(&mut self, delete: DeleteRequest) {
        self.deletions.push(delete);
    }

    fn kitty_graphics_response(&mut self, response: String) {
        self.responses.push(response);
    }
}

/// Helper to create a KittyPlacement for tests.
fn make_test_placement(
    image_id: u32,
    placement_id: u32,
    dest_col: usize,
    dest_row: i64,
    columns: u32,
    rows: u32,
    z_index: i32,
) -> KittyPlacement {
    KittyPlacement {
        image_id,
        placement_id,
        source_x: 0,
        source_y: 0,
        source_width: 0,
        source_height: 0,
        dest_col,
        dest_row,
        columns,
        rows,
        pixel_width: columns * 10,
        pixel_height: rows * 20,
        cell_x_offset: 0,
        cell_y_offset: 0,
        z_index,
        transmit_time: std::time::Instant::now(),
    }
}

fn make_test_term() -> Crosswords {
    Crosswords::new(
        neoism_terminal_core::crosswords::CrosswordsSize::new(80, 24),
        neoism_terminal_core::ansi::CursorShape::Block,
        neoism_terminal_core::TerminalId::new(0),
        10_000,
    )
}

fn store_red_pixel(term: &mut Crosswords, image_id: u32) {
    let graphic = GraphicData {
        id: GraphicId::new(image_id as u64),
        width: 1,
        height: 1,
        color_type: ColorType::Rgba,
        pixels: vec![255, 0, 0, 255],
        is_opaque: true,
        resize: None,
        display_width: None,
        display_height: None,
        transmit_time: std::time::Instant::now(),
    };
    term.store_graphic(graphic);
}

/// Drive a single icat-style transmit+display through the full pipeline.
/// `payload` is a 1x1 RGBA pixel base64 encoded; we vary the colour so
/// each transmission is distinguishable. `with_explicit_id` controls
/// whether we send `i=N` (true) or omit it (false, like icat does).
fn icat_invocation(term: &mut Crosswords, payload: &[u8], explicit_id: Option<u32>) {
    let control = match explicit_id {
        Some(id) => format!("a=T,f=32,s=1,v=1,i={id}"),
        None => "a=T,f=32,s=1,v=1".to_string(),
    };
    let params = vec![b"G".as_ref(), control.as_bytes(), payload];
    let mut state = std::mem::take(&mut term.graphics.kitty_chunking_state);
    let resp = kitty_graphics_protocol::parse(&params, &mut state);
    term.graphics.kitty_chunking_state = state;
    let resp = resp.expect("transmit+display must produce a response struct");

    if let Some(graphic_data) = resp.graphic_data {
        if let Some(placement) = resp.placement_request {
            term.kitty_transmit_and_display(graphic_data, placement);
        } else {
            term.insert_graphic(graphic_data, None, Some(0));
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct ReflowDim {
    columns: usize,
    lines: usize,
}

impl neoism_terminal_core::crosswords::grid::Dimensions for ReflowDim {
    fn columns(&self) -> usize {
        self.columns
    }
    fn screen_lines(&self) -> usize {
        self.lines
    }
    fn total_lines(&self) -> usize {
        self.lines
    }
    fn square_width(&self) -> f32 {
        10.0
    }
    fn square_height(&self) -> f32 {
        20.0
    }
}

/// Type a string of ASCII into the terminal so it lands in the grid like
/// real shell input would.
fn type_text(term: &mut Crosswords, text: &str) {
    use neoism_terminal_core::handler::Handler;
    for c in text.chars() {
        term.input(c);
    }
}

/// Print the visible grid contents for debugging.
fn dump_grid(term: &Crosswords, label: &str) {
    use neoism_terminal_core::crosswords::grid::Dimensions;
    eprintln!("=== {label} ===");
    eprintln!(
        "  cursor.row={}, history={}, columns={}, screen_lines={}",
        term.grid.cursor.pos.row.0,
        term.history_size(),
        Dimensions::columns(&term.grid),
        Dimensions::screen_lines(&term.grid),
    );
    for placement in term.graphics.kitty_placements.values() {
        eprintln!(
            "  placement: image_id={}, dest_row={}, dest_col={}, columns={}, rows={}",
            placement.image_id,
            placement.dest_row,
            placement.dest_col,
            placement.columns,
            placement.rows,
        );
    }
    use neoism_terminal_core::crosswords::pos::{Column, Line};
    let lines = Dimensions::screen_lines(&term.grid);
    let cols = Dimensions::columns(&term.grid);
    for r in 0..lines {
        let line = Line(r as i32);
        let mut s = String::new();
        for c in 0..cols {
            let cell = &term.grid[line][Column(c)];
            let ch = cell.c();
            if ch == '\0' || ch == ' ' {
                s.push('.');
            } else {
                s.push(ch);
            }
        }
        eprintln!("  row {:>2}: |{}|", r, s.trim_end_matches('.'));
    }
}
