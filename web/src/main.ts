import "./style.css";
import "./mobile-kbd-bar.css";
import "./project-banner.css";
import type { Terminal } from "@xterm/xterm";
import type { FitAddon } from "@xterm/addon-fit";
import { makeTerminal, THEMES, type ThemeName } from "./terminal";
import { Connection, type Handlers } from "./connection";
import { stateName, type BackendSpec, type Msg, type SessionState, type ShellCapability } from "./protocol";
import { DeviceVault } from "./device-vault";
import { createComposer, type Composer, type CompletionResult } from "./composer";
import { createCommandBlocks, type CommandBlockView } from "./blocks";
import { createProjectBanner, type ProjectBannerHandle } from "./project-banner";
import { isMarkdownFileName, renderMarkdownToFragment } from "./markdown";
import { icon, type IconName } from "./icons";
import { installVisualViewportSync } from "./mobile-kbd-bar";
import { createAgentLogoElement, formatTabLabel } from "./tab-label";
import {
  buildPairShareUrl,
  buildSshAuthSpec,
  classifyGatewayStatus,
  computeRefreshDelayMs,
  defaultDeviceLabel,
  parsePairGrantFromHash,
} from "./auth-helpers";

// Gateway endpoints this UI expects:
//   GET  /api/sessions                      -> Sess[]
//   GET  /api/runtime/shells                -> ShellCapability[]
//   GET  /api/git/changes?session=ID        -> { branch, files: FileChg[] }
//   GET  /api/git/diff?session=ID&path=P    -> unified diff text
//   GET  /api/git/log?session=ID            -> { hash, subject }[]
//   POST /api/git/stage  { session, paths, staged }
//   POST /api/git/commit { session, message }
//   GET  /api/git/branches?session=ID       -> { current, branches: string[] }
//   POST /api/git/checkout { session, branch }
//   POST /api/git/fetch    { session }
// Missing endpoints show an empty state; nothing is faked.
type Sess = {
  id: string;
  state: SessionState;
  label?: string;
  workspace?: string;
  cwd?: string | null;
  cwd_id?: string | null;
  home_dir?: string | null;
  active_agent?: string | null;
  agent_label?: string | null;
  shell_kind?: string | null;
};
type FileChg = { path: string; add: number; del: number; staged: boolean };
const sessionCwds = new Map<string, string | null>();
let cwdRefreshTimer: number | undefined;
let cwdFollowupTimer: number | undefined;
interface Tab {
  id: string | null;
  label: string;
  host: string;
  el: HTMLElement;
  term: Terminal;
  fit: FitAddon;
  conn: Connection;
  a: string;
  b: string;
  msg: string;
  pair: Tab | null;
  composer: Composer;
  blocks: CommandBlockView;
  banner: ProjectBannerHandle;
  /** Folder name the gateway reports for this session; null until the first refresh. */
  ws: string | null;
  /** Full working directory path reported for this session. */
  cwd: string | null;
  homeDir: string | null;
  activeAgent: string | null;
  agentLabel: string | null;
  shellKind: string | null;
  hasWriterControl: boolean;
  /** Server-assigned id of this tab's current connection; null until `Attached` (or on older servers). */
  myClientId: string | null;
}

/** Git facts for the open session, shared by the changes panel and the input bar chips. */
interface GitCtx {
  id: string;
  branch: string;
  files: number;
  add: number;
  del: number;
}
let gitCtx: GitCtx | null = null;

const $ = <T extends HTMLElement = HTMLElement>(s: string): T => document.querySelector<T>(s) as T;
const el = <K extends keyof HTMLElementTagNameMap>(tag: K, cls = "", text = ""): HTMLElementTagNameMap[K] => {
  const e = document.createElement(tag);
  e.className = cls;
  e.textContent = text;
  return e;
};
const note = (n: HTMLElement, text: string): void => n.replaceChildren(el("p", "note", text));
const enc = new TextEncoder();
// Credentials stay strictly in memory (or encrypted in IndexedDB via DeviceVault when a PIN is set).
let token = "";
let authPrompted = false;
let localShellAvailable = true;
let powershellAvailable = false;
let shellsProbed = false;
let refreshing: Promise<void> | null = null;
let refreshFailures = 0;
let retryRefreshTimer: number | undefined;
let lastAuthError = "";

class ApiError extends Error {
  constructor(public readonly status: number | null, message: string) {
    super(message);
  }
}
// The server labels every session "session-<id>". Remember what this browser called the ones it created.
const names: Record<string, string> = (() => { try { return JSON.parse(localStorage.getItem("oxidite.names") ?? "{}"); } catch { return {}; } })();
function uniqueLabel(base: string): string {
  const used = new Set([...Object.values(names), ...tabs.map((t) => t.label)]);
  for (let i = 1; ; i++) { const l = i === 1 ? base : `${base} ${i}`; if (!used.has(l)) return l; }
}
const rememberName = (id: string, label: string): void => {
  names[id] = label;
  localStorage.setItem("oxidite.names", JSON.stringify(Object.fromEntries(Object.entries(names).slice(-100))));
};
let theme = (localStorage.getItem("oxidite.theme") as ThemeName | null) ?? "onyx";
const tabs: Tab[] = [];
let cur: Tab | null = null;
let raf = 0;
let splitDir: "h" | "v" = "h";
let splitNext = false;
let autoHid = false;
const small = (): boolean => matchMedia("(max-width: 1100px)").matches;

// Activity feed: real connection changes seen by this browser, newest first, capped at 20.
const events: { at: Date; tab: Tab; text: string; b: string }[] = [];
function logEvent(tab: Tab, text: string): void {
  if (events[0]?.tab === tab && events[0].text === text) return;
  events.unshift({ at: new Date(), tab, text, b: tab.b });
  events.length = Math.min(events.length, 20);
  renderActivity();
}
function renderActivity(): void {
  $("#actsum").textContent = "";
  const list = $("#actlist");
  if (!events.length) {
    note(list, "Connection changes show up here.");
    return;
  }
  list.replaceChildren(
    ...events.map((e) => {
      const b = el("button", "ev"),
        when = e.at.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
      b.dataset.b = e.b;
      b.append(el("i", "dot"), el("b", "", tabCaption(e.tab, false).text), el("span", "", `${e.text}, ${when}`));
      b.onclick = () => {
        if (tabs.includes(e.tab)) select(e.tab);
      };
      return b;
    }),
  );
}
function fitVisible(): void {
  for (const t of tabs) if (!t.el.hidden) t.fit.fit();
}

async function api<T>(path: string, body?: unknown, methodOverride?: string): Promise<T> {
  const signal = typeof AbortSignal !== "undefined" && "timeout" in AbortSignal ? AbortSignal.timeout(8000) : undefined;
  let r: Response;
  try {
    r = await fetch(path, {
      method: methodOverride ?? (body !== undefined ? "POST" : "GET"),
      headers: { Authorization: `Bearer ${token}`, ...(body !== undefined ? { "Content-Type": "application/json" } : {}) },
      body: body !== undefined ? JSON.stringify(body) : undefined,
      signal,
    });
  } catch {
    throw new ApiError(null, "Gateway unreachable");
  }
  if (r.status === 401) {
    token = "";
    if (!authPrompted) void promptUnlock("Your gateway token was rejected or revoked.");
    throw new ApiError(401, "Unauthorized");
  }
  if (!r.ok) {
    let errMsg = String(r.status);
    try {
      const ct = r.headers.get("content-type") ?? "";
      if (ct.includes("json")) {
        const parsed = (await r.json()) as { message?: string };
        if (parsed?.message) errMsg = parsed.message;
      }
    } catch {
      // Keep status code fallback when response has no JSON message
    }
    throw new ApiError(r.status, errMsg);
  }
  if (r.status === 204) return undefined as T;
  return (r.headers.get("content-type")?.includes("json") ? r.json() : r.text()) as Promise<T>;
}

const stateText = (s: SessionState): string => {
  if (typeof s === "object" && "Reconnecting" in s) return `Reconnecting to the host (attempt ${s.Reconnecting.attempt})`;
  if (typeof s === "object" && "Failed" in s) return `Failed: ${s.Failed.reason}`;
  return stateName(s) === "Detached" ? "Running with no viewers" : stateName(s);
};

function paint(): void {
  const c = $("#chain");
  c.dataset.a = cur?.a ?? "retry";
  c.dataset.b = cur?.b ?? "ok";
  $("#node-t").textContent = cur ? tabCaption(cur, false).text : "host";
  $("#msg").textContent = cur?.msg ?? "No session open";
  $("#off").textContent = cur ? `${(cur.conn.offset / 1024).toFixed(0)} KiB` : "";
  for (const t of tabs) if (!t.el.hidden) syncComposer(t);
}

function syncComposer(t: Tab): void {
  t.composer.setHealth(t.b);
  t.composer.setEnabled(t.b !== "down", t.msg);
  const folder = t.ws === "Remote" ? t.host : (t.cwd ?? (t.ws && t.ws !== "local shell" ? t.ws : null));
  const git = gitCtx !== null && gitCtx.id === t.id ? gitCtx : null;
  t.composer.setContext({
    folder,
    branch: git?.branch ?? null,
    files: git?.files ?? 0,
    add: git?.add ?? 0,
    del: git?.del ?? 0,
  });
}

function tabCaption(t: Tab, compact: boolean): { text: string; tooltip: string; agentId: string | null; logoId: string | null } {
  if (t.activeAgent || t.cwd) {
    return formatTabLabel(
      {
        cwd: t.cwd,
        homeDir: t.homeDir,
        active_agent: t.activeAgent,
        agent_label: t.agentLabel,
        shell_kind: t.shellKind,
      },
      compact,
    );
  }
  const fallback = t.label && t.label !== "local shell" ? t.label : "shell";
  return {
    text: fallback,
    tooltip: t.host && t.host !== "local shell" ? t.host : fallback,
    agentId: null,
    logoId: t.shellKind === "powershell" ? "powershell" : null,
  };
}

function renderTabs(): void {
  const compact = typeof matchMedia === "function" && matchMedia("(max-width: 900px)").matches;
  const add = el("button", "ib addtab");
  add.setAttribute("aria-label", "New session");
  add.onclick = () => $("#new").click();
  $("#tabs").replaceChildren(
    ...tabs.map((t) => {
      const cap = tabCaption(t, compact);
      const b = el("div", "tab"),
        main = el("button", "tabmain", cap.text),
        x = el("button", "ib x");
      main.title = cap.tooltip;
      b.setAttribute("role", "tab");
      b.setAttribute("aria-selected", String(t === cur || t === cur?.pair));
      b.dataset.b = t.b;
      if (cur?.pair && (t === cur || t === cur.pair)) b.dataset.pair = "1";
      b.addEventListener("auxclick", (e) => {
        if (e.button === 1) closeTab(t);
      });
      main.onclick = () => select(t);
      x.title = "End session (use Pause in Workspaces to suspend)";
      x.setAttribute("aria-label", `Close and end ${cap.text}`);
      x.onclick = () => closeTab(t);
      if (cap.logoId) b.append(createAgentLogoElement(cap.logoId, cap.text));
      b.append(main, x);
      return b;
    }),
    add,
  );
  $("#tabs").querySelector('[aria-selected="true"]')?.scrollIntoView({ inline: "nearest", block: "nearest" });
}

function select(t: Tab | null): void {
  if (cur && cur !== t) cur.composer.clearStickyModifiers();
  cur = t;
  $("#app").classList.remove("show-side");
  for (const x of tabs) x.el.hidden = !(x === t || (t !== null && x === t.pair));
  $("#panes").classList.toggle("has", tabs.length > 0);
  $("#panes").classList.toggle("split", !!t?.pair);
  $("#panes").classList.toggle("v", splitDir === "v");
  const spLabel = !t?.pair ? "Split terminal" : splitDir === "h" ? "Stack terminals" : "Unsplit";
  $("#sp").title = spLabel;
  $("#sp").setAttribute("aria-label", spLabel);
  $("#sp").setAttribute("aria-pressed", String(!!t?.pair));
  $("#branches").hidden = true;
  $("#branch").setAttribute("aria-expanded", "false");
  $("#dtitle").textContent = "";
  note($("#diff"), "Select a changed file on the right to see its diff.");
  if (t && !matchMedia("(max-width: 900px), (pointer: coarse)").matches) {
    requestAnimationFrame(() => {
      fitVisible();
      if (t.composer.isRaw()) t.term.focus();
      else t.composer.focus();
    });
  } else if (t) {
    requestAnimationFrame(() => fitVisible());
  }
  renderTabs();
  paint();
  void loadChanges();
  if (!$("#hist").hidden) void loadHistory();
  if (!$("#fsbox").hidden) void loadFilesTree();
  void refresh();
}

function removeTabUi(t: Tab): void {
  t.banner.destroy();
  t.composer.destroy();
  t.term.dispose();
  t.el.remove();
  tabs.splice(tabs.indexOf(t), 1);
  for (const x of tabs) if (x.pair === t) x.pair = null;
  renderActivity();
  select(cur === t ? (tabs.at(-1) ?? null) : cur);
  void refresh();
}

function closeTab(t: Tab): void {
  t.conn.kill();
  t.conn.detach();
  removeTabUi(t);
}

function suspendTab(t: Tab): void {
  t.conn.detach();
  removeTabUi(t);
}

function openTab(t: { id: string } | { spec: BackendSpec }, label: string, host: string): void {
  $("#app").classList.remove("show-side");
  if ("spec" in t) label = uniqueLabel(label);
  const pane = el("div", "pane"),
    stage = el("div", "pane-stage"),
    screen = el("div", "screen"),
    blocks = createCommandBlocks(),
    banner = createProjectBanner({
      fetchJson: (url) => api(url),
      version: "v0.1.0",
      helpHint: "tab autocomplete · ctrl+shift+l raw terminal",
      commands: [
        {
          name: "new",
          desc: "start a local or SSH session",
          onClick: () => $("#new").click(),
        },
        {
          name: "files",
          desc: "browse worktree files & markdown viewer",
          onClick: () => {
            if (matchMedia("(max-width: 720px)").matches) $("#app").classList.add("show-side");
            else $("#app").classList.remove("noside");
            switchSidebar("files");
          },
        },
        {
          name: "pull",
          desc: "fast-forward current branch from remote",
          onClick: () => document.querySelector<HTMLButtonElement>('[data-fetch-act="pull"]')?.click(),
        },
        {
          name: "push",
          desc: "push local commits to remote",
          onClick: () => document.querySelector<HTMLButtonElement>('[data-fetch-act="push"]')?.click(),
        },
        {
          name: "devices",
          desc: "manage paired phones & tokens",
          onClick: () => $("#devs").click(),
        },
      ],
    }),
    jumpTop = el("button", "jump-btn jump-top"),
    jumpBottom = el("button", "jump-btn jump-bottom");
  jumpTop.type = "button";
  jumpTop.title = "Jump to top";
  jumpTop.setAttribute("aria-label", "Jump to top of output");
  jumpTop.append(icon("jumpTop"));
  jumpTop.onclick = () => {
    if (pane.dataset.mode === "raw") {
      term.scrollToTop();
      screen.querySelector(".xterm-viewport")?.scrollTo({ top: 0 });
    } else {
      blocks.el.scrollTo({ top: 0, behavior: "smooth" });
    }
  };
  jumpBottom.type = "button";
  jumpBottom.title = "Jump to latest bottom";
  jumpBottom.setAttribute("aria-label", "Jump to bottom of output");
  jumpBottom.append(icon("jumpBottom"));
  jumpBottom.onclick = () => {
    if (pane.dataset.mode === "raw") {
      term.scrollToBottom();
      const vp = screen.querySelector(".xterm-viewport");
      if (vp) vp.scrollTop = vp.scrollHeight;
    } else {
      blocks.el.scrollTo({ top: blocks.el.scrollHeight, behavior: "smooth" });
    }
  };
  pane.dataset.mode = "blocks";
  blocks.el.prepend(banner.element);
  stage.append(blocks.el, screen, jumpTop, jumpBottom);
  pane.append(stage);
  $("#panes").append(pane);
  const { term, fit } = makeTerminal(screen, theme);
  // The input bar grows with its text, so the terminal above it refits whenever its own box changes.
  new ResizeObserver(() => {
    if (!pane.hidden) fit.fit();
  }).observe(screen);
  const initialCwd =
    "spec" in t && "LocalPty" in t.spec && typeof t.spec.LocalPty.cwd === "string" && t.spec.LocalPty.cwd.trim()
      ? t.spec.LocalPty.cwd.trim()
      : null;
  const initialShellKind =
    "spec" in t
      ? "Ssh" in t.spec
        ? "ssh"
        : t.spec.LocalPty.shell === "SystemPowerShell"
          ? "powershell"
          : null
      : null;
  const tab = {
    id: "id" in t ? t.id : null,
    label,
    host,
    el: pane,
    term,
    fit,
    a: "retry",
    b: "ok",
    msg: "Connecting",
    ws: initialCwd ? (initialCwd.split(/[/\\]/).filter(Boolean).pop() ?? null) : null,
    cwd: initialCwd,
    homeDir: null,
    activeAgent: null,
    agentLabel: null,
    shellKind: initialShellKind,
    hasWriterControl: true,
    myClientId: null,
    pair: null,
    blocks,
    banner,
  } as unknown as Tab;
  const updateLiveCwd = (rawPath: string): void => {
    const cleaned = rawPath
      .trim()
      .replace(/^["']|["']$/g, "")
      .replace(/^\\\\\?\\/, "");
    if (!/^([a-zA-Z]:[\\/]|\/)/.test(cleaned)) return;
    if (tab.cwd === cleaned) return;
    tab.cwd = cleaned;
    tab.ws = cleaned.split(/[/\\]/).filter(Boolean).pop() ?? cleaned;
    if (tab.id && tab.ws !== "Remote") void tab.banner.refreshForSession(tab.id, tab.cwd);
    renderTabs();
    paint();
    clearTimeout(cwdRefreshTimer);
    cwdRefreshTimer = window.setTimeout(() => {
      if (token && !gate.open && !vaultDlg.open && cur?.id === tab.id) void refresh();
    }, 90);
  };
  term.parser.registerOscHandler(9, (data) => {
    if (data.startsWith("9;")) {
      updateLiveCwd(data.slice(2));
      return true;
    }
    return false;
  });
  term.parser.registerOscHandler(7, (data) => {
    const m = /^file:\/\/[^/]*(.*)$/i.exec(data.trim());
    if (m && m[1]) {
      let decoded = decodeURIComponent(m[1]);
      if (/^\/[a-zA-Z]:/.test(decoded)) decoded = decoded.slice(1);
      updateLiveCwd(decoded);
      return true;
    }
    return false;
  });
  const h: Handlers = {
    output: (b, done) => {
      term.write(b, () => {
        blocks.syncFromTerminal(term);
        done?.();
      });
      if (!raf)
        raf = requestAnimationFrame(() => {
          raf = 0;
          paint();
        });
    },
    reset: () => {
      term.reset();
      blocks.reset();
      blocks.el.prepend(tab.banner.element);
    },
    link: (l) => {
      if (l === "retry" && tab.a === "ok") logEvent(tab, "Lost the gateway");
      if (l === "ok" && tab.a === "retry" && tab.msg !== "Connecting") logEvent(tab, "Back on the gateway");
      tab.a = l;
      paint();
    },
    title: (s) => {
      tab.label = s;
      renderTabs();
    },
    error: (m) => {
      if (m) {
        tab.msg = m;
        paint();
      } else if (tab.msg.startsWith("Connection lost") || tab.msg === "Connecting") {
        tab.msg = "Running";
        paint();
      }
    },
    unauthorized: () => {
      token = "";
      tab.b = "down";
      tab.msg = "Unauthorized: unlock with a valid device token";
      paint();
      if (!authPrompted) void promptUnlock("Session WebSocket rejected your token.");
    },
    created: (id) => {
      tab.id = id;
      rememberName(id, tab.label);
      if (tab.ws !== "Remote") void tab.banner.refreshForSession(id, tab.cwd);
      void refresh();
      void loadChanges();
    },
    exit: (c) => {
      tab.b = "down";
      tab.msg = c === null ? "Session ended." : `Session ended with exit code ${c}.`;
      renderTabs();
      paint();
      logEvent(tab, "Session ended");
      void refresh();
    },
    state: (s) => {
      const n = stateName(s);
      tab.b = n === "Degraded" ? "warn" : n === "Reconnecting" ? "retry" : n === "Failed" || n === "Exited" ? "down" : "ok";
      tab.msg = stateText(s);
      renderTabs();
      paint();
      logEvent(tab, stateText(s));
      void refresh();
    },
    attached: (clientId) => {
      tab.myClientId = clientId;
    },
    presence: (writerId) => {
      // Older servers never report our id; fall back to "someone holds control" as before.
      const mine = writerId !== null && (tab.myClientId === null || writerId === tab.myClientId);
      if (writerId === null && tab.b !== "down") {
        tab.hasWriterControl = false;
        tab.b = "warn";
        tab.msg = "Write control paused (slow connection). Click here to reclaim.";
        renderTabs();
        paint();
        logEvent(tab, "Write control paused");
      } else if (writerId !== null && !mine && tab.b !== "down") {
        if (tab.hasWriterControl) {
          tab.hasWriterControl = false;
          tab.b = "warn";
          tab.msg = "Another device has write control. Click here to take over.";
          renderTabs();
          paint();
          logEvent(tab, "Another device took write control");
        }
      } else if (mine && !tab.hasWriterControl) {
        tab.hasWriterControl = true;
        if (tab.b === "warn") tab.b = "ok";
        if (tab.msg.startsWith("Write control paused") || tab.msg.startsWith("Another device") || tab.msg.startsWith("Reclaiming"))
          tab.msg = "Running";
        renderTabs();
        paint();
      }
    },
    prompt: (k, m) => ask(k, m, tab),
  };
  tab.conn = new Connection(token, h, () => ({ rows: term.rows, cols: term.cols }));
  const sendInput = (d: string): void => {
    tab.conn.input(enc.encode(d));
    if (d.includes("\r") || d.includes("\n")) {
      clearTimeout(cwdRefreshTimer);
      clearTimeout(cwdFollowupTimer);
      cwdRefreshTimer = window.setTimeout(() => {
        if (token && !gate.open && !vaultDlg.open && cur?.id === tab.id) void refresh();
      }, 90);
      cwdFollowupTimer = window.setTimeout(() => {
        if (token && !gate.open && !vaultDlg.open && cur?.id === tab.id) void refresh();
      }, 450);
    }
  };
  term.onData(sendInput);
  tab.composer = createComposer({
    send: sendInput,
    onCommand: (cmd) => {
      const folder = tab.ws === "Remote" ? tab.host : (tab.cwd ?? (tab.ws && tab.ws !== "local shell" ? tab.ws : null));
      const git = gitCtx !== null && gitCtx.id === tab.id ? gitCtx : null;
      blocks.startCommand(
        cmd,
        {
          folder,
          branch: git?.branch ?? null,
          files: git?.files ?? 0,
          add: git?.add ?? 0,
          del: git?.del ?? 0,
        },
        term,
      );
    },
    complete: async (line, cursor) => {
      if (!tab.id || !token) return null;
      return api<CompletionResult>(
        `/api/complete?session=${encodeURIComponent(tab.id)}&line=${encodeURIComponent(line)}&cursor=${cursor}`,
      );
    },
    focusTerminal: () => {
      pane.dataset.mode = "raw";
      fit.fit();
      term.focus();
    },
    openChanges: () => {
      const hidden = small() ? !$("#app").classList.contains("show-diff") : $("#app").classList.contains("nodiff");
      if (hidden) $("#dt").click();
    },
    onScrollLockChange: (locked) => {
      pane.dataset.scrollLocked = locked ? "1" : "0";
      term.options.disableStdin = locked;
    },
  });
  pane.append(tab.composer.el);
  screen.addEventListener("click", () => {
    if (!tab.composer.isScrollLocked() && tab.composer.isRaw() && matchMedia("(max-width: 900px), (pointer: coarse)").matches) {
      tab.composer.focusRaw();
    }
  });
  // Full-screen programs (vim, htop) and detected Agentic CLIs switch to raw terminal input.
  term.buffer.onBufferChange((b) => {
    const raw = b.type === "alternate" || !!tab.activeAgent;
    tab.composer.setRaw(raw);
    pane.dataset.mode = raw ? "raw" : "blocks";
    fit.fit();
    if (raw) {
      if (matchMedia("(max-width: 900px), (pointer: coarse)").matches) tab.composer.focusRaw();
      else term.focus();
    } else {
      tab.composer.focus();
    }
  });
  term.onBinary((d) => tab.conn.input(Uint8Array.from(d, (c) => c.charCodeAt(0))));
  term.onResize(({ rows, cols }) => tab.conn.resize({ rows, cols }));
  tabs.push(tab);
  tab.conn.open(t);
  if (splitNext && cur) {
    splitNext = false;
    cur.pair = tab;
    if (!small() && !$("#app").classList.contains("nodiff")) {
      $("#app").classList.add("nodiff");
      autoHid = true;
    }
    select(cur);
    requestAnimationFrame(() => tab.composer.focus());
  } else select(tab);
}

function ask(kind: "hostkey" | "auth", m: Msg, tab: Tab): void {
  const dlg = $<HTMLDialogElement>("#prompt"),
    body = $("#pbody"),
    btns = $("#pbtns");
  body.replaceChildren();
  btns.replaceChildren();
  const btn = (label: string, value: string, alt = false): void => {
    const b = el("button", alt ? "btn alt" : "btn", label);
    b.value = value;
    btns.append(b);
  };
  if (kind === "hostkey") {
    $("#prompt h2").textContent = "Trust this host?";
    body.append(
      el("p", "", `First connection to ${String(m.host)}. Check this fingerprint matches the host before you continue.`),
      el("code", "", String(m.fingerprint)),
    );
    btn("Cancel", "no", true);
    btn("Trust and connect", "yes");
  } else {
    $("#prompt h2").textContent = "The host wants more";
    for (const q of m.prompts as string[]) {
      const l = el("label", "", q),
        i = el("input");
      i.type = "password";
      i.autocomplete = "off";
      l.append(i);
      body.append(l);
    }
    btn("Continue", "yes");
  }
  dlg.onclose = () => {
    if (kind === "hostkey") tab.conn.trust(dlg.returnValue === "yes");
    else tab.conn.answer([...body.querySelectorAll("input")].map((i) => i.value));
  };
  dlg.showModal();
}

function renderOfflineFallback(reason: string, canRetry: boolean): void {
  const box = $("#ws-fallback");
  box.hidden = false;
  const actions = el("div", "row");
  if (canRetry) {
    const retryBtn = el("button", "btn alt", "Retry now");
    retryBtn.type = "button";
    retryBtn.onclick = () => void refresh();
    actions.append(retryBtn);
  }
  const authBtn = el("button", "btn alt", token ? "Switch token" : "Unlock / Pair");
  authBtn.type = "button";
  authBtn.onclick = () => void promptUnlock("", true);
  actions.append(authBtn);
  box.replaceChildren(el("p", "", reason), actions);

  if (tabs.length > 0 && $("#ws").children.length === 0) {
    const g = el("section", "wsg");
    g.append(el("h3", "", "Open in this browser"));
    for (const t of tabs) {
      const b = el("button", "sess");
      b.dataset.s = t.b;
      b.dataset.open = "1";
      b.setAttribute("aria-current", String(cur === t));
      b.append(el("i", "dot"), el("b", "", t.label), el("span", "", t.a === "ok" ? "open" : "offline"));
      b.onclick = () => select(t);
      g.append(b);
    }
    $("#ws").replaceChildren(g);
    $("#empty-ws").hidden = true;
  }
}

async function probeShellCapabilities(): Promise<void> {
  if (!token) return;
  try {
    const caps = await api<ShellCapability[]>("/api/runtime/shells");
    localShellAvailable = caps.some((c) => c.id === "default" && c.available);
    powershellAvailable = caps.some((c) => c.id === "SystemPowerShell" && c.available);
    shellsProbed = true;
    applyShellAvailabilityUi();
  } catch {
    // Non-fatal fallback: keep defaults usable when local probe fails temporarily
  }
}

function applyShellAvailabilityUi(): void {
  const pwshLabel = document.querySelector<HTMLElement>("#lbl-powershell");
  const kLocal = document.querySelector<HTMLInputElement>("#k-local");
  const kPwsh = document.querySelector<HTMLInputElement>("#k-powershell");
  const kSsh = document.querySelector<HTMLInputElement>("#k-ssh");
  const cwdWrap = document.querySelector<HTMLElement>("#local-cwd-wrap");
  const disabledNote = document.querySelector<HTMLElement>("#local-disabled-note");

  if (pwshLabel) pwshLabel.hidden = !powershellAvailable;
  if (kLocal) kLocal.disabled = !localShellAvailable;
  if (kPwsh) kPwsh.disabled = !powershellAvailable;
  if (cwdWrap) cwdWrap.hidden = !localShellAvailable && !powershellAvailable;
  if (disabledNote) disabledNote.hidden = localShellAvailable || powershellAvailable;
  if (!localShellAvailable && kLocal?.checked && kSsh) kSsh.checked = true;
}

// Names are pruned only for sessions the server once listed, so a stale in-flight list cannot erase a new one.
const seen = new Set<string>();
let primed = false;
async function refresh(): Promise<void> {
  if (!token) {
    renderOfflineFallback("Gateway is locked. Pair this device or unlock with a token.", false);
    return;
  }
  if (refreshing) return refreshing;
  refreshing = (async () => {
    try {
      const list = await api<Sess[]>("/api/sessions");
      refreshFailures = 0;
      lastAuthError = "";
      clearTimeout(retryRefreshTimer);
      retryRefreshTimer = undefined;
      $("#ws-fallback").hidden = true;

      const live = new Set(list.map((x) => x.id));
      for (const id of Object.keys(names)) if (!live.has(id) && (!primed || seen.has(id))) delete names[id];
      for (const id of sessionCwds.keys()) if (!live.has(id)) sessionCwds.delete(id);
      for (const id of live) seen.add(id);
      primed = true;
      let activeCwdChanged = false;
      let tabsDirty = false;
      for (const s of list) {
        const open = tabs.find((x) => x.id === s.id);
        if (open) {
          const nextWs = s.workspace ?? null;
          const nextCwd = s.cwd ?? null;
          const nextHome = s.home_dir ?? null;
          const nextAgent = s.active_agent ?? null;
          const nextAgentLabel = s.agent_label ?? null;
          const nextShellKind = s.shell_kind ?? null;
          const agentChanged = open.activeAgent !== nextAgent;
          if (
            open.ws !== nextWs ||
            open.cwd !== nextCwd ||
            open.homeDir !== nextHome ||
            agentChanged ||
            open.agentLabel !== nextAgentLabel ||
            open.shellKind !== nextShellKind
          ) {
            open.ws = nextWs;
            open.cwd = nextCwd;
            open.homeDir = nextHome;
            open.activeAgent = nextAgent;
            open.agentLabel = nextAgentLabel;
            open.shellKind = nextShellKind;
            tabsDirty = true;
          }
          if (nextCwd && nextWs !== "Remote") {
            void open.banner.refreshForSession(s.id, nextCwd);
          }
          if (nextAgent) {
            if (!open.composer.isRaw() || open.el.dataset.mode !== "raw") {
              open.composer.setRaw(true);
              open.el.dataset.mode = "raw";
              open.fit.fit();
              if (cur === open) {
                if (matchMedia("(max-width: 900px), (pointer: coarse)").matches) open.composer.focusRaw();
                else open.term.focus();
              }
            }
          } else if (agentChanged && open.term.buffer.active.type !== "alternate") {
            open.composer.setRaw(false);
            open.el.dataset.mode = "blocks";
            open.fit.fit();
            if (cur === open) open.composer.focus();
          }
        }
        const nextCwd = s.cwd_id ?? null;
        if (!sessionCwds.has(s.id) || sessionCwds.get(s.id) !== nextCwd) {
          sessionCwds.set(s.id, nextCwd);
          if (cur?.id === s.id) activeCwdChanged = true;
        }
      }
      const groups = new Map<string, Sess[]>();
      for (const s of list) {
        const g = s.workspace ?? "Sessions";
        groups.set(g, [...(groups.get(g) ?? []), s]);
      }
      $("#ws").replaceChildren(
        ...[...groups].map(([name, items]) => {
          const g = el("section", "wsg");
          g.append(el("h3", "", name));
          for (const s of items) {
            const row = el("div", "sess-row");
            const b = el("button", "sess"),
              n = stateName(s.state),
              savedNm = names[s.id],
              nm =
                s.active_agent || s.cwd
                  ? formatTabLabel(
                      {
                        cwd: s.cwd,
                        homeDir: s.home_dir,
                        active_agent: s.active_agent,
                        agent_label: s.agent_label,
                        shell_kind: s.shell_kind,
                      },
                      false,
                    ).text
                  : (savedNm && savedNm !== "local shell" ? savedNm : s.label && s.label !== "local shell" ? s.label : s.id.slice(0, 8));
            b.dataset.s = n;
            b.setAttribute("aria-current", String(cur !== null && cur.id === s.id));
            const openTabRef = tabs.find((t) => t.id === s.id);
            const isOpen = !!openTabRef;
            b.dataset.open = isOpen ? "1" : "0";
            b.append(el("i", "dot"), el("b", "", nm), el("span", "", isOpen ? `${n}, open` : n));
            b.onclick = () => {
              if (openTabRef) select(openTabRef);
              else openTab({ id: s.id }, nm, nm);
            };
            const actBtn = el("button", "ib sess-act");
            actBtn.type = "button";
            if (isOpen && openTabRef) {
              actBtn.title = "Suspend session (keep running in background)";
              actBtn.setAttribute("aria-label", `Suspend ${nm}`);
              actBtn.append(icon("pause"));
              actBtn.onclick = (ev) => {
                ev.stopPropagation();
                suspendTab(openTabRef);
              };
            } else {
              actBtn.title = "Resume session";
              actBtn.setAttribute("aria-label", `Resume ${nm}`);
              actBtn.append(icon("play"));
              actBtn.onclick = (ev) => {
                ev.stopPropagation();
                openTab({ id: s.id }, nm, nm);
              };
            }
            row.append(b, actBtn);
            g.append(row);
          }
          return g;
        }),
      );
      $("#empty-ws").hidden = list.length > 0;
      if (tabsDirty) {
        renderTabs();
        renderActivity();
      }
      paint();
      void probeShellCapabilities();
      if (cur?.id) {
        if (activeCwdChanged) {
          $("#dtitle").textContent = "";
          note($("#diff"), "Select a changed file on the right to see its diff.");
          if (!$("#fsbox").hidden) void loadFilesTree();
        }
        void loadChanges();
        if (!$("#hist").hidden) void loadHistory();
        if (!$("#branches").hidden) void loadBranches();
      }
    } catch (err) {
      const status = err instanceof ApiError ? err.status : null;
      const kind = classifyGatewayStatus(status);
      if (kind === "unauthorized") {
        lastAuthError = "Access token was rejected or revoked.";
        renderOfflineFallback(lastAuthError, false);
        $("#msg").textContent = "Gateway token expired or revoked. Unlock or switch token.";
      } else {
        refreshFailures += 1;
        const delay = computeRefreshDelayMs(refreshFailures);
        renderOfflineFallback(
          status === null
            ? `Cannot reach gateway. Retrying in ${Math.round(delay / 1000)}s...`
            : `Gateway error (${status}). Retrying in ${Math.round(delay / 1000)}s...`,
          true,
        );
        if (!cur || cur.msg === "No session open") {
          $("#msg").textContent = "Gateway unreachable — retrying automatically.";
        }
        clearTimeout(retryRefreshTimer);
        retryRefreshTimer = window.setTimeout(() => {
          if (token && document.visibilityState === "visible") void refresh();
        }, delay);
      }
    } finally {
      refreshing = null;
    }
  })();
  return refreshing;
}

function clearGit(branch = "No repository"): void {
  $("#bname").textContent = branch;
  $("#sum").textContent = "";
  $("#count").textContent = "0";
  gitCtx = null;
  paint();
}

async function loadChanges(): Promise<void> {
  const list = $("#files");
  if (!token) {
    clearGit();
    note(list, "Unlock the gateway to view session changes.");
    return;
  }
  if (!cur?.id) {
    clearGit();
    note(list, "Open a session in a Git folder to view its changes.");
    return;
  }
  try {
    const sessionId = cur.id;
    const r = await api<{
      is_repo?: boolean;
      folder?: string | null;
      branch: string;
      ahead?: number;
      behind?: number;
      files: FileChg[];
    }>(
      `/api/git/changes?session=${encodeURIComponent(sessionId)}`,
    );
    if (cur?.id !== sessionId) return;
    if (r.is_repo === false) {
      clearGit("No repository");
      const wrap = el("div", "empty-git");
      const title = el("strong", "", r.folder ? `"${r.folder}" is not a Git repository.` : "Not inside a Git repository.");
      const sub = el("p", "", "Initialize a repository to start tracking changes, or open a folder with an existing Git history.");
      wrap.append(title, sub);
      list.replaceChildren(wrap);
      return;
    }
    const add = r.files.reduce((a, f) => a + f.add, 0),
      del = r.files.reduce((a, f) => a + f.del, 0);
    const branchLabel = r.branch && r.branch !== "HEAD (unborn)" ? r.branch : "No branch";
    const ahead = r.ahead ?? 0;
    const behind = r.behind ?? 0;
    const ab = [behind > 0 ? `\u2193${behind}` : "", ahead > 0 ? `\u2191${ahead}` : ""].filter(Boolean).join(" ");
    $("#bname").textContent = branchLabel;
    $("#sum").textContent = ab ? `${ab}  +${add} \u2212${del}` : `+${add} \u2212${del}`;
    $("#count").textContent = String(r.files.length);
    list.replaceChildren(...(r.files.length ? r.files.map(fileRow) : [el("p", "note", "No changes in this folder.")]));
    gitCtx = { id: sessionId, branch: branchLabel, files: r.files.length, add, del };
    if (cur.id === sessionId) cur.blocks.updateActiveContext({ folder: cur.cwd ?? cur.ws, branch: branchLabel, files: r.files.length, add, del });
    paint();
  } catch {
    clearGit();
    note(list, "Git data is unavailable for this session.");
  }
}

function fileRow(f: FileChg): HTMLElement {
  const r = el("div", "file"),
    name = el("button", "fname"),
    cb = el("input"),
    i = f.path.lastIndexOf("/");
  cb.type = "checkbox";
  cb.checked = f.staged;
  cb.setAttribute("aria-label", `Stage ${f.path}`);
  cb.onchange = () => void stage([f.path], cb.checked);
  name.append(el("b", "", f.path.slice(i + 1)), el("small", "", i > 0 ? f.path.slice(0, i) : ""));
  name.onclick = () => void loadDiff(f.path);
  r.append(name, el("span", "add", `+${f.add}`), el("span", "del", `−${f.del}`), cb);
  return r;
}

async function stage(paths: string[], staged: boolean): Promise<void> {
  if (!cur?.id) return;
  try {
    await api("/api/git/stage", { session: cur.id, paths, staged });
  } catch {
    $("#msg").textContent = "Could not change staging.";
  }
  await loadChanges();
}

async function loadHistory(): Promise<void> {
  const box = $("#hist");
  if (!cur?.id) {
    note(box, "Open a session in a Git folder to view its commit history.");
    return;
  }
  try {
    const log = await api<{ hash: string; subject: string }[]>(
      `/api/git/log?session=${encodeURIComponent(cur.id)}`,
    );
    if (!log.length) {
      note(box, "No commit history in the current session folder.");
      return;
    }
    box.replaceChildren(
      ...log.map((c) => {
        const r = el("div", "commit");
        r.append(el("b", "", c.subject), el("small", "", c.hash.slice(0, 7)));
        return r;
      }),
    );
  } catch {
    note(box, "History is unavailable for this session.");
  }
}

async function loadDiff(path: string): Promise<void> {
  if (!cur?.id) return;
  $("#dtitle").textContent = path;
  $("#app").classList.remove("nodiff");
  if (small()) {
    $("#app").classList.remove("show-chg");
    $("#app").classList.add("show-diff");
  }
  try {
    renderDiff(
      await api<string>(
        `/api/git/diff?session=${encodeURIComponent(cur.id)}&path=${encodeURIComponent(path)}`,
      ),
    );
  } catch {
    note($("#diff"), "Could not load this diff.");
  }
}

const KW =
  /^(?:fn|let|mut|pub|use|mod|impl|struct|enum|trait|match|if|else|for|while|loop|return|const|static|async|await|import|from|export|default|function|class|new|try|catch|throw|def|self|None|True|False|true|false|null|undefined|type|interface|extends|as|in|of|yield|package|func|var|where|ref|move|dyn|crate|super)$/;
const TOK =
  /(\/\/.*|#(?!\[).*)|("(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*'|`(?:[^`\\]|\\.)*`)|(\b\d[\w.]*\b)|(\b[A-Za-z_]\w*\b)/g;

/** Light, language-agnostic highlighting: comments, strings, numbers, keywords, Capitalized types. */
function hl(text: string): DocumentFragment {
  const f = document.createDocumentFragment();
  let i = 0;
  for (const m of text.matchAll(TOK)) {
    const at = m.index ?? 0,
      t = m[0];
    if (at > i) f.append(text.slice(i, at));
    const cls = m[1] ? "tc" : m[2] ? "ts" : m[3] ? "tn" : KW.test(t) ? "tk" : /^[A-Z]/.test(t) ? "tt" : "";
    f.append(cls ? el("span", cls, t) : t);
    i = at + t.length;
  }
  f.append(text.slice(i));
  return f;
}

/** Unified diff -> side-by-side rows. Deletions and additions in a hunk are paired line by line. */
function renderDiff(text: string): void {
  const out = $("#diff"),
    rows: HTMLElement[] = [];
  let l = 0,
    r = 0,
    dels: string[] = [],
    adds: string[] = [],
    hunk = false;
  const cell = (cls: string, t: string): HTMLElement => {
    if (!cls.startsWith("c")) return el("span", cls, t);
    const c = el("span", cls);
    c.append(hl(t));
    return c;
  };
  const row = (ln: number | "", lt: string | undefined, rn: number | "", rt: string | undefined, ctx = false): void => {
    const d = el("div", "drow");
    d.append(
      cell("n", String(ln)),
      cell(ctx ? "c" : lt === undefined ? "c nil" : "c del", lt ?? ""),
      cell("n", String(rn)),
      cell(ctx ? "c" : rt === undefined ? "c nil" : "c add", rt ?? ""),
    );
    rows.push(d);
  };
  const flush = (): void => {
    for (let i = 0; i < Math.max(dels.length, adds.length); i++) {
      const a = dels[i],
        b = adds[i];
      row(a !== undefined ? ++l : "", a, b !== undefined ? ++r : "", b);
    }
    dels = [];
    adds = [];
  };
  for (const line of text.split("\n")) {
    const m = /^@@ -(\d+)(?:,\d+)? \+(\d+)/.exec(line);
    if (m) {
      flush();
      hunk = true;
      l = Number(m[1]) - 1;
      r = Number(m[2]) - 1;
      rows.push(el("div", "hunk", line));
      continue;
    }
    if (!hunk) continue;
    if (line.startsWith("-")) dels.push(line.slice(1));
    else if (line.startsWith("+")) adds.push(line.slice(1));
    else if (line.startsWith(" ")) {
      flush();
      row(++l, line.slice(1), ++r, line.slice(1), true);
    }
  }
  flush();
  if (rows.length) out.replaceChildren(...rows);
  else note(out, "No textual changes in this file.");
}

function showTab(changes: boolean): void {
  $("#tab-c").setAttribute("aria-selected", String(changes));
  $("#tab-h").setAttribute("aria-selected", String(!changes));
  $("#files").hidden = !changes;
  $("#commitbox").hidden = !changes;
  $("#hist").hidden = changes;
  if (changes) void loadChanges();
  else void loadHistory();
}

function setTheme(n: ThemeName): void {
  theme = n;
  document.documentElement.dataset.theme = n;
  localStorage.setItem("oxidite.theme", n);
  for (const t of tabs) t.term.options.theme = THEMES[n].term;
  for (const b of document.querySelectorAll<HTMLButtonElement>(".sw"))
    b.setAttribute("aria-pressed", String(b.dataset.t === n));
}

$("#themes").replaceChildren(
  ...(Object.keys(THEMES) as ThemeName[]).map((n) => {
    const b = el("button", "sw");
    b.dataset.t = n;
    b.title = THEMES[n].label;
    b.setAttribute("aria-label", `${THEMES[n].label} theme`);
    b.style.setProperty("--sw", THEMES[n].acc);
    b.style.setProperty("--sw2", THEMES[n].acc2);
    b.onclick = () => setTheme(n);
    return b;
  }),
);
setTheme(theme);

for (const b of document.querySelectorAll<HTMLElement>("[data-icon]")) b.prepend(icon(b.dataset.icon as IconName));

// Pressed state for the ribbon toggles follows the real layout classes, whichever code changed them.
const appEl = $("#app");
const syncRibbon = (): void => {
  const c = appEl.classList;
  const mobileSide = matchMedia("(max-width: 720px)").matches;
  $("#sb").setAttribute("aria-pressed", String(mobileSide ? c.contains("show-side") : !c.contains("noside") && !c.contains("focus")));
  $("#dt").setAttribute("aria-pressed", String(small() ? c.contains("show-diff") : !c.contains("nodiff") && !c.contains("focus")));
  $("#cg").setAttribute("aria-pressed", String(small() ? c.contains("show-chg") : !c.contains("nochg") && !c.contains("focus")));
  $("#fs").setAttribute("aria-pressed", String(c.contains("focus")));
};
new MutationObserver(syncRibbon).observe(appEl, { attributes: true, attributeFilter: ["class"] });
syncRibbon();

// Ctrl+Shift+L moves between the input bar (blocks) and raw typing in the terminal.
document.addEventListener("keydown", (e) => {
  if (!cur || !e.ctrlKey || !e.shiftKey || e.altKey || e.metaKey || e.key.toLowerCase() !== "l") return;
  e.preventDefault();
  if (cur.el.dataset.mode !== "raw") {
    cur.el.dataset.mode = "raw";
    cur.fit.fit();
    cur.term.focus();
  } else if (!cur.composer.isRaw()) {
    cur.el.dataset.mode = "blocks";
    cur.fit.fit();
    cur.composer.focus();
  }
});

$("#tab-c").onclick = () => showTab(true);
$("#tab-h").onclick = () => showTab(false);
$("#all").onclick = () => void stage([], true);
$("#dt").onclick = () => {
  autoHid = false;
  $("#app").classList.toggle(small() ? "show-diff" : "nodiff");
};
$("#fs").onclick = () => $("#app").classList.toggle("focus");
$("#sp").onclick = () => {
  if (!cur) {
    $("#msg").textContent = "Open a session first, then split it.";
    return;
  }
  if (!cur.pair) {
    splitNext = true;
    $<HTMLDialogElement>("#newdlg").showModal();
    return;
  }
  if (splitDir === "h") splitDir = "v";
  else {
    splitDir = "h";
    cur.pair = null;
    if (autoHid) {
      $("#app").classList.remove("nodiff");
      autoHid = false;
    }
  }
  select(cur);
};

async function loadBranches(): Promise<void> {
  const m = $("#branches");
  if (!cur?.id) {
    note(m, "Open a session in a Git folder to list branches.");
    return;
  }
  try {
    const r = await api<{ is_repo?: boolean; current: string; branches: string[] }>(
      `/api/git/branches?session=${encodeURIComponent(cur.id)}`,
    );
    if (r.is_repo === false) {
      note(m, "No Git repository detected in this directory.");
      return;
    }
    if (r.branches.length === 0) {
      note(m, "No branches yet. Create an initial commit to establish a branch.");
      return;
    }
    m.replaceChildren(
      ...r.branches.map((n) => {
        const b = el("button", "", n);
        b.setAttribute("role", "option");
        b.setAttribute("aria-selected", String(n === r.current));
        b.onclick = async () => {
          if (!cur?.id) return;
          try {
            await api("/api/git/checkout", { session: cur.id, branch: n });
            $("#msg").textContent = `Switched to ${n}.`;
            if (cur.ws !== "Remote") void cur.banner.refreshForSession(cur.id, cur.cwd, true);
          } catch {
            $("#msg").textContent = `Could not switch to ${n}. Commit or stash your changes first.`;
          }
          await loadChanges();
        };
        return b;
      }),
    );
  } catch {
    note(m, "Branches are unavailable for this session.");
  }
}
type CommitMode = "tracked" | "staged" | "all";
const COMMIT_LABELS: Record<CommitMode, string> = {
  tracked: "Commit Tracked",
  staged: "Commit Staged",
  all: "Commit All",
};
const savedCommitMode = localStorage.getItem("oxidite.commitMode");
let commitMode: CommitMode =
  savedCommitMode && savedCommitMode in COMMIT_LABELS
    ? (savedCommitMode as CommitMode)
    : "tracked";

function syncCommitModeUi(): void {
  $("#commit").textContent = COMMIT_LABELS[commitMode];
  for (const b of document.querySelectorAll<HTMLButtonElement>("#commit-mode-menu [data-mode]")) {
    b.setAttribute("aria-checked", String(b.dataset.mode === commitMode));
  }
}
syncCommitModeUi();

const closeAllMenus = (): void => {
  $("#branches").hidden = true;
  $("#branch").setAttribute("aria-expanded", "false");
  $("#fetch-menu").hidden = true;
  $("#fetch-menu-btn").setAttribute("aria-expanded", "false");
  $("#commit-mode-menu").hidden = true;
  $("#commit-mode-btn").setAttribute("aria-expanded", "false");
};

$("#branch").onclick = (e) => {
  e.stopPropagation();
  const m = $("#branches");
  const nextHidden = !m.hidden;
  closeAllMenus();
  m.hidden = nextHidden;
  $("#branch").setAttribute("aria-expanded", String(!nextHidden));
  if (!nextHidden) void loadBranches();
};
$("#fetch-menu-btn").onclick = (e) => {
  e.stopPropagation();
  const m = $("#fetch-menu");
  const nextHidden = !m.hidden;
  closeAllMenus();
  m.hidden = nextHidden;
  $("#fetch-menu-btn").setAttribute("aria-expanded", String(!nextHidden));
};
$("#commit-mode-btn").onclick = (e) => {
  e.stopPropagation();
  const m = $("#commit-mode-menu");
  const nextHidden = !m.hidden;
  closeAllMenus();
  m.hidden = nextHidden;
  $("#commit-mode-btn").setAttribute("aria-expanded", String(!nextHidden));
};
for (const b of document.querySelectorAll<HTMLButtonElement>("#commit-mode-menu [data-mode]")) {
  b.onclick = (e) => {
    e.stopPropagation();
    const mode = b.dataset.mode as CommitMode;
    if (mode in COMMIT_LABELS) {
      commitMode = mode;
      localStorage.setItem("oxidite.commitMode", mode);
      syncCommitModeUi();
    }
    closeAllMenus();
  };
}
for (const b of document.querySelectorAll<HTMLButtonElement>("#fetch-menu [data-fetch-act]")) {
  b.onclick = async (e) => {
    e.stopPropagation();
    const act = b.dataset.fetchAct;
    closeAllMenus();
    if (act === "fetch") {
      $("#fetch").click();
    } else if (act === "pull" || act === "push") {
      if (!cur?.id) {
        $("#msg").textContent = "Open a session in a Git folder first.";
        return;
      }
      const fetchBtn = $<HTMLButtonElement>("#fetch");
      const lbl = $("#fetch-label");
      fetchBtn.disabled = true;
      lbl.textContent = act === "pull" ? "Pulling" : "Pushing";
      try {
        const res = await api<{ ok: boolean; message: string }>(`/api/git/${act}`, { session: cur.id });
        $("#msg").textContent = res.message || (act === "pull" ? "Pulled remote changes." : "Pushed to remote.");
      } catch (err) {
        $("#msg").textContent = err instanceof ApiError ? err.message : `${act === "pull" ? "Pull" : "Push"} failed.`;
      }
      fetchBtn.disabled = false;
      lbl.textContent = "Fetch";
      await loadChanges();
      if (cur.id && cur.ws !== "Remote") void cur.banner.refreshForSession(cur.id, cur.cwd, true);
      if (!$("#hist").hidden) await loadHistory();
    } else if (act === "refresh") {
      await loadChanges();
      if (cur?.id && cur.ws !== "Remote") void cur.banner.refreshForSession(cur.id, cur.cwd, true);
      if (!$("#hist").hidden) await loadHistory();
    }
  };
}
$("#cmsg-expand").onclick = () => {
  const ta = $<HTMLTextAreaElement>("#cmsg");
  const expanded = !ta.classList.contains("expanded");
  ta.classList.toggle("expanded", expanded);
  ta.rows = expanded ? 7 : 3;
  $("#cmsg-expand").setAttribute("aria-expanded", String(expanded));
};
document.addEventListener("click", closeAllMenus);
document.addEventListener("keydown", (e) => {
  if (e.key === "Escape") {
    closeAllMenus();
    if (!$("#filepane").hidden) closeFileViewer();
  }
});
$("#fetch").onclick = async () => {
  if (!cur?.id) {
    $("#msg").textContent = "Open a session in a Git folder first.";
    return;
  }
  const b = $<HTMLButtonElement>("#fetch");
  const lbl = $("#fetch-label");
  b.disabled = true;
  lbl.textContent = "Fetching";
  try {
    await api("/api/git/fetch", { session: cur.id });
    $("#msg").textContent = "Fetched from the remote.";
    if (cur.ws !== "Remote") void cur.banner.refreshForSession(cur.id, cur.cwd, true);
  } catch {
    $("#msg").textContent = "Fetch failed. Check the remote and its credentials on the gateway.";
  }
  b.disabled = false;
  lbl.textContent = "Fetch";
  await loadChanges();
};
$("#commit").onclick = async () => {
  if (!cur?.id) {
    $("#msg").textContent = "Open a session in a Git folder first.";
    return;
  }
  const m = $<HTMLTextAreaElement>("#cmsg");
  if (!m.value.trim()) {
    m.focus();
    return;
  }
  try {
    await api("/api/git/commit", { session: cur.id, message: m.value, mode: commitMode });
    m.value = "";
    $("#msg").textContent = "Commit created.";
    if (cur.ws !== "Remote") void cur.banner.refreshForSession(cur.id, cur.cwd, true);
  } catch {
    $("#msg").textContent =
      commitMode === "staged"
        ? "Commit failed. Stage at least one file first."
        : "Commit failed. Check that there are changes to commit.";
  }
  await loadChanges();
};
new ResizeObserver(fitVisible).observe($("#panes"));

// Drawers on small screens, hideable panes on desktop, drag handles on wide ones.
$("#sb").onclick = () => {
  if (matchMedia("(max-width: 720px)").matches) $("#app").classList.toggle("show-side");
  else $("#app").classList.toggle("noside");
  fitVisible();
};
$("#cg").onclick = () => {
  if (small()) {
    $("#app").classList.remove("show-diff");
    $("#app").classList.toggle("show-chg");
  } else {
    $("#app").classList.toggle("nochg");
    fitVisible();
  }
};
$("#side-close").onclick = () => $("#app").classList.remove("show-side");
$("#diff-close").onclick = () => $("#app").classList.remove("show-diff");
$("#chg-close").onclick = () => $("#app").classList.remove("show-chg");
$("#term-pane").addEventListener("pointerdown", () => $("#app").classList.remove("show-side", "show-diff", "show-chg"));
$("#ws").addEventListener("click", () => $("#app").classList.remove("show-side"));

type FsEntry = { name: string; path: string; is_dir: boolean };
interface FileViewState {
  name: string;
  path: string;
  content: string;
  truncated: boolean;
  isMarkdown: boolean;
  mode: "rendered" | "raw";
}
let fileViewState: FileViewState | null = null;
let fileLoadSeq = 0;

function renderActiveFileView(): void {
  const out = $("#filebody");
  if (!fileViewState) {
    out.replaceChildren();
    return;
  }
  const modeBar = $("#fp-mode");
  modeBar.hidden = !fileViewState.isMarkdown;
  $("#fp-rendered").setAttribute("aria-selected", String(fileViewState.mode === "rendered"));
  $("#fp-raw").setAttribute("aria-selected", String(fileViewState.mode === "raw"));

  if (fileViewState.isMarkdown && fileViewState.mode === "rendered") {
    const doc = el("article", "md-view");
    doc.append(renderMarkdownToFragment(fileViewState.content));
    if (fileViewState.truncated) {
      doc.append(el("div", "hunk", "Preview truncated at 256 KiB"));
    }
    out.replaceChildren(doc);
    out.scrollTop = 0;
    return;
  }

  const lines = fileViewState.content.split(/\r?\n/);
  const rows = lines.map((line, idx) => {
    const r = el("div", "drow preview");
    const ln = el("span", "n", String(idx + 1));
    const code = el("span", "c");
    code.append(hl(line));
    r.append(ln, code);
    return r;
  });
  if (fileViewState.truncated) {
    rows.push(el("div", "hunk", "Preview truncated at 256 KiB"));
  }
  out.replaceChildren(...rows);
  out.scrollTop = 0;
}

function closeFileViewer(): void {
  $("#filepane").hidden = true;
  fileViewState = null;
  for (const prev of $("#fstree").querySelectorAll(".fs-row.is-active")) {
    prev.classList.remove("is-active");
  }
}

$("#file-close").onclick = closeFileViewer;
$("#fp-rendered").onclick = () => {
  if (!fileViewState || !fileViewState.isMarkdown) return;
  fileViewState.mode = "rendered";
  renderActiveFileView();
};
$("#fp-raw").onclick = () => {
  if (!fileViewState || !fileViewState.isMarkdown) return;
  fileViewState.mode = "raw";
  renderActiveFileView();
};

async function loadFilePreview(sessionId: string, filePath: string, name: string): Promise<void> {
  const seq = ++fileLoadSeq;
  $("#fptitle").textContent = name;
  $("#fppath").textContent = filePath;
  $("#fp-mode").hidden = true;
  $("#filepane").hidden = false;
  if (small()) $("#app").classList.remove("show-side");
  const out = $("#filebody");
  note(out, "Loading file...");
  try {
    const res = await api<{ path: string; name: string; content: string; truncated: boolean }>(
      `/api/fs/read?session=${encodeURIComponent(sessionId)}&path=${encodeURIComponent(filePath)}`,
    );
    if (seq !== fileLoadSeq) return;
    const isMarkdown = isMarkdownFileName(res.name);
    fileViewState = {
      name: res.name,
      path: res.path,
      content: res.content,
      truncated: res.truncated,
      isMarkdown,
      mode: isMarkdown ? "rendered" : "raw",
    };
    $("#fptitle").textContent = res.name;
    $("#fppath").textContent = res.path;
    renderActiveFileView();
  } catch (err) {
    if (seq !== fileLoadSeq) return;
    fileViewState = null;
    const status = err instanceof ApiError ? err.status : null;
    note(out, status === 415 ? "Binary file preview is not supported." : "Could not load file preview.");
  }
}

async function renderFsDir(container: HTMLElement, sessionId: string, dirPath?: string): Promise<void> {
  const q = dirPath
    ? `/api/fs/list?session=${encodeURIComponent(sessionId)}&path=${encodeURIComponent(dirPath)}`
    : `/api/fs/list?session=${encodeURIComponent(sessionId)}`;
  const res = await api<{ root: string; entries: FsEntry[] }>(q);
  const list = el("div", "fs-list");
  if (!dirPath) {
    const rootName = res.root.split(/[/\\]/).filter(Boolean).pop() ?? res.root;
    container.replaceChildren(el("div", "fs-root", rootName), list);
  } else {
    container.replaceChildren(list);
  }
  if (res.entries.length === 0) {
    list.append(el("p", "note", "Empty folder"));
    return;
  }
  for (const item of res.entries) {
    if (item.is_dir) {
      const wrap = el("div");
      const btn = el("button", "fs-row fs-dir");
      btn.type = "button";
      let chev = icon("chevronRight");
      const folderIcon = icon("folder");
      const nameSpan = el("span", "fs-name", item.name);
      btn.append(chev, folderIcon, nameSpan);
      const children = el("div", "fs-children");
      children.hidden = true;
      let loaded = false;
      btn.onclick = async () => {
        const opening = children.hidden;
        children.hidden = !opening;
        const nextChev = icon(opening ? "chevronDown" : "chevronRight");
        chev.replaceWith(nextChev);
        chev = nextChev;
        if (opening && !loaded) {
          loaded = true;
          try {
            await renderFsDir(children, sessionId, item.path);
          } catch {
            note(children, "Unavailable");
          }
        }
      };
      wrap.append(btn, children);
      list.append(wrap);
    } else {
      const row = el("button", "fs-row fs-file");
      row.type = "button";
      row.append(icon("file"), el("span", "fs-name", item.name));
      row.onclick = () => {
        for (const prev of $("#fstree").querySelectorAll(".fs-row.is-active")) {
          prev.classList.remove("is-active");
        }
        row.classList.add("is-active");
        void loadFilePreview(sessionId, item.path, item.name);
      };
      list.append(row);
    }
  }
}

async function loadFilesTree(): Promise<void> {
  const box = $("#fstree");
  if (!token || !cur?.id) {
    note(box, "Open a session to browse its worktree.");
    return;
  }
  try {
    await renderFsDir(box, cur.id);
  } catch {
    note(box, "Worktree files are unavailable for this session.");
  }
}

const switchSidebar = (mode: "ws" | "files"): void => {
  const isFiles = mode === "files";
  $("#wsbox").hidden = isFiles;
  $("#fsbox").hidden = !isFiles;
  $("#rail-ws").setAttribute("aria-pressed", String(!isFiles));
  $("#rail-files").setAttribute("aria-pressed", String(isFiles));
  if (isFiles) void loadFilesTree();
};
$("#rail-ws").onclick = () => switchSidebar("ws");
$("#rail-files").onclick = () => switchSidebar("files");
$("#fs-refresh").onclick = () => void loadFilesTree();
const root = document.documentElement;
for (const [sel, v, dir] of [
  ["#side", "--w-side", 1],
  ["#term-pane", "--w-term", 1],
  ["#chg", "--w-chg", -1],
] as const) {
  const saved = localStorage.getItem(`oxidite.${v}`);
  if (saved) root.style.setProperty(v, saved);
  const g = el("div", "grip");
  g.dataset.edge = dir === 1 ? "r" : "l";
  g.setAttribute("role", "separator");
  g.setAttribute("aria-orientation", "vertical");
  $(sel).append(g);
  g.onpointerdown = (e) => {
    g.setPointerCapture(e.pointerId);
    const x0 = e.clientX,
      w0 = $(sel).getBoundingClientRect().width;
    g.onpointermove = (m) => root.style.setProperty(v, `${Math.max(180, Math.min(900, w0 + dir * (m.clientX - x0)))}px`);
    g.onpointerup = () => {
      g.onpointermove = null;
      localStorage.setItem(`oxidite.${v}`, root.style.getPropertyValue(v));
      fitVisible();
    };
  };
}

$("#new").onclick = () => {
  if (!shellsProbed && token) void probeShellCapabilities();
  applyShellAvailabilityUi();
  $<HTMLDialogElement>("#newdlg").showModal();
};

const sshAuthSel = $<HTMLSelectElement>("#ssh-auth-kind");
function syncSshAuthFields(): void {
  const k = sshAuthSel.value;
  $("#ssh-pw-wrap").hidden = k !== "password";
  $("#ssh-key-wrap").hidden = k !== "publickey";
  $("#ssh-passphrase-wrap").hidden = k !== "publickey";
}
sshAuthSel.onchange = syncSshAuthFields;
syncSshAuthFields();

$<HTMLDialogElement>("#newdlg").onclose = () => {
  const dlg = $<HTMLDialogElement>("#newdlg");
  if (dlg.returnValue !== "go") {
    splitNext = false;
    return;
  }
  const f = new FormData($<HTMLFormElement>("#newdlg form")),
    s = (k: string): string => String(f.get(k) ?? "");
  const cwd = s("cwd").trim() || null;
  if (s("kind") === "ssh") {
    const auth = buildSshAuthSpec(s("auth_kind"), s("pw"), s("key_path"), s("passphrase"));
    openTab(
      {
        spec: {
          Ssh: {
            host: s("host").trim(),
            port: Number(s("port")) || 22,
            user: s("user").trim(),
            auth,
            multiplexer: f.get("tmux") ? "Tmux" : "None",
          },
        },
      },
      s("host").trim(),
      s("host").trim(),
    );
  } else if (s("kind") === "powershell") {
    const initLbl = cwd ? (cwd.split(/[/\\]/).filter(Boolean).pop() ?? "powershell") : "powershell";
    openTab({ spec: { LocalPty: { shell: "SystemPowerShell", cwd } } }, initLbl, "powershell");
  } else {
    const initLbl = cwd ? (cwd.split(/[/\\]/).filter(Boolean).pop() ?? "shell") : "shell";
    openTab({ spec: { LocalPty: { shell: null, cwd } } }, initLbl, "shell");
  }
};

const gate = $<HTMLDialogElement>("#gate");
const gateForm = $<HTMLFormElement>("#gate form");
const gateErr = $("#gate-err");
const vaultDlg = $<HTMLDialogElement>("#vault-unlock-dlg");
const vaultForm = $<HTMLFormElement>("#vault-unlock-dlg form");
const vaultErr = $("#vault-err");
const devicesDlg = $<HTMLDialogElement>("#devicesdlg");

function applyAuthenticatedToken(newToken: string): void {
  token = newToken;
  authPrompted = false;
  for (const t of tabs) {
    t.conn.setToken(newToken);
    t.conn.reconnectNow();
  }
  void refresh();
  void loadChanges();
}

async function tryStoreInVault(deviceId: string | null, tokVal: string, label: string, pin: string): Promise<string | null> {
  if (!pin) return null;
  if (pin.length < 6) return "PIN must be at least 6 characters.";
  if (!deviceId) return null;
  if (!DeviceVault.isAvailable() || !window.isSecureContext) {
    $("#msg").textContent = "Browser vault requires HTTPS or localhost; token held in memory only for this tab.";
    return null;
  }
  await DeviceVault.enroll(deviceId, tokVal, label, pin);
  return null;
}

function bindVisibilityToggle(btnSel: string, inputSel: string): void {
  const btn = document.querySelector<HTMLButtonElement>(btnSel);
  const input = document.querySelector<HTMLInputElement>(inputSel);
  if (!btn || !input) return;
  const sync = (): void => { btn.textContent = input.type === "password" ? "Show" : "Hide"; };
  sync();
  btn.onclick = () => {
    const start = input.selectionStart;
    const end = input.selectionEnd;
    input.type = input.type === "password" ? "text" : "password";
    sync();
    input.focus();
    if (start !== null && end !== null) {
      try { input.setSelectionRange(start, end); } catch { /* ignore */ }
    }
  };
}
bindVisibilityToggle("#tok-vis", "#tok");
bindVisibilityToggle("#pin-vis", "#tok-pin");
bindVisibilityToggle("#vault-pin-vis", "#vault-pin");

gateForm.onsubmit = async (e) => {
  e.preventDefault();
  gateErr.hidden = true;
  const raw = $<HTMLInputElement>("#tok").value.trim();
  const label = $<HTMLInputElement>("#tok-label").value.trim() || defaultDeviceLabel();
  const pin = $<HTMLInputElement>("#tok-pin").value;
  if (!raw) {
    gateErr.textContent = "Enter a token or pairing grant.";
    gateErr.hidden = false;
    return;
  }
  if (pin && pin.length < 6) {
    gateErr.textContent = "Local vault PIN must be at least 6 characters (or leave blank).";
    gateErr.hidden = false;
    return;
  }

  try {
    // 1. Try treating input directly as a single-use pairing grant (bootstrap or issued eg_...)
    const enrollRes = await fetch("/api/devices/enroll", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ grant: raw, label }),
    });
    if (enrollRes.status >= 500) {
      gateErr.textContent = `Gateway error (HTTP ${enrollRes.status}). Make sure oxidited is running on the same port as this UI / Vite proxy.`;
      gateErr.hidden = false;
      return;
    }
    if (enrollRes.ok) {
      const enrolled = (await enrollRes.json()) as { device_id: string; token: string; label: string };
      await tryStoreInVault(enrolled.device_id, enrolled.token, enrolled.label, pin);
      $<HTMLInputElement>("#tok").value = "";
      $<HTMLInputElement>("#tok-pin").value = "";
      gate.close();
      applyAuthenticatedToken(enrolled.token);
      return;
    }

    // 2. Otherwise verify raw as an existing bearer / od1 device / gateway token
    const verifyRes = await fetch("/api/sessions", {
      headers: { Authorization: `Bearer ${raw}` },
    });
    if (!verifyRes.ok) {
      gateErr.textContent =
        verifyRes.status === 401
          ? "Token rejected. Check the startup token or ask another paired device for a fresh pairing link."
          : `Gateway error (HTTP ${verifyRes.status}). Make sure oxidited is running on the same port as this UI / Vite proxy.`;
      gateErr.hidden = false;
      return;
    }

    let finalToken = raw;
    let finalDeviceId: string | null = raw.startsWith("od1.") ? raw.split(".")[1] ?? null : null;
    if (!raw.startsWith("od1.")) {
      // Automatically convert a bootstrap/gateway token into a unique persistent token for this device
      try {
        const grantRes = await fetch("/api/devices/grant", {
          method: "POST",
          headers: { Authorization: `Bearer ${raw}`, "Content-Type": "application/json" },
          body: JSON.stringify({ ttl_secs: 300 }),
        });
        if (grantRes.ok) {
          const { grant } = (await grantRes.json()) as { grant: string };
          const devRes = await fetch("/api/devices/enroll", {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify({ grant, label }),
          });
          if (devRes.ok) {
            const enrolled = (await devRes.json()) as { device_id: string; token: string };
            finalToken = enrolled.token;
            finalDeviceId = enrolled.device_id;
          }
        }
      } catch {
        // Fallback: keep raw verified token in memory
      }
    }

    await tryStoreInVault(finalDeviceId, finalToken, label, pin);
    $<HTMLInputElement>("#tok").value = "";
    $<HTMLInputElement>("#tok-pin").value = "";
    gate.close();
    applyAuthenticatedToken(finalToken);
  } catch {
    gateErr.textContent = "Cannot reach the gateway server. Is oxidited running?";
    gateErr.hidden = false;
  }
};

gate.oncancel = () => {
  authPrompted = false;
  renderOfflineFallback("Gateway locked. Click Unlock / Pair when ready.", false);
};

vaultForm.onsubmit = async (e) => {
  e.preventDefault();
  vaultErr.hidden = true;
  const pinInput = $<HTMLInputElement>("#vault-pin");
  try {
    const unlocked = await DeviceVault.unlock(pinInput.value);
    pinInput.value = "";
    vaultDlg.close();
    applyAuthenticatedToken(unlocked);
  } catch (err) {
    vaultErr.textContent = err instanceof Error ? err.message : "Could not unlock device vault.";
    vaultErr.hidden = false;
  }
};

$("#vault-forget-btn").onclick = async () => {
  await DeviceVault.forget();
  $<HTMLInputElement>("#vault-pin").value = "";
  vaultDlg.close();
  openGate();
};

vaultDlg.oncancel = () => {
  authPrompted = false;
  renderOfflineFallback("Device vault locked. Click Unlock / Pair to enter your PIN or token.", false);
};

function openGate(errText = "", prefillGrant = ""): void {
  if (vaultDlg.open) vaultDlg.close();
  gateErr.hidden = !errText;
  gateErr.textContent = errText;
  $<HTMLInputElement>("#tok").value = prefillGrant;
  const lblInput = $<HTMLInputElement>("#tok-label");
  if (!lblInput.value) lblInput.value = defaultDeviceLabel();
  if (!gate.open) gate.showModal();
  $<HTMLInputElement>(prefillGrant ? "#tok-label" : "#tok").focus();
}

async function promptUnlock(errText = "", forceGate = false): Promise<void> {
  if (authPrompted && !forceGate) return;
  authPrompted = true;
  if (!forceGate && DeviceVault.isAvailable() && window.isSecureContext && (await DeviceVault.hasEnrolledDevice())) {
    const info = await DeviceVault.getEnrolledInfo();
    $("#vault-device-label").textContent = info
      ? `Paired device: ${info.label} (${info.deviceId.slice(0, 8)})`
      : "Unlock your encrypted local device token.";
    vaultErr.hidden = !errText;
    vaultErr.textContent = errText;
    if (!vaultDlg.open) vaultDlg.showModal();
    $<HTMLInputElement>("#vault-pin").focus();
    return;
  }
  openGate(errText);
}

type ListedDevice = {
  device_id: string;
  user_id: string;
  label: string;
  created_at_secs: number;
  last_seen_at_secs: number | null;
  revoked: boolean;
};

async function loadDevicesList(): Promise<void> {
  const listEl = $("#devices-list");
  const devErr = $("#dev-err");
  devErr.hidden = true;
  if (!token) {
    note(listEl, "Unlock first to view paired devices.");
    return;
  }
  try {
    const list = await api<ListedDevice[]>("/api/devices");
    if (!list.length) {
      note(listEl, "No paired devices yet. Enter or generate a pairing link below.");
      return;
    }
    listEl.replaceChildren(
      ...list.map((d) => {
        const row = el("div", "dev-row");
        const meta = el("div", "dev-meta");
        meta.append(
          el("strong", "", `${d.label} (${d.device_id.slice(0, 8)})`),
          el("span", "", d.revoked ? "Revoked" : `Active • ${d.user_id}`),
        );
        const btn = el("button", "btn alt", d.revoked ? "Revoked" : "Revoke");
        btn.type = "button";
        btn.disabled = d.revoked;
        btn.onclick = async () => {
          try {
            await api(`/api/devices/${encodeURIComponent(d.device_id)}`, undefined, "DELETE");
            await loadDevicesList();
          } catch {
            devErr.textContent = "Could not revoke device.";
            devErr.hidden = false;
          }
        };
        row.append(meta, btn);
        return row;
      }),
    );
  } catch {
    note(listEl, "Could not fetch paired devices. Ensure gateway is reachable.");
  }
}

$("#devs").onclick = () => {
  if (!token) {
    void promptUnlock("", true);
    return;
  }
  $("#pair-result").hidden = true;
  if (!devicesDlg.open) devicesDlg.showModal();
  void loadDevicesList();
};

$("#pair-new-btn").onclick = async () => {
  const devErr = $("#dev-err");
  devErr.hidden = true;
  try {
    const res = await api<{ grant: string; expires_in_secs: number; share_url?: string }>("/api/devices/grant", { ttl_secs: 300 });
    $("#pair-ttl").textContent = `${Math.max(1, Math.round(res.expires_in_secs / 60))}m`;
    $<HTMLInputElement>("#pair-link").value = res.share_url || buildPairShareUrl(location.origin, location.pathname, res.grant);
    $<HTMLInputElement>("#pair-grant").value = res.grant;
    $("#pair-result").hidden = false;
  } catch {
    devErr.textContent = "Failed to generate pairing link. Verify gateway access.";
    devErr.hidden = false;
  }
};

$("#copy-pair-link").onclick = () => {
  void navigator.clipboard?.writeText($<HTMLInputElement>("#pair-link").value);
};
$("#copy-pair-grant").onclick = () => {
  void navigator.clipboard?.writeText($<HTMLInputElement>("#pair-grant").value);
};
$("#switch-tok-btn").onclick = () => {
  devicesDlg.close();
  openGate();
};

async function initAuth(): Promise<void> {
  const pairGrant = parsePairGrantFromHash(location.hash);
  if (pairGrant) {
    history.replaceState(null, "", location.pathname + location.search);
    openGate("Pairing link detected. Give this device a name and optional PIN.", pairGrant);
    return;
  }
  await promptUnlock();
}

installVisualViewportSync();
if (typeof matchMedia === "function") {
  matchMedia("(max-width: 900px)").addEventListener("change", () => renderTabs());
}
document.addEventListener("visibilitychange", () => {
  if (document.visibilityState === "visible" && token && !gate.open && !vaultDlg.open) {
    void refresh();
  }
});

$("#msg").onclick = () => {
  if (cur && !cur.hasWriterControl && cur.b !== "down") {
    // Do not mark control as regained yet: the server's Presence message confirms (or denies) it.
    cur.conn.takeoverWriter();
    cur.msg = "Reclaiming write control...";
    renderTabs();
    paint();
  }
};

renderActivity();
$("#ws-fallback").hidden = true;
void loadChanges();
void initAuth();

// Keep the sidebar and active session folder/git state live while the tab is visible.
setInterval(() => {
  if (token && !gate.open && !vaultDlg.open && document.visibilityState === "visible") void refresh();
}, 2500);
