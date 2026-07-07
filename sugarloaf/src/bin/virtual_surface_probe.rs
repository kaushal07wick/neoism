use std::time::Instant;

use sugarloaf::{
    AcceptAllVirtualSurfaceBackend, DirtyKind, NodeGeometry, NodeId, VirtualNode,
    VirtualNodeKind, VirtualResourceOp, VirtualScroll, VirtualSourceRevision,
    VirtualSurface, VirtualSurfaceBackend, VirtualSurfaceBatch, VirtualSurfaceCommand,
    VirtualViewport,
};

fn main() {
    let node_count = std::env::args()
        .nth(1)
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(100_000);
    let scroll_y = std::env::args()
        .nth(2)
        .and_then(|value| value.parse::<f32>().ok())
        .unwrap_or(1_000_000.0);

    let started = Instant::now();
    let mut surface = VirtualSurface::default();
    surface
        .apply(VirtualSurfaceCommand::SetViewport(VirtualViewport::new(
            0.0, 0.0, 920.0, 720.0, 1.0,
        )))
        .expect("viewport command should apply");

    let nodes = (0..node_count)
        .map(|ix| {
            let kind = if ix % 257 == 0 {
                VirtualNodeKind::CodeBlock
            } else {
                match ix % 17 {
                    0 => VirtualNodeKind::Heading,
                    1 | 2 => VirtualNodeKind::CodeLine,
                    3 => VirtualNodeKind::TableTile,
                    4 => VirtualNodeKind::AgentMessage,
                    _ => VirtualNodeKind::MarkdownBlock,
                }
            };
            let height = match &kind {
                VirtualNodeKind::Heading => 34.0,
                VirtualNodeKind::CodeLine => 20.0,
                VirtualNodeKind::CodeBlock => 4_096.0,
                VirtualNodeKind::TableTile => 220.0,
                VirtualNodeKind::AgentMessage => 96.0,
                _ => 24.0,
            };
            let mut geometry = NodeGeometry::fixed(height);
            if kind == VirtualNodeKind::CodeBlock {
                geometry.can_split = true;
            }
            VirtualNode::new(NodeId::new(ix as u64 + 1), kind)
                .with_geometry(geometry)
                .with_revision(1)
                .with_text_hash(ix as u64 + 0xA11CE)
        })
        .collect::<Vec<_>>();
    surface
        .apply(VirtualSurfaceCommand::ReplaceAll(nodes))
        .expect("replace-all command should apply");
    let build_ms = started.elapsed().as_secs_f64() * 1000.0;

    surface
        .apply(VirtualSurfaceCommand::SetScroll(VirtualScroll {
            scroll_y,
            velocity_y: 0.0,
        }))
        .expect("scroll command should apply");

    let mut backend = AcceptAllVirtualSurfaceBackend;
    let frame_started = Instant::now();
    let transaction = surface.build_frame_transaction();
    let commit = backend
        .execute_frame(&transaction)
        .expect("probe backend should execute frame");
    surface
        .commit_frame_transaction(&transaction, &commit)
        .expect("probe backend commit should succeed");
    let frame_us = frame_started.elapsed().as_micros();
    let second_frame_started = Instant::now();
    let second = surface.build_frame_transaction();
    let second_commit = backend
        .execute_frame(&second)
        .expect("probe backend should execute frame");
    surface
        .commit_frame_transaction(&second, &second_commit)
        .expect("probe backend commit should succeed");
    let second_frame_us = second_frame_started.elapsed().as_micros();
    let tile_scroll_y = surface
        .nodes()
        .iter()
        .enumerate()
        .find(|(_, node)| node.geometry.can_split)
        .map(|(index, _)| surface.layouts()[index].bounds.y + 1_024.0)
        .unwrap_or(scroll_y);
    surface
        .apply(VirtualSurfaceCommand::SetScroll(VirtualScroll {
            scroll_y: tile_scroll_y,
            velocity_y: 0.0,
        }))
        .expect("tile scroll command should apply");
    let tile_started = Instant::now();
    let tile_frame = surface.build_frame_transaction();
    let tile_commit = backend
        .execute_frame(&tile_frame)
        .expect("probe backend should execute frame");
    surface
        .commit_frame_transaction(&tile_frame, &tile_commit)
        .expect("probe backend commit should succeed");
    let tile_frame_us = tile_started.elapsed().as_micros();
    let metrics = surface.metrics();
    let cache = surface.cache_stats();
    let tiled_descriptors = tile_frame
        .resource_ops
        .iter()
        .filter_map(VirtualResourceOp::descriptor)
        .filter(|descriptor| descriptor.tile.is_some())
        .count();

    let dirty_range = surface.visible_range_for_current_viewport();
    let dirty_start = dirty_range.start;
    let dirty_end = (dirty_start + 32).min(metrics.node_count);
    if dirty_start < dirty_end {
        surface
            .apply(VirtualSurfaceCommand::MarkRangeDirty {
                start: dirty_start,
                end: dirty_end,
                kind: DirtyKind::Draw,
            })
            .expect("dirty range should apply");
    }
    let dirty_started = Instant::now();
    let dirty = surface.build_frame_transaction();
    let dirty_commit = backend
        .execute_frame(&dirty)
        .expect("probe backend should execute frame");
    surface
        .commit_frame_transaction(&dirty, &dirty_commit)
        .expect("probe backend commit should succeed");
    let dirty_frame_us = dirty_started.elapsed().as_micros();
    let mut stream_batch = VirtualSurfaceBatch::new(VirtualSourceRevision(2))
        .expecting_surface_revision(surface.revision());
    let stream_base = node_count as u64 + 1;
    let stream_nodes = (0..2_048)
        .map(|ix| {
            VirtualNode::new(NodeId::new(stream_base + ix), VirtualNodeKind::AgentMessage)
                .with_geometry(NodeGeometry::fixed(96.0))
                .with_revision(1)
                .with_text_hash(stream_base + ix + 0x51_0000)
        })
        .collect::<Vec<_>>();
    stream_batch.push(VirtualSurfaceCommand::UpsertNodes(stream_nodes));
    stream_batch
        .apply_to(&mut surface)
        .expect("stream batch should apply");
    let stream_scroll_y = (surface.content_height() - 720.0).max(0.0);
    surface
        .apply(VirtualSurfaceCommand::SetScroll(VirtualScroll {
            scroll_y: stream_scroll_y,
            velocity_y: 0.0,
        }))
        .expect("stream scroll command should apply");
    let stream_started = Instant::now();
    let stream = surface.build_frame_transaction();
    let stream_commit = backend
        .execute_frame(&stream)
        .expect("probe backend should execute frame");
    surface
        .commit_frame_transaction(&stream, &stream_commit)
        .expect("probe backend commit should succeed");
    let stream_frame_us = stream_started.elapsed().as_micros();
    let stream_stats = stream.stats();

    println!(
        "virtual_surface_probe nodes={} content_height={:.1} build_ms={:.2}",
        metrics.node_count, metrics.content_height, build_ms
    );
    println!(
        "frame scroll_y={:.1} visible={} commands={} resource_ops={} command_bytes={} frame_us={}",
        scroll_y,
        transaction.plan.visible.nodes.len(),
        transaction.commands.len(),
        transaction.resource_ops.len(),
        transaction.command_bytes,
        frame_us
    );
    println!(
        "plan reuse={} build={} rebuild={} upload={} bake={}",
        transaction.plan.reused,
        transaction.plan.build,
        transaction.plan.rebuild_draw,
        transaction.plan.upload_gpu,
        transaction.plan.bake_static
    );
    let frame_stats = transaction.stats();
    println!(
        "resource_stats retain={} draw={} hit={} upload={} bake={} drop={}",
        frame_stats.retain,
        frame_stats.build_draw_list,
        frame_stats.build_hit_region,
        frame_stats.upload_gpu_buffer,
        frame_stats.bake_texture,
        frame_stats.drop
    );
    println!(
        "tile_frame scroll_y={:.1} visible={} tiled_descriptors={} resource_ops={} frame_us={}",
        tile_scroll_y,
        tile_frame.plan.visible.nodes.len(),
        tiled_descriptors,
        tile_frame.resource_ops.len(),
        tile_frame_us
    );
    println!(
        "cache chunks={} hot={} warm={} cold={} frozen={} gpu_ready={} texture_backed={} bytes={}",
        cache.chunks,
        cache.hot,
        cache.warm,
        cache.cold,
        cache.frozen,
        cache.gpu_ready,
        cache.texture_backed,
        cache.estimated_bytes
    );
    println!(
        "second_frame visible={} reuse={} build={} rebuild={} upload={} bake={} commands={} resource_ops={} frame_us={}",
        second.plan.visible.nodes.len(),
        second.plan.reused,
        second.plan.build,
        second.plan.rebuild_draw,
        second.plan.upload_gpu,
        second.plan.bake_static,
        second.commands.len(),
        second.resource_ops.len(),
        second_frame_us
    );
    println!(
        "dirty_frame range={}..{} visible={} rebuild={} upload={} resource_ops={} frame_us={}",
        dirty_start,
        dirty_end,
        dirty.plan.visible.nodes.len(),
        dirty.plan.rebuild_draw,
        dirty.plan.upload_gpu,
        dirty.resource_ops.len(),
        dirty_frame_us
    );
    println!(
        "stream_frame appended=2048 visible={} build={} draw={} hit={} upload={} resource_ops={} frame_us={}",
        stream.plan.visible.nodes.len(),
        stream.plan.build,
        stream_stats.build_draw_list,
        stream_stats.build_hit_region,
        stream_stats.upload_gpu_buffer,
        stream.resource_ops.len(),
        stream_frame_us
    );
}
