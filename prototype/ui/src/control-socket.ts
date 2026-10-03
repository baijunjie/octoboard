import type { ClientToControl, ControlToClient } from "./protocol";

/**
 * Thin wrapper over `/ws/control`. One connection for the lifetime of the page — the control
 * strip and the log panel both listen to `onEvent`; nothing here owns UI state.
 */
export class ControlSocket {
  private socket: WebSocket;
  private listeners = new Set<(event: ControlToClient) => void>();

  constructor(url: string) {
    this.socket = new WebSocket(url);
    this.socket.addEventListener("message", (ev) => {
      const event = JSON.parse(ev.data as string) as ControlToClient;
      for (const listener of this.listeners) listener(event);
    });
  }

  onEvent(listener: (event: ControlToClient) => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  onOpen(listener: () => void): void {
    this.socket.addEventListener("open", listener);
  }

  send(message: ClientToControl): void {
    this.socket.send(JSON.stringify(message));
  }
}
