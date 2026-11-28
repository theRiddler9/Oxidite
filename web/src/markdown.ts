/**
 * Safe DOM-only Markdown renderer (zero `innerHTML`).
 * Supports headings, paragraphs, bold, inline code, fenced code blocks,
 * bullet/numbered lists, task checkboxes, blockquotes, horizontal rules, GFM pipe tables
 * (with `:---` / `:---:` / `---:` alignment), a Copy button on fenced code blocks, and http(s) links.
 */
export function renderMarkdownToFragment(src: string): DocumentFragment {
  const frag = document.createDocumentFragment();
  const lines = src.replace(/\r\n/g, "\n").split("\n");
  let i = 0;

  while (i < lines.length) {
    const line = lines[i] ?? "";

    // Fenced code block
    if (/^```/.test(line.trim())) {
      const codeLines: string[] = [];
      i += 1;
      while (i < lines.length && !/^```/.test((lines[i] ?? "").trim())) {
        codeLines.push(lines[i] ?? "");
        i += 1;
      }
      if (i < lines.length) i += 1;
      const pre = document.createElement("pre");
      pre.className = "md-pre";
      const code = document.createElement("code");
      const codeText = codeLines.join("\n");
      code.textContent = codeText;
      pre.append(code);
      // Wrapper keeps the Copy button pinned while a long line scrolls horizontally inside <pre>.
      const wrap = document.createElement("div");
      wrap.className = "md-code";
      wrap.append(pre, makeCopyButton(codeText));
      frag.append(wrap);
      continue;
    }

    // GFM pipe table: header row, delimiter row, then body rows
    if (isTableStart(lines, i)) {
      const header = splitTableRow(line);
      const aligns = parseDelimiterRow(lines[i + 1] ?? "") ?? [];
      i += 2;
      const table = document.createElement("table");
      table.className = "md-table";
      const thead = document.createElement("thead");
      const hr = document.createElement("tr");
      header.forEach((cell, c) => hr.append(makeCell("th", cell, aligns[c])));
      thead.append(hr);
      table.append(thead);
      const tbody = document.createElement("tbody");
      while (i < lines.length && (lines[i] ?? "").trim() !== "" && (lines[i] ?? "").includes("|")) {
        const cells = splitTableRow(lines[i] ?? "");
        const tr = document.createElement("tr");
        // Normalize to header width, as GFM does: pad short rows, drop extra cells.
        for (let c = 0; c < header.length; c += 1) tr.append(makeCell("td", cells[c] ?? "", aligns[c]));
        tbody.append(tr);
        i += 1;
      }
      table.append(tbody);
      const wrap = document.createElement("div");
      wrap.className = "md-table-wrap";
      wrap.append(table);
      frag.append(wrap);
      continue;
    }

    // Blank line
    if (!line.trim()) {
      i += 1;
      continue;
    }

    // Horizontal rule
    if (/^\s*(?:---+|___+|\*\*\*+)\s*$/.test(line)) {
      frag.append(document.createElement("hr"));
      i += 1;
      continue;
    }

    // Heading (#..####)
    const hm = /^(#{1,4})\s+(.+)$/.exec(line);
    if (hm && hm[1] && hm[2]) {
      const level = Math.min(4, hm[1].length) as 1 | 2 | 3 | 4;
      const hEl = document.createElement(`h${level}`);
      hEl.append(renderInline(hm[2]));
      frag.append(hEl);
      i += 1;
      continue;
    }

    // Blockquote
    if (/^\s*>\s?/.test(line)) {
      const qLines: string[] = [];
      while (i < lines.length && /^\s*>\s?/.test(lines[i] ?? "")) {
        qLines.push((lines[i] ?? "").replace(/^\s*>\s?/, ""));
        i += 1;
      }
      const bq = document.createElement("blockquote");
      bq.append(renderInline(qLines.join(" ")));
      frag.append(bq);
      continue;
    }

    // Unordered / Task list
    if (/^\s*[-*+]\s+/.test(line)) {
      const ul = document.createElement("ul");
      while (i < lines.length && /^\s*[-*+]\s+/.test(lines[i] ?? "")) {
        const rawItem = (lines[i] ?? "").replace(/^\s*[-*+]\s+/, "");
        const li = document.createElement("li");
        const taskMatch = /^\[([ xX])\]\s+(.*)$/.exec(rawItem);
        if (taskMatch && taskMatch[1] !== undefined && taskMatch[2] !== undefined) {
          const cb = document.createElement("input");
          cb.type = "checkbox";
          cb.disabled = true;
          cb.checked = taskMatch[1].toLowerCase() === "x";
          li.className = "md-task";
          li.append(cb, " ", renderInline(taskMatch[2]));
        } else {
          li.append(renderInline(rawItem));
        }
        ul.append(li);
        i += 1;
      }
      frag.append(ul);
      continue;
    }

    // Ordered list
    if (/^\s*\d+\.\s+/.test(line)) {
      const ol = document.createElement("ol");
      while (i < lines.length && /^\s*\d+\.\s+/.test(lines[i] ?? "")) {
        const rawItem = (lines[i] ?? "").replace(/^\s*\d+\.\s+/, "");
        const li = document.createElement("li");
        li.append(renderInline(rawItem));
        ol.append(li);
        i += 1;
      }
      frag.append(ol);
      continue;
    }

    // Paragraph (join contiguous plain lines)
    const pLines: string[] = [];
    while (
      i < lines.length &&
      (lines[i] ?? "").trim() !== "" &&
      !isTableStart(lines, i) &&
      !/^(#{1,4}\s+|```|\s*[-*+]\s+|\s*\d+\.\s+|\s*>\s?|\s*(?:---+|___+|\*\*\*+)\s*$)/.test(
        lines[i] ?? ""
      )
    ) {
      pLines.push((lines[i] ?? "").trim());
      i += 1;
    }
    const p = document.createElement("p");
    p.append(renderInline(pLines.join(" ")));
    frag.append(p);
  }

  return frag;
}

type Align = "left" | "center" | "right" | null;

/** Splits one table row on unescaped `|` outside inline code spans; trims outer pipes and cells. */
export function splitTableRow(row: string): string[] {
  const cells: string[] = [];
  let cur = "";
  let inCode = false;
  const text = row.trim();
  for (let k = 0; k < text.length; k += 1) {
    const ch = text[k] ?? "";
    if (ch === "\\" && text[k + 1] === "|") {
      cur += "|";
      k += 1;
    } else if (ch === "`") {
      inCode = !inCode;
      cur += ch;
    } else if (ch === "|" && !inCode) {
      cells.push(cur.trim());
      cur = "";
    } else {
      cur += ch;
    }
  }
  cells.push(cur.trim());
  if (text.startsWith("|")) cells.shift();
  if (text.endsWith("|") && !text.endsWith("\\|")) cells.pop();
  return cells;
}

function parseDelimiterRow(row: string): Align[] | null {
  if (!row.includes("-") || !row.includes("|") && !/^\s*:?-+:?\s*$/.test(row)) return null;
  const cells = splitTableRow(row);
  if (cells.length === 0) return null;
  const aligns: Align[] = [];
  for (const cell of cells) {
    if (!/^:?-+:?$/.test(cell)) return null;
    const l = cell.startsWith(":");
    const r = cell.endsWith(":");
    aligns.push(l && r ? "center" : r ? "right" : l ? "left" : null);
  }
  return aligns;
}

function isTableStart(lines: string[], i: number): boolean {
  const head = lines[i] ?? "";
  if (!head.includes("|")) return false;
  const aligns = parseDelimiterRow(lines[i + 1] ?? "");
  return aligns !== null && aligns.length === splitTableRow(head).length;
}

function makeCell(tag: "th" | "td", text: string, align: Align | undefined): HTMLElement {
  const cell = document.createElement(tag);
  if (align) cell.className = `md-al-${align}`;
  cell.append(renderInline(text));
  return cell;
}

function copyText(text: string): Promise<boolean> {
  if (navigator.clipboard?.writeText) {
    return navigator.clipboard.writeText(text).then(
      () => true,
      () => false
    );
  }
  // Fallback for non-secure contexts (plain-http LAN access), where navigator.clipboard is undefined.
  try {
    const ta = document.createElement("textarea");
    ta.value = text;
    ta.className = "md-copy-scratch";
    document.body.append(ta);
    ta.select();
    const ok = document.execCommand("copy");
    ta.remove();
    return Promise.resolve(ok);
  } catch {
    return Promise.resolve(false);
  }
}

function makeCopyButton(text: string): HTMLButtonElement {
  const btn = document.createElement("button");
  btn.type = "button";
  btn.className = "md-copy";
  btn.textContent = "Copy";
  btn.title = "Copy code";
  btn.setAttribute("aria-label", "Copy code to clipboard");
  let timer: ReturnType<typeof setTimeout> | undefined;
  btn.onclick = () => {
    void copyText(text).then((ok) => {
      btn.textContent = ok ? "Copied" : "Copy failed";
      if (timer) clearTimeout(timer);
      timer = setTimeout(() => {
        btn.textContent = "Copy";
      }, 1500);
    });
  };
  return btn;
}

const INLINE_TOK = /(`[^`]+`|\*\*[^*]+\*\*|\[[^\]]+\]\([^)]+\))/g;

function renderInline(text: string): DocumentFragment {
  const frag = document.createDocumentFragment();
  let lastIdx = 0;
  for (const m of text.matchAll(INLINE_TOK)) {
    const at = m.index ?? 0;
    const tok = m[0];
    if (at > lastIdx) frag.append(text.slice(lastIdx, at));
    if (tok.startsWith("`") && tok.endsWith("`")) {
      const code = document.createElement("code");
      code.textContent = tok.slice(1, -1);
      frag.append(code);
    } else if (tok.startsWith("**") && tok.endsWith("**")) {
      const strong = document.createElement("strong");
      strong.textContent = tok.slice(2, -2);
      frag.append(strong);
    } else if (tok.startsWith("[")) {
      const linkMatch = /^\[([^\]]+)\]\(([^)]+)\)$/.exec(tok);
      if (linkMatch && linkMatch[1] !== undefined && linkMatch[2] !== undefined) {
        const href = linkMatch[2].trim();
        if (/^https?:\/\//i.test(href)) {
          const a = document.createElement("a");
          a.href = href;
          a.target = "_blank";
          a.rel = "noopener noreferrer";
          a.textContent = linkMatch[1];
          frag.append(a);
        } else {
          frag.append(linkMatch[1]);
        }
      } else {
        frag.append(tok);
      }
    }
    lastIdx = at + tok.length;
  }
  if (lastIdx < text.length) frag.append(text.slice(lastIdx));
  return frag;
}

export function isMarkdownFileName(name: string): boolean {
  return /\.(?:md|markdown|mdx)$/i.test(name.trim());
}
