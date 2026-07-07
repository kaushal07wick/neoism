use sugarloaf::{
    AcceptAllVirtualSurfaceBackend, DirtyKind, NodeGeometry, NodeId, NodeRevision,
    NodeSource, NodeSourceRange, VirtualAgentAdapter, VirtualAgentMessage,
    VirtualAgentRole, VirtualCodeAdapter, VirtualFrameSchedulePolicy,
    VirtualMarkdownAdapter, VirtualNode, VirtualNodeKind, VirtualScroll,
    VirtualSourceRevision, VirtualSourceTextStore, VirtualSugarloafObjectPlan,
    VirtualSurface, VirtualSurfaceCommand, VirtualSurfaceFrameRequest,
    VirtualSurfacePipeline, VirtualSurfaceRoute, VirtualViewport,
};

fn main() {
    let mut surface = VirtualSurface::default();
    surface
        .apply(VirtualSurfaceCommand::SetViewport(VirtualViewport::new(
            0.0, 0.0, 920.0, 720.0, 1.0,
        )))
        .expect("viewport should apply");

    let mut markdown = VirtualMarkdownAdapter::new("adapter-md");
    markdown
        .build_replace_batch(
            "notes/demo.md",
            "# Notes\nParagraph\n```rust\nfn main() {}\n```\n",
            VirtualSourceRevision(1),
        )
        .apply_to(&mut surface)
        .expect("markdown batch should apply");

    let mut agent = VirtualAgentAdapter::new("adapter-agent");
    let agent_messages = (0..2_000)
        .map(|ix| VirtualAgentMessage {
            id: format!("msg-{ix}"),
            role: if ix % 11 == 0 {
                VirtualAgentRole::Tool
            } else if ix % 2 == 0 {
                VirtualAgentRole::Assistant
            } else {
                VirtualAgentRole::User
            },
            markdown: format!("agent markdown message {ix}\n\n- item\n- item\n"),
            tool_name: (ix % 11 == 0).then(|| "shell".to_string()),
        })
        .collect::<Vec<_>>();
    let mut agent_batch = agent.build_append_batch(
        "session-demo",
        &agent_messages,
        VirtualSourceRevision(2),
    );
    agent_batch = agent_batch.expecting_surface_revision(surface.revision());
    agent_batch
        .apply_to(&mut surface)
        .expect("agent batch should apply");
    let agent_reuse = agent_replace_reuse_probe();

    let mut code = VirtualCodeAdapter::new("adapter-code");
    let code_text = (0..50_000)
        .map(|ix| format!("let value_{ix} = compute({ix});\n"))
        .collect::<String>();
    let code_line_index = code.build_line_index(&code_text);
    let code_source = NodeSource::CodeBuffer {
        buffer: "buffer://demo.rs".to_string(),
    };
    let mut code_source_store = VirtualSourceTextStore::new();
    code_source_store.insert(code_source.clone(), NodeRevision(3), code_text.clone());
    let edit_range = {
        let entry = code_source_store
            .entry(&code_source, NodeRevision(3))
            .expect("code source should exist");
        entry
            .line_index
            .line_range(&entry.text, 49_900)
            .expect("edit line should exist")
    };
    let code_edit_stats = code_source_store
        .replace_range_revision(
            code_source,
            NodeRevision(3),
            NodeRevision(5),
            edit_range,
            "let value_49900 = patched_fast_path();\n",
        )
        .expect("code source edit should apply");
    let mut code_batch =
        code.build_append_batch("buffer://demo.rs", &code_text, VirtualSourceRevision(3));
    code_batch = code_batch.expecting_surface_revision(surface.revision());
    code_batch
        .apply_to(&mut surface)
        .expect("code batch should apply");
    surface
        .apply(VirtualSurfaceCommand::SetScroll(VirtualScroll {
            scroll_y: 220_000.0,
            velocity_y: 0.0,
        }))
        .expect("pre-splice scroll should apply");
    let anchor = surface
        .capture_scroll_anchor(120.0)
        .expect("anchor should capture");
    let anchored_node = anchor.node;
    let before_splice = surface.metrics().node_count;
    let splice_nodes = (0..8)
        .map(|ix| {
            VirtualNode::new(NodeId::new(0xD00D_0000 + ix), VirtualNodeKind::CodeLine)
                .with_geometry(NodeGeometry::fixed(20.0))
                .with_revision(4)
                .with_text_hash(0xCAFE_0000 + ix)
                .with_source(
                    NodeSource::CodeBuffer {
                        buffer: "buffer://demo.rs".to_string(),
                    },
                    NodeSourceRange::new(ix * 32, ix * 32 + 31),
                )
        })
        .collect::<Vec<_>>();
    surface
        .apply(VirtualSurfaceCommand::SpliceNodes {
            start: before_splice.saturating_sub(code.stats().tiles / 2),
            delete: 3,
            insert: splice_nodes,
        })
        .expect("splice should apply");
    if let Some(range) = surface
        .nodes()
        .iter()
        .find(|node| matches!(node.source, Some(NodeSource::CodeBuffer { .. })))
        .and_then(|node| node.source_range)
    {
        code.build_edit_batch(
            "buffer://demo.rs",
            range,
            NodeSourceRange::new(range.start, range.end.saturating_add(12)),
            VirtualSourceRevision(5),
            DirtyKind::Draw,
        )
        .expecting_surface_revision(surface.revision())
        .apply_to(&mut surface)
        .expect("source edit command should apply");
    }
    surface
        .apply(VirtualSurfaceCommand::RestoreScrollAnchor(anchor))
        .expect("anchor restore should apply");
    let restored_anchor = surface
        .capture_scroll_anchor(120.0)
        .expect("anchor should recapture");
    let anchor_preserved = restored_anchor.node == anchored_node;
    let after_splice = surface.metrics().node_count;

    surface
        .apply(VirtualSurfaceCommand::SetScroll(VirtualScroll {
            scroll_y: 500_000.0,
            velocity_y: 0.0,
        }))
        .expect("scroll should apply");
    let mut pipeline = VirtualSurfacePipeline::new(
        surface,
        AcceptAllVirtualSurfaceBackend,
        VirtualFrameSchedulePolicy {
            max_upload_ops: 2,
            max_upload_bytes: 64 * 1024 * 1024,
            ..VirtualFrameSchedulePolicy::default()
        },
    );
    let first_report = pipeline.run_frame().expect("pipeline frame should run");
    pipeline.set_schedule(VirtualFrameSchedulePolicy {
        max_upload_ops: 64,
        max_upload_bytes: 64 * 1024 * 1024,
        ..VirtualFrameSchedulePolicy::default()
    });
    let resume_report = pipeline
        .run_gpu_frame()
        .expect("resume gpu frame should run");
    let model_batch = markdown
        .build_append_batch(
            "session-demo/model-tail.md",
            "## streamed model markdown\n\n- fast path\n- same protocol\n",
            VirtualSourceRevision(4),
        )
        .expecting_surface_revision(pipeline.metrics().revision)
        .preserving_anchor(120.0);
    let request_report = pipeline
        .run_request(
            VirtualSurfaceFrameRequest::new(VirtualSurfaceRoute::model_markdown(
                "session-demo",
                "tail",
            ))
            .with_scroll(VirtualScroll {
                scroll_y: 500_000.0,
                velocity_y: 0.0,
            })
            .with_batch(model_batch),
        )
        .expect("route request should run");
    let final_gpu_packet = pipeline.build_gpu_frame_packet();
    final_gpu_packet
        .validate()
        .expect("final gpu packet should validate");
    let final_gpu_stats = final_gpu_packet.stats();
    let object_plan = VirtualSugarloafObjectPlan::from_gpu_packet(&final_gpu_packet);
    let capabilities = pipeline.capabilities();
    let request_commands = request_report
        .batch
        .as_ref()
        .map(|report| report.commands)
        .unwrap_or(0);
    let request_anchor = request_report
        .batch
        .as_ref()
        .map(|report| report.anchor_preserved)
        .unwrap_or(false);

    let code_stats = code.stats();
    let code_index_stats = code_line_index.stats();
    let code_reuse = code_replace_reuse_probe();

    println!(
        "virtual_adapter_probe nodes={} splice_delta={} anchor_preserved={} visible={} draw={} hit={} upload={} scheduled_upload={} deferred_upload={} gpu_instances={} gpu_batches={} gpu_content={} gpu_prefetch_content={} gpu_prefetch_bytes={} gpu_source_windows={} gpu_prefetch_windows={} gpu_source_window_bytes={} gpu_measure={} resume_draw={} resume_hit={} resume_upload={} request_commands={} request_anchor={} request_gpu={} final_gpu_damage={} final_gpu_prefetch={} final_gpu_prefetch_content={} final_gpu_prefetch_bytes={} final_gpu_source_windows={} final_gpu_prefetch_windows={} final_gpu_source_window_bytes={} sugarloaf_objects={} protocol={}.{} tile={} needs_frame={} markdown_nodes={} agent_messages={} agent_replace_reuse={} agent_replace_build={} agent_stable_revision={} code_replace_reuse={} code_replace_build={} code_stable_revision={} code_lines={} code_tiles={} code_bytes={} code_max_line={} code_max_tile={} code_index_checkpoints={} code_edit_reindexed_lines={} code_edit_reindexed_bytes={}",
        pipeline.metrics().node_count,
        after_splice as isize - before_splice as isize,
        anchor_preserved,
        first_report.original.visible_nodes,
        first_report.original.build_draw_list,
        first_report.original.build_hit_region,
        first_report.original.upload_gpu_buffer,
        first_report.scheduled.upload_gpu_buffer,
        first_report.deferred.upload_gpu_buffer,
        first_report.gpu.instances,
        first_report.gpu.draw_batches,
        first_report.gpu.content_requests,
        first_report.gpu.prefetch_content_requests,
        first_report.gpu.prefetch_content_bytes,
        first_report.gpu.source_windows,
        first_report.gpu.prefetch_source_windows,
        first_report.gpu.source_window_bytes,
        first_report.gpu.measurement_requests,
        resume_report.scheduled.build_draw_list,
        resume_report.scheduled.build_hit_region,
        resume_report.scheduled.upload_gpu_buffer,
        request_commands,
        request_anchor,
        request_report.frame.gpu.instances,
        final_gpu_stats.damage_regions,
        final_gpu_stats.prefetch_hints,
        final_gpu_stats.prefetch_content_requests,
        final_gpu_stats.prefetch_content_bytes,
        final_gpu_stats.source_windows,
        final_gpu_stats.prefetch_source_windows,
        final_gpu_stats.source_window_bytes,
        object_plan.len(),
        capabilities.version.major,
        capabilities.version.minor,
        capabilities.tile_height_px,
        first_report.needs_another_frame,
        markdown.stats().nodes,
        agent.stats().messages,
        agent_reuse.reused,
        agent_reuse.build,
        agent_reuse.stable_revision,
        code_reuse.reused,
        code_reuse.build,
        code_reuse.stable_revision,
        code_stats.lines,
        code_stats.tiles,
        code_stats.bytes,
        code_stats.max_line_bytes,
        code_stats.max_tile_bytes,
        code_index_stats.checkpoints,
        code_edit_stats
            .line_count
            .saturating_sub(code_edit_stats.reindexed_from_line),
        code_edit_stats.reindexed_bytes
    );
}

#[derive(Clone, Copy, Debug, Default)]
struct AgentReplaceReuseProbe {
    reused: usize,
    build: usize,
    stable_revision: bool,
}

#[derive(Clone, Copy, Debug, Default)]
struct CodeReplaceReuseProbe {
    reused: usize,
    build: usize,
    stable_revision: bool,
}

fn code_replace_reuse_probe() -> CodeReplaceReuseProbe {
    let mut adapter = VirtualCodeAdapter::with_config(
        "adapter-code-reuse",
        sugarloaf::VirtualCodeAdapterConfig {
            tile_lines: 1,
            ..sugarloaf::VirtualCodeAdapterConfig::default()
        },
    );
    let mut surface = VirtualSurface::default();
    surface
        .apply(VirtualSurfaceCommand::SetViewport(VirtualViewport::new(
            0.0, 0.0, 620.0, 140.0, 1.0,
        )))
        .expect("code reuse viewport should apply");
    adapter
        .build_replace_batch(
            "buffer://reuse.rs",
            "let a = 1;\nlet stable = compute();\n",
            VirtualSourceRevision(1),
        )
        .apply_to(&mut surface)
        .expect("code initial replace should apply");
    let stable_line = surface
        .nodes()
        .iter()
        .find(|node| {
            node.content
                .as_ref()
                .is_some_and(|content| content.line_start == 1)
        })
        .map(|node| (node.id, node.revision, node.text_hash))
        .expect("stable code line should exist");
    let first = surface.build_frame_transaction();
    surface
        .commit_frame_transaction(&first, &first.successful_commit())
        .expect("code initial frame should commit");

    adapter
        .build_replace_batch(
            "buffer://reuse.rs",
            "let much_longer_name = 1;\nlet stable = compute();\n",
            VirtualSourceRevision(2),
        )
        .apply_to(&mut surface)
        .expect("code shifted replace should apply");
    let stable_line_after = surface
        .nodes()
        .iter()
        .find(|node| {
            node.content
                .as_ref()
                .is_some_and(|content| content.line_start == 1)
        })
        .map(|node| (node.id, node.revision, node.text_hash))
        .expect("stable shifted code line should exist");
    let second = surface.build_frame_transaction();
    CodeReplaceReuseProbe {
        reused: second.plan.reused,
        build: second.plan.build,
        stable_revision: stable_line == stable_line_after,
    }
}

fn agent_replace_reuse_probe() -> AgentReplaceReuseProbe {
    let mut adapter = VirtualAgentAdapter::new("adapter-agent-reuse");
    let mut surface = VirtualSurface::default();
    surface
        .apply(VirtualSurfaceCommand::SetViewport(VirtualViewport::new(
            0.0, 0.0, 620.0, 460.0, 1.0,
        )))
        .expect("agent reuse viewport should apply");
    let initial = vec![
        VirtualAgentMessage {
            id: "user-1".to_string(),
            role: VirtualAgentRole::User,
            markdown: "make this fast".to_string(),
            tool_name: None,
        },
        VirtualAgentMessage {
            id: "assistant-1".to_string(),
            role: VirtualAgentRole::Assistant,
            markdown: "working on retained virtual markdown".to_string(),
            tool_name: None,
        },
    ];
    adapter
        .build_replace_batch("reuse-session", &initial, VirtualSourceRevision(1))
        .apply_to(&mut surface)
        .expect("agent initial replace should apply");
    let first_revision = surface.nodes()[0].revision;
    let first = surface.build_frame_transaction();
    surface
        .commit_frame_transaction(&first, &first.successful_commit())
        .expect("agent initial frame should commit");

    let mut next = initial.clone();
    next.push(VirtualAgentMessage {
        id: "assistant-2".to_string(),
        role: VirtualAgentRole::Assistant,
        markdown: "new streamed reply".to_string(),
        tool_name: None,
    });
    adapter
        .build_replace_batch("reuse-session", &next, VirtualSourceRevision(2))
        .apply_to(&mut surface)
        .expect("agent replace append should apply");
    let second = surface.build_frame_transaction();
    AgentReplaceReuseProbe {
        reused: second.plan.reused,
        build: second.plan.build,
        stable_revision: surface.nodes()[0].revision == first_revision,
    }
}
