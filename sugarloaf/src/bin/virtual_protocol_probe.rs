use sugarloaf::{
    AcceptAllVirtualSurfaceBackend, DirtyKind, NodeSource, NodeSourceRange,
    VirtualAgentAdapter, VirtualAgentMessage, VirtualAgentRole, VirtualCodeAdapter,
    VirtualFrameSchedulePolicy, VirtualGpuFrameBackend, VirtualMarkdownAdapter,
    VirtualMeasuredLayout, VirtualRevealAlign, VirtualRevealTarget, VirtualScroll,
    VirtualSourceQuery, VirtualSourceRevision, VirtualSourceTextStore,
    VirtualSurfaceBatch, VirtualSurfaceCommand, VirtualSurfaceRoute,
    VirtualSurfaceRouter, VirtualSurfaceWireEnvelope, VirtualTextOverlay,
    VirtualTextOverlayKind, VirtualViewport,
};

fn main() {
    let mut router = VirtualSurfaceRouter::default();
    let viewport = VirtualViewport::new(0.0, 0.0, 920.0, 720.0, 1.0);

    let mut markdown = VirtualMarkdownAdapter::new("protocol-md");
    let markdown_batch = markdown.build_replace_batch(
        "notes/protocol.md",
        "# Protocol\n\nShared markdown file path.\n",
        VirtualSourceRevision(1),
    );
    let markdown_route = markdown_batch.route.clone();
    let markdown_wire =
        serde_json::to_string(&markdown_batch).expect("markdown batch should serialize");
    let markdown_batch: VirtualSurfaceBatch =
        serde_json::from_str(&markdown_wire).expect("markdown batch should deserialize");
    let markdown_report = router
        .apply_batch(markdown_batch)
        .expect("markdown route batch should apply");
    router
        .apply(
            markdown_route.clone(),
            VirtualSurfaceCommand::SetViewport(viewport),
        )
        .expect("markdown viewport should apply");

    let mut agent = VirtualAgentAdapter::new("protocol-agent");
    let messages = (0..128)
        .map(|ix| VirtualAgentMessage {
            id: format!("message-{ix}"),
            role: if ix % 9 == 0 {
                VirtualAgentRole::Tool
            } else {
                VirtualAgentRole::Assistant
            },
            markdown: format!("agent streamed markdown {ix}\n\n- same route standard\n"),
            tool_name: (ix % 9 == 0).then(|| "shell".to_string()),
        })
        .collect::<Vec<_>>();
    let agent_batch = agent.build_replace_batch(
        "session-protocol",
        &messages,
        VirtualSourceRevision(1),
    );
    let agent_route = agent_batch.route.clone();
    let agent_report = router
        .apply_batch(agent_batch)
        .expect("agent route batch should apply");
    router
        .apply(
            agent_route.clone(),
            VirtualSurfaceCommand::SetViewport(viewport),
        )
        .expect("agent viewport should apply");
    router
        .apply(
            agent_route.clone(),
            VirtualSurfaceCommand::SetScroll(VirtualScroll {
                scroll_y: 4_000.0,
                velocity_y: 0.0,
            }),
        )
        .expect("agent scroll should apply");

    let mut code = VirtualCodeAdapter::new("protocol-code");
    let code_text = (0..20_000)
        .map(|ix| format!("let protocol_line_{ix} = {ix};\n"))
        .collect::<String>();
    let code_line_index = code.build_line_index(&code_text);
    let code_batch = code.build_replace_batch(
        "buffer://protocol.rs",
        &code_text,
        VirtualSourceRevision(1),
    );
    let code_stats = code.stats();
    let code_index_stats = code_line_index.stats();
    let code_route = code_batch.route.clone();
    let code_report = router
        .apply_batch(code_batch)
        .expect("code route batch should apply");
    router
        .apply(
            code_route.clone(),
            VirtualSurfaceCommand::SetViewport(viewport),
        )
        .expect("code viewport should apply");
    router
        .apply(
            code_route.clone(),
            VirtualSurfaceCommand::SetScroll(VirtualScroll {
                scroll_y: 300_000.0,
                velocity_y: 0.0,
            }),
        )
        .expect("code scroll should apply");
    router
        .apply(
            code_route.clone(),
            VirtualSurfaceCommand::MarkSourceDirty {
                source: NodeSource::CodeBuffer {
                    buffer: "buffer://protocol.rs".to_string(),
                },
                range: NodeSourceRange::new(1_024, 4_096),
                kind: DirtyKind::Draw,
            },
        )
        .expect("code source dirty should apply");
    let reveal_query = VirtualSourceQuery::new(
        NodeSource::CodeBuffer {
            buffer: "buffer://protocol.rs".to_string(),
        },
        NodeSourceRange::new(1_024, 4_096),
    );
    let source_matches = router
        .surface_mut(&code_route.id)
        .map(|surface| surface.source_matches(reveal_query.clone()).len())
        .unwrap_or(0);
    router
        .apply(
            code_route.clone(),
            VirtualSurfaceCommand::RevealSource(VirtualRevealTarget::new(
                reveal_query.source.clone(),
                reveal_query.range,
                VirtualRevealAlign::Center,
            )),
        )
        .expect("code source reveal should apply");
    router
        .apply(
            code_route.clone(),
            VirtualSurfaceCommand::SetSourceTextOverlays {
                source: reveal_query.source.clone(),
                overlays: vec![VirtualTextOverlay {
                    id: 1,
                    range: reveal_query.range,
                    kind: VirtualTextOverlayKind::Selection,
                    color: [0.2, 0.45, 0.9, 0.35],
                    priority: 10,
                }],
            },
        )
        .expect("code text overlay should apply");
    let reveal_scroll = router
        .surface(&code_route.id)
        .map(|surface| surface.scroll().scroll_y)
        .unwrap_or(0.0);

    let markdown_nodes = router
        .surface(&markdown_route.id)
        .map(|surface| surface.metrics().node_count)
        .unwrap_or(0);
    let agent_visible = router
        .surface_mut(&agent_route.id)
        .map(|surface| surface.visible_set().nodes.len())
        .unwrap_or(0);
    let code_visible = router
        .surface_mut(&code_route.id)
        .map(|surface| surface.visible_set().nodes.len())
        .unwrap_or(0);
    let content_refs = router
        .surface(&code_route.id)
        .map(|surface| {
            surface
                .nodes()
                .iter()
                .filter(|node| node.content.is_some())
                .count()
        })
        .unwrap_or(0);
    let mut content_store = VirtualSourceTextStore::new();
    content_store.insert(
        NodeSource::CodeBuffer {
            buffer: "buffer://protocol.rs".to_string(),
        },
        sugarloaf::NodeRevision(1),
        code_text.clone(),
    );
    let code_snapshot = router
        .snapshot(&code_route.id)
        .expect("code snapshot should exist");
    let code_visible_line_start = router
        .surface(&code_route.id)
        .and_then(|surface| {
            let visible = code_snapshot.visible.nodes.first()?;
            let node = surface.nodes().get(visible.index)?;
            node.content.as_ref().map(|content| content.line_start)
        })
        .unwrap_or(0);
    let measured = router
        .surface(&code_route.id)
        .and_then(|surface| {
            let visible = code_snapshot.visible.nodes.first()?;
            let node = surface.nodes().get(visible.index)?;
            Some(VirtualMeasuredLayout::new(
                node.id,
                node.revision,
                visible.bounds.height + 17.0,
                14.0,
                17,
            ))
        })
        .expect("visible code node should be measurable");
    router
        .apply(
            code_route.clone(),
            VirtualSurfaceCommand::CommitMeasuredLayouts(vec![measured]),
        )
        .expect("measured layout should apply");
    let measured_snapshot = router
        .snapshot(&code_route.id)
        .expect("measured code snapshot should exist");
    let mut backend = AcceptAllVirtualSurfaceBackend;
    let code_wire_frame = router
        .build_frame_transaction(&code_route.id)
        .expect("code frame transaction should build");
    let frame_wire = serde_json::to_string(&code_wire_frame)
        .expect("frame transaction should serialize");
    let decoded_frame: sugarloaf::VirtualFrameTransaction =
        serde_json::from_str(&frame_wire).expect("frame transaction should deserialize");
    let gpu_packet = decoded_frame.build_gpu_packet();
    gpu_packet.validate().expect("gpu packet should validate");
    let resolved_content = content_store
        .resolve_packet(&gpu_packet)
        .expect("gpu packet content should resolve");
    let resolved_prefetch_content = content_store
        .resolve_prefetch_packet(&gpu_packet)
        .expect("gpu packet prefetch content should resolve");
    let gpu_stats = gpu_packet.stats();
    let gpu_wire =
        serde_json::to_string(&gpu_packet).expect("gpu packet should serialize");
    let gpu_envelope =
        VirtualSurfaceWireEnvelope::new(code_route.clone(), gpu_packet.clone())
            .with_sequence(42);
    assert!(gpu_envelope.is_compatible());
    let envelope_wire =
        serde_json::to_string(&gpu_envelope).expect("gpu envelope should serialize");
    let decoded_envelope: VirtualSurfaceWireEnvelope<sugarloaf::VirtualGpuFramePacket> =
        serde_json::from_str(&envelope_wire).expect("gpu envelope should deserialize");
    assert_eq!(decoded_envelope.route.id, code_route.id);
    let gpu_commit_ready = backend
        .execute_gpu_frame(&gpu_packet)
        .expect("gpu packet should execute")
        .ready
        .len();
    let code_frame = router
        .run_route_frame(
            &code_route.id,
            &mut backend,
            VirtualFrameSchedulePolicy {
                max_upload_ops: 1,
                ..VirtualFrameSchedulePolicy::default()
            },
        )
        .expect("code route frame should run");
    let code_gpu_frame = router
        .run_route_gpu_frame(
            &code_route.id,
            &mut backend,
            VirtualFrameSchedulePolicy {
                max_upload_ops: 1,
                ..VirtualFrameSchedulePolicy::default()
            },
        )
        .expect("code gpu route frame should run");
    let routed_gpu_envelope = router
        .build_gpu_frame_envelope(&code_route.id, 43)
        .expect("routed gpu envelope should build");
    routed_gpu_envelope
        .payload
        .validate()
        .expect("routed gpu envelope payload should validate");
    let all_gpu_envelopes = router
        .build_all_gpu_frame_envelopes(100)
        .expect("all route gpu envelopes should build");
    for envelope in &all_gpu_envelopes {
        assert!(envelope.is_compatible());
        envelope
            .payload
            .validate()
            .expect("all route envelope payload should validate");
    }

    println!(
        "virtual_protocol_probe routes={} markdown_commands={} agent_commands={} code_commands={} markdown_nodes={} agent_visible={} code_visible={} content_refs={} content_store={} source_matches={} reveal_scroll={:.1} snapshot_range={}..{} visible_line_start={} dirty_draw={} measured_height={:.1} damage={} prefetch={} wire_bytes={} wire_ops={} gpu_instances={} gpu_batches={} gpu_content={} gpu_prefetch_content={} gpu_prefetch_bytes={} gpu_source_windows={} gpu_prefetch_windows={} gpu_source_window_bytes={} gpu_measure={} gpu_overlays={} resolved_content={} resolved_bytes={} resolved_prefetch={} resolved_prefetch_bytes={} gpu_damage={} gpu_prefetch={} gpu_range={}..{} gpu_wire={} envelope_wire={} routed_envelopes={} gpu_ready={} route_gpu={} direct_gpu={} code_upload={} code_deferred={} code_lines={} code_tiles={} code_bytes={} code_max_line={} code_max_tile={} code_trailing_newline={} code_index_checkpoints={} route_kinds={:?}/{:?}/{:?}",
        router.route_count(),
        markdown_report.commands,
        agent_report.commands,
        code_report.commands,
        markdown_nodes,
        agent_visible,
        code_visible,
        content_refs,
        content_store.len(),
        source_matches,
        reveal_scroll,
        code_snapshot.visible_start,
        code_snapshot.visible_end,
        code_visible_line_start,
        measured_snapshot.metrics.dirty_draw_count,
        measured_snapshot
            .visible
            .nodes
            .first()
            .map(|node| node.bounds.height)
            .unwrap_or(0.0),
        code_frame.scheduled.damage_regions,
        code_frame.scheduled.prefetch_hints,
        frame_wire.len(),
        decoded_frame.resource_ops.len(),
        gpu_stats.instances,
        gpu_stats.draw_batches,
        gpu_stats.content_requests,
        gpu_stats.prefetch_content_requests,
        gpu_stats.prefetch_content_bytes,
        gpu_stats.source_windows,
        gpu_stats.prefetch_source_windows,
        gpu_stats.source_window_bytes,
        gpu_stats.measurement_requests,
        gpu_stats.text_overlays,
        resolved_content.requests,
        resolved_content.bytes,
        resolved_prefetch_content.requests,
        resolved_prefetch_content.bytes,
        gpu_stats.damage_regions,
        gpu_stats.prefetch_hints,
        gpu_packet.context.visible_start,
        gpu_packet.context.visible_end,
        gpu_wire.len(),
        envelope_wire.len(),
        all_gpu_envelopes.len(),
        gpu_commit_ready,
        code_frame.gpu.instances,
        code_gpu_frame.gpu.instances,
        code_frame.scheduled.upload_gpu_buffer,
        code_frame.deferred.upload_gpu_buffer,
        code_stats.lines,
        code_stats.tiles,
        code_stats.bytes,
        code_stats.max_line_bytes,
        code_stats.max_tile_bytes,
        code_stats.trailing_newline,
        code_index_stats.checkpoints,
        markdown_route.kind,
        agent_route.kind,
        code_route.kind
    );

    let model_route = VirtualSurfaceRoute::model_markdown("session-protocol", "tail");
    assert!(!router.contains_route(&model_route.id));
}
