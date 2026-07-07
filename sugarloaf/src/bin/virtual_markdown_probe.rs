use std::time::Instant;

use sugarloaf::{
    AcceptAllVirtualSurfaceBackend, DirtyKind, NodeRevision, NodeSource,
    VirtualMarkdownAdapter, VirtualMeasuredLayout, VirtualScroll, VirtualSourceRevision,
    VirtualSourceTextStore, VirtualSurface, VirtualSurfaceBackend, VirtualSurfaceCommand,
    VirtualViewport,
};

fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let input = args.first().map(String::as_str);
    let scroll_y = args
        .get(1)
        .and_then(|value| value.parse::<f32>().ok())
        .unwrap_or(1_500_000.0);

    let source_started = Instant::now();
    let (source, line_count) = match input {
        Some(path) if std::path::Path::new(path).exists() => {
            let source = std::fs::read_to_string(path).expect("probe input should read");
            let line_count = source.lines().count();
            (source, line_count)
        }
        Some(value) => {
            let line_count = value.parse::<usize>().unwrap_or(100_000);
            (synthetic_markdown(line_count), line_count)
        }
        None => {
            let line_count = 100_000;
            (synthetic_markdown(line_count), line_count)
        }
    };
    let source_ms = source_started.elapsed().as_secs_f64() * 1000.0;
    let source_id = "probe/huge.md";
    let source_node = NodeSource::File {
        path: source_id.to_string(),
    };
    let mut source_store = VirtualSourceTextStore::new();
    source_store.insert(source_node.clone(), NodeRevision(1), source.clone());

    let mut adapter = VirtualMarkdownAdapter::new("probe-md");
    let mut surface = VirtualSurface::default();
    surface
        .apply(VirtualSurfaceCommand::SetViewport(VirtualViewport::new(
            0.0, 0.0, 920.0, 720.0, 1.0,
        )))
        .expect("viewport command should apply");

    let adapt_started = Instant::now();
    let batch = adapter.build_replace_batch(source_id, &source, VirtualSourceRevision(1));
    batch
        .apply_to(&mut surface)
        .expect("markdown batch should apply");
    let adapt_ms = adapt_started.elapsed().as_secs_f64() * 1000.0;

    surface
        .apply(VirtualSurfaceCommand::SetScroll(VirtualScroll {
            scroll_y,
            velocity_y: 0.0,
        }))
        .expect("scroll command should apply");

    let mut backend = AcceptAllVirtualSurfaceBackend;
    let frame_started = Instant::now();
    let frame = surface.build_frame_transaction();
    let commit = backend
        .execute_frame(&frame)
        .expect("probe backend should execute frame");
    surface
        .commit_frame_transaction(&frame, &commit)
        .expect("probe backend commit should apply");
    let frame_us = frame_started.elapsed().as_micros();
    let stats = adapter.stats();
    let tx = frame.stats();

    let stream = (0..2_048)
        .map(|ix| format!("streamed model markdown paragraph {ix}\n"))
        .collect::<String>();
    let append_started = Instant::now();
    let store_append_started = Instant::now();
    let source_append = source_store
        .append_revision(
            source_node.clone(),
            NodeRevision(1),
            NodeRevision(2),
            &stream,
        )
        .expect("source store append should apply");
    let source_append_us = store_append_started.elapsed().as_micros();
    let append = adapter
        .build_append_batch(source_id, &stream, VirtualSourceRevision(2))
        .expecting_surface_revision(surface.revision());
    append
        .apply_to(&mut surface)
        .expect("stream append batch should apply");
    surface
        .apply(VirtualSurfaceCommand::SetScroll(VirtualScroll {
            scroll_y: (surface.content_height() - 720.0).max(0.0),
            velocity_y: 0.0,
        }))
        .expect("stream scroll command should apply");
    let stream_frame = surface.build_frame_transaction();
    let stream_commit = backend
        .execute_frame(&stream_frame)
        .expect("probe backend should execute frame");
    surface
        .commit_frame_transaction(&stream_frame, &stream_commit)
        .expect("stream backend commit should apply");
    let append_us = append_started.elapsed().as_micros();
    let stream_tx = stream_frame.stats();
    let edit_range = {
        let entry = source_store
            .entry(&source_node, NodeRevision(2))
            .expect("source store revision 2 should exist");
        entry
            .line_index
            .line_range(&entry.text, source_append.line_count.saturating_sub(8))
            .expect("tail edit line should exist")
    };
    let source_edit = source_store
        .replace_range_revision(
            source_node,
            NodeRevision(2),
            NodeRevision(3),
            edit_range,
            "patched markdown tail from source edit\n",
        )
        .expect("source store edit should apply");
    let edit_batch = adapter
        .build_edit_batch(
            source_id,
            edit_range,
            edit_range,
            VirtualSourceRevision(3),
            DirtyKind::Draw,
        )
        .expecting_surface_revision(surface.revision());
    edit_batch
        .apply_to(&mut surface)
        .expect("markdown edit batch should apply");
    let edit_frame = surface.build_frame_transaction();
    let edit_tx = edit_frame.stats();
    let reuse = shifted_replace_reuse_probe();

    println!(
        "virtual_markdown_probe lines={} nodes={} source_ms={:.2} adapt_ms={:.2}",
        line_count,
        surface.metrics().node_count,
        source_ms,
        adapt_ms
    );
    println!(
        "blocks lines={} headings={} code={} tables={} paragraphs={} blanks={}",
        stats.lines,
        stats.headings,
        stats.code_blocks,
        stats.tables,
        stats.paragraphs,
        stats.blank_lines
    );
    println!(
        "frame scroll_y={:.1} visible={} draw={} hit={} upload={} commands={} frame_us={}",
        scroll_y,
        frame.plan.visible.nodes.len(),
        tx.build_draw_list,
        tx.build_hit_region,
        tx.upload_gpu_buffer,
        tx.draw_commands,
        frame_us
    );
    println!(
        "stream_append paragraphs=2048 visible={} build={} draw={} hit={} upload={} append_us={} source_append_us={} source_lines={} source_reindexed_lines={} source_reindexed_bytes={}",
        stream_frame.plan.visible.nodes.len(),
        stream_frame.plan.build,
        stream_tx.build_draw_list,
        stream_tx.build_hit_region,
        stream_tx.upload_gpu_buffer,
        append_us,
        source_append_us,
        source_append.line_count,
        source_append
            .line_count
            .saturating_sub(source_append.reindexed_from_line),
        source_append.reindexed_bytes
    );
    println!(
        "source_edit visible={} rebuild_draw={} upload={} source_lines={} edit_reindexed_lines={} edit_reindexed_bytes={}",
        edit_frame.plan.visible.nodes.len(),
        edit_frame.plan.rebuild_draw,
        edit_tx.upload_gpu_buffer,
        source_edit.line_count,
        source_edit
            .line_count
            .saturating_sub(source_edit.reindexed_from_line),
        source_edit.reindexed_bytes
    );
    println!(
        "replace_shift reused={} build={} rebuild_draw={} upload={} stable_task_id={} stable_task_revision={} stable_task_hash={} preserved_measured_height={} measured_height={:.1}",
        reuse.reused,
        reuse.build,
        reuse.rebuild_draw,
        reuse.upload_gpu,
        reuse.stable_task_id,
        reuse.stable_task_revision,
        reuse.stable_task_hash,
        reuse.preserved_measured_height,
        reuse.measured_height
    );
}

#[derive(Clone, Copy, Debug, Default)]
struct ShiftedReplaceReuseProbe {
    reused: usize,
    build: usize,
    rebuild_draw: usize,
    upload_gpu: usize,
    stable_task_id: bool,
    stable_task_revision: bool,
    stable_task_hash: bool,
    preserved_measured_height: bool,
    measured_height: f32,
}

fn shifted_replace_reuse_probe() -> ShiftedReplaceReuseProbe {
    let mut adapter = VirtualMarkdownAdapter::new("probe-shift-md");
    let mut surface = VirtualSurface::default();
    surface
        .apply(VirtualSurfaceCommand::SetViewport(VirtualViewport::new(
            0.0, 0.0, 520.0, 220.0, 1.0,
        )))
        .expect("reuse viewport should apply");
    adapter
        .build_replace_batch(
            "probe/reuse.md",
            "short\n- [ ] stable task\n",
            VirtualSourceRevision(1),
        )
        .apply_to(&mut surface)
        .expect("reuse initial batch should apply");
    let first = surface.build_frame_transaction();
    surface
        .commit_frame_transaction(&first, &first.successful_commit())
        .expect("reuse initial frame should commit");
    let first_task = surface
        .nodes()
        .iter()
        .find(|node| node.source_range.is_some_and(|range| range.start > 0))
        .map(|node| (node.id, node.revision, node.text_hash))
        .expect("initial task node should exist");
    surface
        .apply(VirtualSurfaceCommand::CommitMeasuredLayouts(vec![
            VirtualMeasuredLayout::new(first_task.0, first_task.1, 88.0, 0.0, 4),
        ]))
        .expect("task measured layout should commit");
    let measured = surface.build_frame_transaction();
    surface
        .commit_frame_transaction(&measured, &measured.successful_commit())
        .expect("reuse measured frame should commit");

    adapter
        .build_replace_batch(
            "probe/reuse.md",
            "a much longer first line before the same task\n- [ ] stable task\n",
            VirtualSourceRevision(2),
        )
        .apply_to(&mut surface)
        .expect("reuse shifted batch should apply");
    let second_task = surface
        .nodes()
        .iter()
        .enumerate()
        .find(|(_, node)| node.source_range.is_some_and(|range| range.start > 0))
        .map(|(index, node)| (index, node.id, node.revision, node.text_hash))
        .expect("shifted task node should exist");
    let second = surface.build_frame_transaction();
    let measured_height = surface.layouts()[second_task.0].bounds.height;
    ShiftedReplaceReuseProbe {
        reused: second.plan.reused,
        build: second.plan.build,
        rebuild_draw: second.plan.rebuild_draw,
        upload_gpu: second.plan.upload_gpu,
        stable_task_id: first_task.0 == second_task.1,
        stable_task_revision: first_task.1 == second_task.2,
        stable_task_hash: first_task.2 == second_task.3,
        preserved_measured_height: (measured_height - 88.0).abs() < 0.01,
        measured_height,
    }
}

fn synthetic_markdown(lines: usize) -> String {
    let mut out = String::with_capacity(lines.saturating_mul(48));
    let mut ix = 0usize;
    while ix < lines {
        if ix % 1_000 == 0 {
            out.push_str("# Section ");
            out.push_str(&ix.to_string());
            out.push('\n');
            ix += 1;
        } else if ix % 257 == 0 && ix + 40 < lines {
            out.push_str("```rust\n");
            ix += 1;
            for code_ix in 0..38 {
                out.push_str("let value_");
                out.push_str(&code_ix.to_string());
                out.push_str(" = compute();\n");
                ix += 1;
            }
            out.push_str("```\n");
            ix += 1;
        } else if ix % 149 == 0 && ix + 4 < lines {
            out.push_str("| name | value | status |\n");
            out.push_str("| --- | --- | --- |\n");
            out.push_str("| alpha | 123 | ready |\n");
            out.push_str("| beta | 456 | streaming |\n");
            ix += 4;
        } else {
            out.push_str("Paragraph line ");
            out.push_str(&ix.to_string());
            out.push_str(" with **bold** text and `code` and [link](notes.md).\n");
            ix += 1;
        }
    }
    out
}
