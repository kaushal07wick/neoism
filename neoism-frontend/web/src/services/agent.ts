// JS-side glue for the daemon-hosted Claude API agent proxy.
//
// Wire shape (mirrors `neoism-protocol/src/agent.rs`):
//
//   inbound   ServiceServerMessage::AgentReply { request_id, message }
//   outbound  ServiceClientMessage::Agent      { request_id, message }
//
// The chrome bridge (`ChromeBridge::agent_send_message` /
// `agent_cancel` / `agent_new_thread`) emits the inner
// `AgentClientMessage` JSON; we wrap it in the `Agent` envelope and
// hand it to the WebSocket. Streaming events arrive back through the
// existing `onServiceReply` channel on `ProtocolClient` and we replay
// them into the bridge via `ChromeBridge::agent_event(...)`.

import type { ProtocolClient } from "../workspace/ProtocolClient";
import type {
  AgentClientMessage,
  AgentServerMessage,
} from "../workspace/types";

export type { AgentClientMessage, AgentServerMessage };

export interface AgentBridge {
  agentEvent(eventJson: string): void;
}

/**
 * Subscribes to the WebSocket, routes inbound agent events into the
 * wasm bridge, and exposes a `send` hook the bridge can call when the
 * chrome wants to ship a `SendMessage` envelope.
 */
export class AgentService {
  constructor(
    private readonly client: ProtocolClient,
    private readonly bridge: AgentBridge,
  ) {}

  /** Route a daemon-emitted `AgentServerMessage` into the bridge.
   *  The bridge mirrors `Notice` events into the chrome's global
   *  toast stack internally (`mirror_agent_event_to_bridge`), so we
   *  don't fan it out twice here. */
  ingestServerMessage(message: AgentServerMessage): void {
    this.bridge.agentEvent(JSON.stringify(message));
  }

  /**
   * Ship a pre-allocated `AgentClientMessage` envelope to the daemon.
   * `requestId` MUST be the value the bridge allocated alongside
   * `agent_send_message` so streaming replies tag through the same
   * pending-correlation slot.
   */
  sendEnvelope(requestId: number, envelope: AgentClientMessage): void {
    // `ProtocolClient.sendAgent` wraps the envelope under the
    // top-level `Agent` service tag the daemon dispatches via
    // `ServiceClientMessage::Agent`. Going through the typed sender
    // keeps the unified status / reconnect bookkeeping in
    // `ProtocolClient` and gives us type checking on the inner
    // message variant.
    this.client.sendAgent(requestId, envelope);
  }

  /**
   * Helper that parses the JSON the bridge emits via its
   * `set_agent_send` callback and ships it to the daemon. The
   * `requestId` and `envelopeJson` come straight off the wasm
   * callback's two arguments.
   */
  forwardBridgeOutbound(requestId: number, envelopeJson: string): void {
    let envelope: AgentClientMessage;
    try {
      envelope = JSON.parse(envelopeJson) as AgentClientMessage;
    } catch (err) {
      if (typeof console !== "undefined") {
        console.warn("[agent] failed to parse outbound envelope", err);
      }
      return;
    }
    this.sendEnvelope(requestId, envelope);
  }
}
