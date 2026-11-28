import { PROTOCOL_VERSION, pack, parse, bytesOf, idBytes, idStr, type BackendSpec, type Msg, type SessionState, type TermSize } from "./protocol";
import { installWakeProbe, type WakeProbeHandle } from "./wake-probe";

export type Link = "ok" | "retry" | "down";
export interface Handlers {
  output(b: Uint8Array, done: () => void): void;
  reset(): void;
  state(s: SessionState): void;
  link(l: Link): void;
  title(t: string): void;
  prompt(kind: "hostkey" | "auth", m: Msg): void;
  created(id: string): void;
  error(msg: string): void;
  unauthorized?(): void;
  presence?(writerId: string | null, viewersCount: number): void;
  /** This connection's own id as assigned by the server, to compare against `presence` writer ids. */
  attached?(clientId: string | null): void;
  exit(code: number | null): void;
}

/** One browser connection to one session. Reconnects with full jitter and resumes from the last offset. */
export class Connection {
  id: string | null = null;
  offset = 0;
  private spec: BackendSpec | null = null;
  private ws?: WebSocket;
  private attempt = 0;
  private done = false;
  private lastPong = 0;
  private pendingPingAt: number | null = null;
  private pingT?: number;
  private ackT?: number;
  private acked = 0;
  private retryT?: number;
  private wakeProbe: WakeProbeHandle;

  constructor(private token: string, private h: Handlers, private size: () => TermSize) {
    this.wakeProbe = installWakeProbe({
      isSocketOpen: () => this.ws?.readyState === WebSocket.OPEN,
      sendAppPing: () => {
        if (this.ws?.readyState !== WebSocket.OPEN) {
          throw new Error("WebSocket is not open");
        }
        this.ws.send(pack("Ping", { nonce: Date.now() }));
      },
      reconnectNow: () => this.forceReconnect(),
      deadlineMs: 4000,
    });
  }

  open(t: { id: string } | { spec: BackendSpec }): void {
    if ("id" in t) this.id = t.id; else this.spec = t.spec;
    this.connect();
  }
  input(b: Uint8Array): void { this.tx(pack("Input", b)); }
  resize(s: TermSize): void { this.tx(pack("Resize", s)); }
  trust(accept: boolean): void { this.tx(pack("HostKeyDecision", { accept })); }
  answer(answers: string[]): void { this.tx(pack("AuthResponse", { answers })); }
  takeoverWriter(): void { this.tx(pack("TakeoverWriter")); }
  kill(): void { this.tx(pack("Kill")); }
  setToken(token: string): void { this.token = token; }
  reconnectNow(): void {
    this.forceReconnect();
  }
  private forceReconnect(): void {
    if (this.done) return;
    clearInterval(this.pingT);
    clearTimeout(this.retryT);
    this.retryT = undefined;
    if (this.ws && this.ws.readyState !== WebSocket.CLOSED) {
      const stale = this.ws;
      stale.onclose = null;
      stale.onmessage = null;
      try {
        stale.close();
      } catch {
        // Ignore close failures on stalled half-open sockets
      }
      this.ws = undefined;
    }
    this.attempt = 0;
    if (this.id || this.spec) this.connect();
  }
  detach(): void {
    this.done = true;
    this.wakeProbe.destroy();
    this.tx(pack("Detach"));
    this.ws?.close();
    this.stop();
  }

  /** Send only on an open socket; typing during a reconnect must not throw. */
  private tx(b: Uint8Array): void { if (this.ws?.readyState === WebSocket.OPEN) this.ws.send(b); }

  private stop(): void { clearInterval(this.pingT); clearTimeout(this.retryT); clearTimeout(this.ackT); }

  private connect(): void {
    this.h.link("retry");
    const ws = (this.ws = new WebSocket(`${location.protocol === "https:" ? "wss" : "ws"}://${location.host}/ws`));
    ws.binaryType = "arraybuffer";
    ws.onopen = () => ws.send(pack("Hello", { protocol_version: PROTOCOL_VERSION, token: this.token }));
    ws.onmessage = (e: MessageEvent<ArrayBuffer>) => this.on(parse(e.data));
    ws.onclose = () => { clearInterval(this.pingT); if (!this.done) this.retry(); };
  }

  private retry(): void {
    this.attempt += 1;
    this.h.link("retry");
    this.h.error(`Connection lost. Retrying (attempt ${this.attempt}).`);
    const ceiling = Math.min(30_000, 1000 * 2 ** Math.min(this.attempt, 6));
    this.retryT = window.setTimeout(() => this.connect(), Math.max(250, Math.random() * ceiling));
  }

  private attach(): void {
    this.tx(pack("Attach", { session: this.id ? idBytes(this.id) : null, last_offset: this.id ? this.offset || null : null, mode: "Writer" }));
  }

  private ackTo(n: number): void {
    this.acked = n;
    if (this.ackT) return;
    this.ackT = window.setTimeout(() => { this.ackT = undefined; this.tx(pack("Ack", { offset: this.acked })); }, 30);
  }

  private on(m: Msg): void {
    this.lastPong = Date.now();
    this.pendingPingAt = null;
    this.wakeProbe.notifyTrafficReceived();
    switch (m.t) {
      case "Welcome":
        this.attempt = 0; this.lastPong = Date.now(); this.pendingPingAt = null; this.h.link("ok"); this.h.error("");
        this.pingT = window.setInterval(() => {
          const now = Date.now();
          if (this.pendingPingAt !== null && now - this.pendingPingAt > 20_000 && now - this.lastPong > 35_000) {
            this.ws?.close();
            return;
          }
          if (this.pendingPingAt === null) this.pendingPingAt = now;
          this.tx(pack("Ping", { nonce: now }));
        }, 15_000);
        if (this.id) this.attach(); else this.tx(pack("Create", { backend: this.spec, size: this.size() }));
        break;
      case "Created": this.id = idStr(m.session); this.h.created(this.id); this.attach(); break;
      case "Attached": if ((m.start as Record<string, unknown>)["Snapshot"]) this.h.reset();
        this.h.attached?.(typeof m.client_id === "string" ? m.client_id : null);
        this.h.state("Running"); break;
      case "Snapshot": { const b = bytesOf(m.bytes); this.offset = m.offset as number; this.h.output(b, () => this.ackTo(this.offset)); break; }
      case "Output": {
        const b = bytesOf(m.bytes), end = (m.offset as number) + b.length;
        this.offset = end;
        this.h.output(b, () => this.ackTo(end));
        if (b.includes(0x1b) && new TextDecoder().decode(b).includes("\x1b[6n")) {
          this.input(new TextEncoder().encode("\x1b[1;1R"));
        }
        break;
      }
      case "State": this.h.state(m.state as SessionState); break;
      case "HostKeyPrompt": this.h.prompt("hostkey", m); break;
      case "AuthPrompt": this.h.prompt("auth", m); break;
      case "Title": this.h.title(m.v as string); break;
      case "Pong":
        this.lastPong = Date.now();
        this.pendingPingAt = null;
        this.wakeProbe.notifyPongReceived();
        break;
      case "Presence":
        this.h.presence?.(
          typeof m.writer_id === "string" ? m.writer_id : null,
          Number(m.viewers_count ?? 0),
        );
        break;
      case "Exit":
        this.done = true;
        this.wakeProbe.destroy();
        this.stop();
        this.h.exit(m.code as number | null);
        break;
      case "Error":
        this.h.error(String(m.message));
        if (m.code === "Unauthorized") {
          this.done = true;
          this.wakeProbe.destroy();
          this.stop();
          this.ws?.close();
          this.h.unauthorized?.();
        }
        break;
    }
  }
}
