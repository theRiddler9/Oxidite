import { applyStickyCtrl } from "./sticky-ctrl";

export interface MobileKbdBarOptions {
  /** Primary input element (composer textarea). */
  textarea: HTMLTextAreaElement;
  /** Sends raw terminal bytes directly to the active PTY session. */
  sendRaw: (data: string) => void;
  /** Inserts text into the composer (or forwards to PTY in raw mode). */
  insertText: (text: string) => void;
  /** Triggered when Tab is tapped in composer mode. */
  onTab?: () => void;
  /** Triggered when Shift+Enter is tapped in composer mode. */
  onShiftEnter?: () => void;
  /** Triggered when Scroll-Lock / Read-Only mode toggles. */
  onScrollLockChange?: (locked: boolean) => void;
  /** Returns true when the terminal is in raw/alternate-screen mode (e.g. vim). */
  isRawMode?: () => boolean;
}

export interface MobileKbdBarHandle {
  element: HTMLElement;
  rawProxy: HTMLTextAreaElement;
  isCtrlLatched: () => boolean;
  clearCtrl: () => void;
  isScrollLocked: () => boolean;
  setScrollLocked: (locked: boolean) => void;
  focusActiveInput: () => void;
  destroy: () => void;
}

const SYMBOLS = ["\\", "/", "-", "|", "..", "~", "_", ";", "&"];

export function createMobileKbdBar(opts: MobileKbdBarOptions): MobileKbdBarHandle {
  let ctrlLatched = false;
  let scrollLocked = false;
  let repeatTimer: number | null = null;
  let repeatInterval: number | null = null;

  // Off-screen focusable textarea proxy so raw mode (vim) keeps phone keyboard open.
  const rawProxy = document.createElement("textarea");
  rawProxy.className = "oxidite-raw-kbd-proxy";
  rawProxy.setAttribute("aria-label", "Raw terminal keyboard proxy");
  configureMobileInputAttrs(rawProxy);
  configureMobileInputAttrs(opts.textarea);

  const bar = document.createElement("div");
  bar.className = "kbd-bar";
  bar.setAttribute("role", "toolbar");
  bar.setAttribute("aria-label", "Mobile terminal keys");

  const fixedRow = document.createElement("div");
  fixedRow.className = "kbd-bar-fixed";

  const symbolStrip = document.createElement("div");
  symbolStrip.className = "kbd-bar-symbols";

  const ctrlBtn = makeFixedButton("Ctrl", "Toggle sticky Ctrl", () => {
    setCtrlLatched(!ctrlLatched);
  });
  ctrlBtn.classList.add("kbd-btn-modifier");

  const lockBtn = makeFixedButton("Lock", "Toggle Read-Only Scroll Lock", () => {
    setScrollLocked(!scrollLocked);
  });
  lockBtn.classList.add("kbd-btn-lock");

  const setCtrlLatched = (next: boolean) => {
    ctrlLatched = next;
    ctrlBtn.classList.toggle("is-active", ctrlLatched);
    ctrlBtn.setAttribute("aria-pressed", String(ctrlLatched));
  };

  const setScrollLocked = (next: boolean) => {
    scrollLocked = next;
    lockBtn.classList.toggle("is-active", scrollLocked);
    lockBtn.setAttribute("aria-pressed", String(scrollLocked));
    lockBtn.textContent = scrollLocked ? "Locked" : "Lock";
    opts.textarea.readOnly = scrollLocked;
    rawProxy.readOnly = scrollLocked;
    if (scrollLocked) {
      opts.textarea.blur();
      rawProxy.blur();
    }
    opts.onScrollLockChange?.(scrollLocked);
  };

  const focusActiveInput = () => {
    if (scrollLocked) return;
    if (opts.isRawMode?.()) {
      rawProxy.focus({ preventScroll: true });
    } else {
      opts.textarea.focus({ preventScroll: true });
    }
  };

  function makeFixedButton(
    label: string,
    title: string,
    onTrigger: () => void,
    repeatable = false
  ): HTMLButtonElement {
    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "kbd-btn";
    btn.textContent = label;
    btn.title = title;
    btn.setAttribute("aria-label", title);

    let pointerHandled = false;

    const stopRepeat = () => {
      if (repeatTimer !== null) {
        window.clearTimeout(repeatTimer);
        repeatTimer = null;
      }
      if (repeatInterval !== null) {
        window.clearInterval(repeatInterval);
        repeatInterval = null;
      }
    };

    // Prevent soft-keyboard blur on touch/mouse press
    btn.addEventListener("pointerdown", (e) => {
      e.preventDefault();
      pointerHandled = true;
      if (scrollLocked && btn !== lockBtn) return;
      onTrigger();
      if (repeatable) {
        stopRepeat();
        repeatTimer = window.setTimeout(() => {
          repeatInterval = window.setInterval(() => {
            if (scrollLocked) {
              stopRepeat();
              return;
            }
            onTrigger();
          }, 75);
        }, 360);
      }
    });

    // Keyboard activation (Enter/Space) and screen-reader clicks fire `click` without `pointerdown`
    btn.addEventListener("click", (e) => {
      if (pointerHandled) {
        pointerHandled = false;
        return;
      }
      e.preventDefault();
      if (scrollLocked && btn !== lockBtn) return;
      onTrigger();
    });

    btn.addEventListener("pointerup", stopRepeat);
    btn.addEventListener("pointercancel", stopRepeat);
    btn.addEventListener("pointerleave", stopRepeat);

    return btn;
  }

  const runUnlatchingAction = (fn: () => void) => () => {
    if (ctrlLatched) setCtrlLatched(false);
    fn();
  };

  // Fixed action buttons
  fixedRow.append(
    makeFixedButton(
      "Tab",
      "Tab autocomplete",
      runUnlatchingAction(() => {
        if (!opts.isRawMode?.() && opts.onTab) {
          opts.onTab();
        } else {
          opts.sendRaw("\t");
        }
      }),
    ),
    ctrlBtn,
    makeFixedButton("^C", "Send Ctrl+C (SIGINT)", runUnlatchingAction(() => opts.sendRaw("\x03"))),
    makeFixedButton("Esc", "Send Escape", runUnlatchingAction(() => opts.sendRaw("\x1b"))),
    makeFixedButton(
      "⇧↵",
      "Shift+Enter (newline)",
      runUnlatchingAction(() => {
        if (opts.onShiftEnter) {
          opts.onShiftEnter();
        } else {
          opts.insertText("\n");
        }
      }),
    ),
    makeFixedButton("↑", "Arrow Up", runUnlatchingAction(() => opts.sendRaw("\x1b[A")), true),
    makeFixedButton("↓", "Arrow Down", runUnlatchingAction(() => opts.sendRaw("\x1b[B")), true),
    makeFixedButton("←", "Arrow Left", runUnlatchingAction(() => opts.sendRaw("\x1b[D")), true),
    makeFixedButton("→", "Arrow Right", runUnlatchingAction(() => opts.sendRaw("\x1b[C")), true)
  );

  // Paste button: invokes navigator.clipboard.readText() synchronously in click/pointerup
  // to satisfy iOS Safari user-gesture restrictions.
  const pasteBtn = document.createElement("button");
  pasteBtn.type = "button";
  pasteBtn.className = "kbd-btn";
  pasteBtn.textContent = "Paste";
  pasteBtn.title = "Paste from clipboard";
  pasteBtn.setAttribute("aria-label", "Paste from clipboard");
  pasteBtn.addEventListener("mousedown", (e) => e.preventDefault());
  pasteBtn.addEventListener("click", (e) => {
    e.preventDefault();
    if (ctrlLatched) setCtrlLatched(false);
    if (scrollLocked || !navigator.clipboard?.readText) return;
    navigator.clipboard
      .readText()
      .then((text) => {
        if (!text) return;
        if (opts.isRawMode?.()) {
          opts.sendRaw(text);
        } else {
          opts.insertText(text);
        }
        focusActiveInput();
      })
      .catch(() => {
        // Ignore clipboard permission denial silently
      });
  });
  fixedRow.append(pasteBtn, lockBtn);

  // Scrollable symbol strip: DO NOT call e.preventDefault() on pointerdown so horizontal
  // touch panning works smoothly on iOS Safari & Android Chrome.
  for (const sym of SYMBOLS) {
    const chip = document.createElement("button");
    chip.type = "button";
    chip.className = "kbd-btn kbd-chip";
    chip.textContent = sym;
    chip.title = `Insert ${sym}`;
    chip.setAttribute("aria-label", `Insert ${sym}`);

    let startX = 0;
    let startY = 0;
    let activePointerId: number | null = null;
    let suppressNextClick = false;

    const triggerChip = () => {
      if (scrollLocked) return;
      if (ctrlLatched) setCtrlLatched(false);
      if (opts.isRawMode?.()) {
        opts.sendRaw(sym);
      } else {
        opts.insertText(sym);
      }
      focusActiveInput();
    };

    chip.addEventListener("mousedown", (e) => e.preventDefault());
    chip.addEventListener("pointerdown", (e) => {
      activePointerId = e.pointerId;
      startX = e.clientX;
      startY = e.clientY;
      suppressNextClick = false;
    });
    chip.addEventListener("pointerup", (e) => {
      if (activePointerId !== e.pointerId) return;
      activePointerId = null;
      suppressNextClick = true;
      const dx = Math.abs(e.clientX - startX);
      const dy = Math.abs(e.clientY - startY);
      if (dx < 8 && dy < 8) {
        triggerChip();
      }
    });
    chip.addEventListener("pointercancel", () => {
      activePointerId = null;
      suppressNextClick = false;
    });
    chip.addEventListener("click", (e) => {
      if (suppressNextClick) {
        suppressNextClick = false;
        return;
      }
      e.preventDefault();
      triggerChip();
    });

    symbolStrip.append(chip);
  }

  bar.append(fixedRow, symbolStrip);

  // Sticky Ctrl interception (supports both standard beforeinput AND Android Gboard IME
  // composition where beforeinput.preventDefault() is ignored).
  const attachCtrlInterceptor = (el: HTMLTextAreaElement, isProxy: boolean) => {
    let handledInBeforeInput = false;
    let isComposing = false;
    // Text already forwarded to the PTY for the in-flight IME composition (raw proxy only).
    let composingSent = "";

    // Gboard keeps a word "composing" until a space, which would delay raw-mode typing (vim, REPLs).
    // Forward composition text as it grows; if the IME rewrites it (autocorrect), erase what was sent
    // with DEL and retype, so the PTY always ends up with the committed text.
    const forwardComposition = (current: string) => {
      if (scrollLocked || current === composingSent) return;
      if (current.startsWith(composingSent)) {
        opts.sendRaw(current.slice(composingSent.length));
      } else {
        opts.sendRaw("\x7f".repeat([...composingSent].length) + current);
      }
      composingSent = current;
    };

    const processCommittedCtrlOrFlush = (inserted: string): boolean => {
      if (!ctrlLatched) return false;
      setCtrlLatched(false);
      // A multi-character commit (a whole IME word) has no single control character; applying Ctrl to
      // its first letter while deleting one char would corrupt the text, so just drop the latch.
      if ([...inserted].length !== 1) return false;
      const ctrlByte = applyStickyCtrl(inserted);
      if (!ctrlByte) return false;
      if (isProxy) {
        el.value = "";
      } else {
        const pos = el.selectionStart ?? el.value.length;
        if (pos > 0) {
          el.value = el.value.slice(0, pos - 1) + el.value.slice(pos);
          el.setSelectionRange(pos - 1, pos - 1);
        }
      }
      opts.sendRaw(ctrlByte);
      return true;
    };

    const onBeforeInput = (e: InputEvent) => {
      handledInBeforeInput = false;
      // Gboard often reports Backspace/Enter as key "Unidentified" (keydown never matches) and sends no
      // `input` for Backspace on an empty proxy; the standardized inputType is reliable.
      if (isProxy && !scrollLocked && !isComposing && !e.isComposing) {
        if (e.inputType === "deleteContentBackward") {
          e.preventDefault();
          opts.sendRaw("\x7f");
          return;
        }
        if (e.inputType === "insertLineBreak" || e.inputType === "insertParagraph") {
          e.preventDefault();
          opts.sendRaw("\r");
          return;
        }
      }
      if (!ctrlLatched || !e.data || e.data.length === 0) return;

      if (e.isComposing || isComposing) return;

      const ctrlByte = applyStickyCtrl(e.data);
      if (e.cancelable && ctrlByte) {
        e.preventDefault();
        handledInBeforeInput = true;
        setCtrlLatched(false);
        opts.sendRaw(ctrlByte);
      } else {
        setCtrlLatched(false);
      }
    };

    const onInput = (e: Event) => {
      const ie = e as InputEvent;
      if (ie.isComposing || isComposing) {
        // Sticky Ctrl needs the committed text, so it still waits for compositionend; otherwise the
        // raw proxy streams the composition live so typing is not delayed until the word ends.
        if (isProxy && !ctrlLatched) forwardComposition(el.value);
        return;
      }
      if (ctrlLatched && !handledInBeforeInput) {
        const inserted = ie.data ?? el.value.slice(-1);
        if (processCommittedCtrlOrFlush(inserted)) {
          return;
        }
      }
      handledInBeforeInput = false;

      // In raw proxy mode, forward normal phone typing straight to the PTY.
      if (isProxy && el.value.length > 0) {
        const payload = el.value;
        el.value = "";
        if (!scrollLocked) {
          opts.sendRaw(payload);
        }
      }
    };

    const onCompositionStart = () => {
      isComposing = true;
    };

    const onCompositionEnd = (e: CompositionEvent) => {
      isComposing = false;
      const committed = e.data || el.value;
      if (isProxy && !ctrlLatched && !scrollLocked && (composingSent !== "" || committed)) {
        forwardComposition(committed);
        composingSent = "";
        el.value = "";
        return;
      }
      composingSent = "";
      if (ctrlLatched && committed && processCommittedCtrlOrFlush(committed)) {
        return;
      }
      if (isProxy && el.value.length > 0) {
        const payload = el.value;
        el.value = "";
        if (!scrollLocked) {
          opts.sendRaw(payload);
        }
      }
    };

    const onKeyDown = (e: KeyboardEvent) => {
      if (
        ctrlLatched &&
        ["Tab", "Escape", "ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight"].includes(e.key)
      ) {
        setCtrlLatched(false);
      }
      if (isProxy && !scrollLocked && !isComposing && !e.isComposing) {
        if (e.key === "Enter") {
          e.preventDefault();
          opts.sendRaw("\r");
        } else if (e.key === "Backspace") {
          e.preventDefault();
          opts.sendRaw("\x7f");
        }
      }
    };

    el.addEventListener("beforeinput", onBeforeInput);
    el.addEventListener("input", onInput);
    el.addEventListener("compositionstart", onCompositionStart);
    el.addEventListener("compositionend", onCompositionEnd);
    el.addEventListener("keydown", onKeyDown);

    return () => {
      el.removeEventListener("beforeinput", onBeforeInput);
      el.removeEventListener("input", onInput);
      el.removeEventListener("compositionstart", onCompositionStart);
      el.removeEventListener("compositionend", onCompositionEnd);
      el.removeEventListener("keydown", onKeyDown);
    };
  };

  const cleanupPrimary = attachCtrlInterceptor(opts.textarea, false);
  const cleanupProxy = attachCtrlInterceptor(rawProxy, true);

  return {
    element: bar,
    rawProxy,
    isCtrlLatched: () => ctrlLatched,
    clearCtrl: () => setCtrlLatched(false),
    isScrollLocked: () => scrollLocked,
    setScrollLocked,
    focusActiveInput,
    destroy: () => {
      if (repeatTimer !== null) window.clearTimeout(repeatTimer);
      if (repeatInterval !== null) window.clearInterval(repeatInterval);
      repeatTimer = null;
      repeatInterval = null;
      cleanupPrimary();
      cleanupProxy();
      bar.remove();
      rawProxy.remove();
    },
  };
}

function configureMobileInputAttrs(el: HTMLTextAreaElement) {
  el.setAttribute("autocomplete", "off");
  el.setAttribute("autocorrect", "off");
  el.setAttribute("autocapitalize", "none");
  el.setAttribute("spellcheck", "false");
  el.setAttribute("enterkeyhint", "send");
}

/**
 * Syncs `--app-h` and `--vv-offset-top` CSS variables with `window.visualViewport`
 * using `requestAnimationFrame` coalescing and equality checks to prevent iOS/Android
 * scroll/animation jitter.
 */
export function installVisualViewportSync(): () => void {
  const root = document.documentElement;
  let rafId = 0;
  let lastH = "";
  let lastTop = "";

  const apply = () => {
    rafId = 0;
    const vv = window.visualViewport;
    const nextH = `${Math.round(vv ? vv.height : window.innerHeight)}px`;
    const offset = vv && vv.offsetTop > 1 ? Math.round(vv.offsetTop) : 0;
    const nextTop = `${offset}px`;
    if (nextH !== lastH) {
      lastH = nextH;
      root.style.setProperty("--app-h", nextH);
    }
    if (nextTop !== lastTop) {
      lastTop = nextTop;
      root.style.setProperty("--vv-offset-top", nextTop);
    }
  };

  const schedule = () => {
    if (rafId) return;
    if (typeof requestAnimationFrame === "function") {
      rafId = requestAnimationFrame(apply);
    } else {
      apply();
    }
  };

  apply();
  window.addEventListener("resize", schedule);
  window.visualViewport?.addEventListener("resize", schedule);
  window.visualViewport?.addEventListener("scroll", schedule);

  return () => {
    if (rafId && typeof cancelAnimationFrame === "function") {
      cancelAnimationFrame(rafId);
    }
    window.removeEventListener("resize", schedule);
    window.visualViewport?.removeEventListener("resize", schedule);
    window.visualViewport?.removeEventListener("scroll", schedule);
  };
}
