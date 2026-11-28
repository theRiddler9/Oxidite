import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { isMarkdownFileName, renderMarkdownToFragment, splitTableRow } from "./markdown";

describe("markdown", () => {
  let prevDocument: unknown;

  beforeEach(() => {
    prevDocument = (globalThis as any).document;
    const mk = (tag: string) => {
      const el: any = {
        tagName: tag.toUpperCase(),
        children: [] as any[],
        textContent: "",
        href: "",
        type: "",
        disabled: false,
        checked: false,
        className: "",
        setAttribute(k: string, v: string) {
          el.attrs = { ...(el.attrs ?? {}), [k]: v };
        },
        append(...nodes: any[]) {
          for (const n of nodes) {
            if (typeof n === "string") el.children.push({ text: n });
            else el.children.push(n);
          }
        },
      };
      return el;
    };
    (globalThis as any).document = {
      createDocumentFragment: () => mk("fragment"),
      createElement: (tag: string) => mk(tag),
    };
  });

  afterEach(() => {
    (globalThis as any).document = prevDocument;
  });

  test("identifies markdown filenames cleanly", () => {
    expect(isMarkdownFileName("README.md")).toBe(true);
    expect(isMarkdownFileName("implementation_plan.markdown")).toBe(true);
    expect(isMarkdownFileName("Cargo.toml")).toBe(false);
  });

  test("renders headings, task lists, code blocks, and strips unsafe javascript: links", () => {
    const md = [
      "# Title",
      "",
      "- [x] Done with **bold** and `code`",
      "- [ ] [Evil](javascript:alert(1)) vs [Safe](https://example.com)",
      "",
      "```rs",
      "fn main() {}",
      "```",
    ].join("\n");

    const frag = renderMarkdownToFragment(md) as any;
    expect(frag.children[0].tagName).toBe("H1");
    expect(frag.children[1].tagName).toBe("UL");
    expect(frag.children[2].className).toBe("md-code");
    expect(frag.children[2].children[0].tagName).toBe("PRE");

    // Evil javascript: link must NOT create an <a> element
    const secondItemChildren = frag.children[1].children[1].children;
    const anchors = JSON.stringify(secondItemChildren);
    expect(anchors).not.toContain("javascript:");
    expect(anchors).toContain("https://example.com");
  });

  const textOf = (n: any): string =>
    n.text ?? (n.children ?? []).map(textOf).join("") + (n.textContent ?? "");

  test("renders GFM pipe tables with alignment, escaped pipes and normalized row width", () => {
    const md = [
      "Intro line",
      "| Name | Qty | Note |",
      "|:-----|----:|:----:|",
      "| a \\| b | 1 | `x|y` |",
      "| short |",
      "| long | 2 | z | extra |",
      "",
      "after",
    ].join("\n");
    const frag = renderMarkdownToFragment(md) as any;
    // Table directly after a paragraph line must still be recognised (GFM allows interrupting).
    expect(frag.children[0].tagName).toBe("P");
    const wrap = frag.children[1];
    expect(wrap.className).toBe("md-table-wrap");
    const table = wrap.children[0];
    const [thead, tbody] = table.children;
    const ths = thead.children[0].children;
    expect(ths.map(textOf)).toEqual(["Name", "Qty", "Note"]);
    expect(ths[0].className).toBe("md-al-left");
    expect(ths[1].className).toBe("md-al-right");
    expect(ths[2].className).toBe("md-al-center");
    expect(tbody.children.length).toBe(3);
    const row1 = tbody.children[0].children.map(textOf);
    expect(row1).toEqual(["a | b", "1", "x|y"]);
    expect(tbody.children[1].children.map(textOf)).toEqual(["short", "", ""]);
    expect(tbody.children[2].children.length).toBe(3);
    expect(frag.children[2].tagName).toBe("P");
  });

  test("does not treat pipes without a delimiter row, or with mismatched columns, as a table", () => {
    expect(splitTableRow("| a | b |")).toEqual(["a", "b"]);
    const a = renderMarkdownToFragment("a | b\nc | d") as any;
    expect(a.children.map((n: any) => n.tagName)).toEqual(["P"]);
    const b = renderMarkdownToFragment("| a | b |\n|---|\n| 1 | 2 |") as any;
    expect(b.children.some((n: any) => n.className === "md-table-wrap")).toBe(false);
  });

  test("table cells render inline markup but never create javascript: links", () => {
    const frag = renderMarkdownToFragment("| h |\n|---|\n| [x](javascript:alert(1)) **b** |") as any;
    const json = JSON.stringify(frag);
    expect(json).not.toContain("javascript:");
    expect(json).toContain("STRONG");
  });

  test("fenced code gets a Copy button that copies the raw code and reports the result", async () => {
    const copied: string[] = [];
    (globalThis as any).navigator = { clipboard: { writeText: async (t: string) => void copied.push(t) } };
    const frag = renderMarkdownToFragment("```sh\nls -la\necho hi\n```") as any;
    const wrap = frag.children[0];
    const btn = wrap.children[1];
    expect(btn.tagName).toBe("BUTTON");
    expect(btn.type).toBe("button");
    expect(btn.textContent).toBe("Copy");
    btn.onclick();
    await Promise.resolve();
    await Promise.resolve();
    expect(copied).toEqual(["ls -la\necho hi"]);
    expect(btn.textContent).toBe("Copied");

    (globalThis as any).navigator = { clipboard: { writeText: async () => Promise.reject(new Error("denied")) } };
    const f2 = renderMarkdownToFragment("```\nx\n```") as any;
    const b2 = f2.children[0].children[1];
    b2.onclick();
    await new Promise((r) => setTimeout(r, 0));
    expect(b2.textContent).toBe("Copy failed");
  });
});
