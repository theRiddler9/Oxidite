import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { encode, decode } from "@msgpack/msgpack";
import { Connection, type Handlers } from "./connection";

class MockWebSocket {
  static OPEN = 1;
  static CLOSED = 3;
  static instances: MockWebSocket[] = [];

  readyState = 0;
  binaryType = "";
  sent: Uint8Array[] = [];
  onopen: (() => void) | null = null;
  onmessage: ((e: { data: ArrayBuffer }) => void) | null = null;
  onclose: (() => void) | null = null;
  closed = false;

  constructor(public readonly url: string) {
    MockWebSocket.instances.push(this);
  }

  send(data: Uint8Array) {
    this.sent.push(data);
  }

  close() {
    this.closed = true;
    this.readyState = MockWebSocket.CLOSED;
    this.onclose?.();
  }

  triggerOpen() {
    this.readyState = MockWebSocket.OPEN;
    this.onopen?.();
  }

  triggerServerMsg(payload: unknown) {
    const bytes = encode(payload);
    const ab = bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength);
    this.onmessage?.({ data: ab });
  }
}

describe("Connection", () => {
  let prevWebSocket: unknown;
  let prevLocation: unknown;
  let prevWindow: unknown;
  let prevDocument: unknown;
  let intervalCallbacks: (() => void)[];
  let fakeNow: number;
  let realDateNow: () => number;

  beforeEach(() => {
    MockWebSocket.instances = [];
    intervalCallbacks = [];
    fakeNow = 1_000_000;
    realDateNow = Date.now;
    Date.now = () => fakeNow;

    prevWebSocket = (globalThis as any).WebSocket;
    prevLocation = (globalThis as any).location;
    prevWindow = (globalThis as any).window;
    prevDocument = (globalThis as any).document;

    (globalThis as any).WebSocket = MockWebSocket;
    (globalThis as any).location = { protocol: "http:", host: "127.0.0.1:3030" };
    (globalThis as any).window = {
      setTimeout: globalThis.setTimeout.bind(globalThis),
      clearTimeout: globalThis.clearTimeout.bind(globalThis),
      setInterval: (fn: () => void) => {
        intervalCallbacks.push(fn);
        return intervalCallbacks.length;
      },
      clearInterval: () => {},
      addEventListener: () => {},
      removeEventListener: () => {},
    };
    (globalThis as any).document = {
      visibilityState: "visible",
      addEventListener: () => {},
      removeEventListener: () => {},
    };
  });

  afterEach(() => {
    Date.now = realDateNow;
    (globalThis as any).WebSocket = prevWebSocket;
    (globalThis as any).location = prevLocation;
    (globalThis as any).window = prevWindow;
    (globalThis as any).document = prevDocument;
  });

  function makeConn(extra: Partial<Handlers> = {}) {
    const calls: string[] = [];
    const handlers: Handlers = {
      output: (b, done) => {
        calls.push(`output:${new TextDecoder().decode(b)}`);
        done();
      },
      reset: () => calls.push("reset"),
      state: (s) => calls.push(`state:${typeof s === "string" ? s : Object.keys(s)[0]}`),
      link: (l) => calls.push(`link:${l}`),
      title: (t) => calls.push(`title:${t}`),
      prompt: (k) => calls.push(`prompt:${k}`),
      created: (id) => calls.push(`created:${id}`),
      error: (m) => calls.push(`error:${m}`),
      presence: (w, v) => calls.push(`presence:${w}:${v}`),
      attached: (id) => calls.push(`attached:${id}`),
      unauthorized: () => calls.push("unauthorized"),
      exit: (c) => calls.push(`exit:${c}`),
      ...extra,
    };
    const conn = new Connection("tok-123", handlers, () => ({ rows: 24, cols: 80 }));
    return { conn, calls };
  }

  test("sends Hello, Create/Attach, tracks cumulative offsets, handles Presence and TakeoverWriter", () => {
    const { conn, calls } = makeConn();
    conn.open({ spec: { LocalPty: { shell: null, cwd: null } } });

    const ws = MockWebSocket.instances[0]!;
    ws.triggerOpen();
    expect(decode(ws.sent[0]!)).toEqual({
      Hello: { protocol_version: 1, token: "tok-123" },
    });

    ws.triggerServerMsg({ Welcome: { server_version: "0.1.0" } });
    expect(decode(ws.sent[1]!)).toEqual({
      Create: { backend: { LocalPty: { shell: null, cwd: null } }, size: { rows: 24, cols: 80 } },
    });

    const rawId = Uint8Array.from({ length: 16 }, (_, i) => i + 1);
    ws.triggerServerMsg({ Created: { session: rawId } });
    expect(conn.id).toBe("01020304-0506-0708-090a-0b0c0d0e0f10");

    ws.triggerServerMsg({
      Attached: { session: rawId, mode: "Writer", start: { Snapshot: { at: 0 } }, client_id: "42" },
    });
    expect(calls).toContain("reset");
    expect(calls).toContain("attached:42");

    ws.triggerServerMsg({
      Output: { offset: 0, bytes: new TextEncoder().encode("hello") },
    });
    expect(conn.offset).toBe(5);

    ws.triggerServerMsg({
      Presence: { session: rawId, writer_id: null, viewers_count: 1 },
    });
    expect(calls).toContain("presence:null:1");

    conn.takeoverWriter();
    expect(decode(ws.sent.at(-1)!)).toBe("TakeoverWriter");
    conn.detach();
  });

  test("older servers omit client_id: attached() receives null", () => {
    const { conn, calls } = makeConn();
    conn.open({ id: "01020304-0506-0708-090a-0b0c0d0e0f10" });
    const ws = MockWebSocket.instances.at(-1)!;
    ws.triggerOpen();
    ws.triggerServerMsg({ Welcome: { server_version: "0.1.0" } });
    const rawId = Uint8Array.from({ length: 16 }, (_, i) => i + 1);
    ws.triggerServerMsg({ Attached: { session: rawId, mode: "Writer", start: { Replay: { from: 0 } } } });
    expect(calls).toContain("attached:null");
    conn.detach();
  });

  test("does not prematurely close healthy socket when background tab throttles ping interval", () => {
    const { conn } = makeConn();
    conn.open({ id: "01020304-0506-0708-090a-0b0c0d0e0f10" });
    const ws = MockWebSocket.instances[0]!;
    ws.triggerOpen();
    ws.triggerServerMsg({ Welcome: { server_version: "0.1.0" } });

    // Background throttling jumps 60 seconds cleanly -> first wake ping probes instead of closing
    fakeNow += 60_000;
    intervalCallbacks[0]!();
    expect(ws.closed).toBe(false);
    expect(decode(ws.sent.at(-1)!)).toHaveProperty("Ping");

    // If next interval fires 25s later with STILL no Pong, now close
    fakeNow += 25_000;
    intervalCallbacks[0]!();
    expect(ws.closed).toBe(true);
    conn.detach();
  });

  test("stops reconnecting on Unauthorized server error", () => {
    const { conn, calls } = makeConn();
    conn.open({ id: "01020304-0506-0708-090a-0b0c0d0e0f10" });
    const ws = MockWebSocket.instances[0]!;
    ws.triggerOpen();
    ws.triggerServerMsg({ Error: { code: "Unauthorized", message: "bad token" } });
    expect(calls).toContain("unauthorized");
    expect(ws.closed).toBe(true);
  });
});
