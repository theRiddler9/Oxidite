#!/usr/bin/env python3
"""Headless-Chrome end-to-end smoke test for a running `oxidited` gateway.

Covers: login + local session, Warp-style blocks, live autocomplete (no 429s while typing),
full-screen programs hiding the input bar, the #filepane Rendered/Raw markdown viewer, the
project banner, the 390x844 mobile layout (keyboard bar, drawer closes after "new session",
16px touch inputs), and WebSocket Origin-spoof rejection.

Usage (gateway already running with OXIDITE_TOKEN set and OXIDITE_ALLOW_LOCAL_PTY=1):
    OXIDITE_E2E_TOKEN=... python3 scripts/e2e/ui-smoke.py
Env: OXIDITE_E2E_URL (default http://127.0.0.1:3030), OXIDITE_E2E_BROWSER (path to Chrome;
default: Playwright's bundled Chromium), OXIDITE_E2E_OUT (screenshot dir; default: ./e2e-out).
Exit status is non-zero if any check fails.
"""
import os
import socket
import sys
import tempfile
from urllib.parse import urlparse

from playwright.sync_api import sync_playwright

URL = os.environ.get("OXIDITE_E2E_URL", "http://127.0.0.1:3030").rstrip("/") + "/"
TOKEN = os.environ.get("OXIDITE_E2E_TOKEN", "")
BROWSER = os.environ.get("OXIDITE_E2E_BROWSER") or None
OUT = os.environ.get("OXIDITE_E2E_OUT", "e2e-out")
os.makedirs(OUT, exist_ok=True)

results = []


def check(name, ok, extra=""):
    results.append(("PASS" if ok else "FAIL", name, str(extra)[:160]))


def login(page):
    page.goto(URL)
    page.wait_for_selector("#gate[open]", timeout=10000)
    page.fill("#tok", TOKEN)
    page.click("#gate button[value=go]")
    page.wait_for_selector("#gate", state="hidden", timeout=10000)


def new_session(page, open_drawer=False):
    if open_drawer:
        page.click("#sb")
    page.click("#new")
    page.wait_for_selector("#newdlg", state="visible")
    page.click("#newdlg button[value=go]")
    page.wait_for_selector(".pane:not([hidden]) .composer textarea", timeout=15000)
    page.wait_for_timeout(1200)


def launch(p):
    return p.chromium.launch(executable_path=BROWSER, headless=True, args=["--no-sandbox"])


def desktop(p):
    b = launch(p)
    page = b.new_context(viewport={"width": 1440, "height": 900}).new_page()
    errors, statuses = [], []
    page.on("pageerror", lambda e: errors.append(str(e)))
    page.on("response", lambda r: statuses.append((r.status, r.url)))
    login(page)
    new_session(page)
    page.screenshot(path=f"{OUT}/desktop-fresh.png")
    ta = page.locator(".pane:not([hidden]) .composer textarea")

    ta.type("echo first-output")
    page.keyboard.press("Enter")
    page.wait_for_timeout(1500)
    ta.type("pwd")
    page.keyboard.press("Enter")
    page.wait_for_timeout(1500)
    check("blocks render for commands", page.locator(".pane:not([hidden]) .blk").count() >= 2)
    outs = " ".join(page.locator(".pane:not([hidden]) .blk-out").all_inner_texts())
    check("block shows command output", "first-output" in outs, outs[-80:])

    ta.type("ls READ")
    page.wait_for_timeout(900)
    page.keyboard.press("Tab")
    page.wait_for_timeout(500)
    check("Tab applies path completion", ta.input_value().startswith("ls README"), ta.input_value())
    page.keyboard.press("Control+c")

    ta.type("less /etc/os-release" if os.path.exists("/etc/os-release") else "less README.md")
    page.keyboard.press("Enter")
    page.wait_for_timeout(1500)
    check("input bar hides in full-screen programs",
          page.get_attribute(".pane:not([hidden]) .composer", "data-raw") == "1")
    page.keyboard.type("q")
    page.wait_for_timeout(1000)
    check("input bar returns after quit",
          page.get_attribute(".pane:not([hidden]) .composer", "data-raw") == "0")

    before = len([s for s in statuses if s[0] == 429])
    ta.focus()
    for _ in range(8):
        ta.type("abcdefghijklmnopqrstuvwxyz0123456789", delay=130)
        page.keyboard.press("Control+c")
    page.wait_for_timeout(800)
    n429 = len([s for s in statuses if s[0] == 429]) - before
    check("no 429s while typing normally", n429 == 0, f"429s={n429}")

    # Project banner (desktop)
    check("project banner renders", page.locator(".proj-banner").count() > 0)

    # Markdown file viewer: Rendered / Raw
    page.click("#rail-files")
    page.wait_for_selector("#fstree .fs-row", timeout=10000)
    md_row = page.locator("#fstree .fs-file", has_text="README.md").first
    if md_row.count():
        md_row.click()
        page.wait_for_selector("#filepane:not([hidden])", timeout=10000)
        page.wait_for_selector("#fp-mode:not([hidden])", timeout=10000)
        check("rendered markdown view shows .md-view", page.locator("#filebody .md-view").count() == 1)
        page.screenshot(path=f"{OUT}/desktop-filepane-rendered.png")
        page.click("#fp-raw")
        page.wait_for_timeout(300)
        check("raw view replaces rendered view",
              page.locator("#filebody .md-view").count() == 0
              and page.get_attribute("#fp-raw", "aria-selected") == "true")
        page.click("#fp-rendered")
        page.wait_for_timeout(300)
        check("switching back to Rendered works", page.locator("#filebody .md-view").count() == 1)
    else:
        check("README.md present in worktree for viewer test", False, "run from a checkout with README.md")

    check("no page errors (desktop)", not errors, errors)
    bad = [s for s in statuses if s[0] >= 500]
    check("no 5xx responses (desktop)", not bad, bad[:3])
    b.close()


def mobile(p):
    b = launch(p)
    ctx = b.new_context(viewport={"width": 390, "height": 844}, device_scale_factor=2,
                        is_mobile=True, has_touch=True)
    page = ctx.new_page()
    errors = []
    page.on("pageerror", lambda e: errors.append(str(e)))
    login(page)
    page.wait_for_timeout(800)
    check("mobile: no horizontal scroll (empty state)",
          not page.evaluate("document.documentElement.scrollWidth > innerWidth"))
    check("mobile: empty-state hint does not point at hidden '+'",
          page.locator(".empty-narrow").count() > 0)
    new_session(page, open_drawer=True)
    page.screenshot(path=f"{OUT}/mobile-session.png")
    check("mobile: drawer closes after creating a session",
          not page.evaluate("document.querySelector('#app').classList.contains('show-side')"))
    check("mobile: key bar visible", page.locator(".kbd-bar").first.is_visible())
    check("mobile: no horizontal scroll (session)",
          not page.evaluate("document.documentElement.scrollWidth > innerWidth"))
    fs = page.evaluate("parseFloat(getComputedStyle(document.querySelector('.composer textarea')).fontSize)")
    check("mobile: composer font >= 16px (no iOS focus zoom)", fs >= 16, fs)
    check("mobile: web manifest linked",
          bool(page.evaluate("document.querySelector('link[rel=manifest]')?.href")))
    check("no page errors (mobile)", not errors, errors)
    b.close()


def origin_spoof():
    u = urlparse(URL)
    host, port = u.hostname, u.port or (443 if u.scheme == "https" else 80)
    if u.scheme == "https":
        check("origin spoof rejected (skipped: https)", True, "run against http loopback")
        return
    req = (
        f"GET /ws HTTP/1.1\r\nHost: {host}:{port}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n"
        "Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n"
        "Origin: https://evil.example\r\n\r\n"
    )
    with socket.create_connection((host, port), timeout=5) as s:
        s.sendall(req.encode())
        status = s.recv(256).split(b"\r\n", 1)[0].decode(errors="replace")
    check("WebSocket upgrade from a spoofed Origin is rejected (403)", " 403 " in status, status)


def main():
    if not TOKEN:
        print("set OXIDITE_E2E_TOKEN to the gateway's OXIDITE_TOKEN", file=sys.stderr)
        return 2
    with sync_playwright() as p:
        desktop(p)
        mobile(p)
    origin_spoof()
    for r in results:
        print(*r)
    failed = [r for r in results if r[0] == "FAIL"]
    print(f"\n{len(results) - len(failed)}/{len(results)} checks passed; screenshots in {OUT}/")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
