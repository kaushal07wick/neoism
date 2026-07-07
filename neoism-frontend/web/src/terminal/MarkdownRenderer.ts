export type MarkdownBlock =
  | { kind: "heading"; level: number; text: string }
  | { kind: "paragraph"; text: string }
  | { kind: "task"; checked: boolean; text: string }
  | { kind: "code"; lang: string | null; code: string }
  | { kind: "quote"; text: string }
  | { kind: "divider" };

/**
 * A parsed block plus the 0-based source-line range it came from
 * (inclusive, counted in the ORIGINAL source — frontmatter included).
 * The presence overlay maps remote collaborator cursors (source
 * line/column on the wire) back onto rendered block elements through
 * these spans.
 */
export interface MarkdownBlockSpan {
  block: MarkdownBlock;
  line: number;
  endLine: number;
}

export function isMarkdownPath(path: string | null | undefined): boolean {
  if (!path) return false;
  const clean = path.split(/[?#]/, 1)[0] ?? path;
  const ext = clean.split(/[\\/]/).pop()?.split(".").pop()?.toLowerCase();
  return ext === "md" || ext === "markdown" || ext === "mdx";
}

export function parseMarkdownBlocks(source: string): MarkdownBlock[] {
  return parseMarkdownBlockSpans(source).map((span) => span.block);
}

export function parseMarkdownBlockSpans(source: string): MarkdownBlockSpan[] {
  const spans: MarkdownBlockSpan[] = [];
  const paragraph: string[] = [];
  let paragraphStart = 0;
  let inCode = false;
  let codeStart = 0;
  let codeLang: string | null = null;
  let code = "";
  const lines = source.split(/\r?\n/);
  let index = lines[0] === "---" ? 1 : 0;
  if (index === 1) {
    while (index < lines.length && lines[index]?.trim() !== "---") index += 1;
    if (index < lines.length) index += 1;
  }

  const flushParagraph = (endLine: number): void => {
    if (paragraph.length === 0) return;
    spans.push({
      block: { kind: "paragraph", text: paragraph.join(" ") },
      line: paragraphStart,
      endLine,
    });
    paragraph.length = 0;
  };

  for (; index < lines.length; index += 1) {
    const line = lines[index] ?? "";
    const trimmed = line.trim();
    const fence = line.trimStart().match(/^```\s*(.*)$/);
    if (fence) {
      flushParagraph(index - 1);
      if (inCode) {
        spans.push({
          block: { kind: "code", lang: codeLang, code: code.replace(/\n$/, "") },
          line: codeStart,
          endLine: index,
        });
        code = "";
        codeLang = null;
        inCode = false;
      } else {
        inCode = true;
        codeStart = index;
        codeLang = fence[1]?.trim() || null;
      }
      continue;
    }
    if (inCode) {
      code += `${line}\n`;
      continue;
    }
    if (trimmed.length === 0) {
      flushParagraph(index - 1);
      continue;
    }
    const heading = parseHeading(trimmed);
    if (heading) {
      flushParagraph(index - 1);
      spans.push({ block: heading, line: index, endLine: index });
      continue;
    }
    if (isDivider(trimmed)) {
      flushParagraph(index - 1);
      spans.push({ block: { kind: "divider" }, line: index, endLine: index });
      continue;
    }
    const task = parseTask(trimmed);
    if (task) {
      flushParagraph(index - 1);
      spans.push({ block: task, line: index, endLine: index });
      continue;
    }
    if (trimmed.startsWith(">")) {
      flushParagraph(index - 1);
      spans.push({
        block: { kind: "quote", text: trimmed.slice(1).trim() },
        line: index,
        endLine: index,
      });
      continue;
    }
    if (paragraph.length === 0) paragraphStart = index;
    paragraph.push(trimmed);
  }
  if (inCode) {
    spans.push({
      block: { kind: "code", lang: codeLang, code: code.replace(/\n$/, "") },
      line: codeStart,
      endLine: Math.max(codeStart, lines.length - 1),
    });
  }
  flushParagraph(lines.length - 1);
  return spans;
}

export function renderMarkdownDocument(source: string): HTMLElement {
  if (typeof document === "undefined") {
    throw new Error("renderMarkdownDocument requires a DOM document");
  }
  const root = document.createElement("article");
  root.className = "web-markdown-document";
  const spans = parseMarkdownBlockSpans(source);
  if (spans.length === 0) {
    const empty = document.createElement("p");
    empty.className = "web-markdown-empty";
    empty.textContent = "Empty markdown document";
    root.appendChild(empty);
    return root;
  }
  for (const span of spans) {
    const el = renderBlock(span.block);
    // Source-line range tags: the presence overlay anchors remote
    // collaborator carets onto block elements through these.
    el.dataset.mdLine = String(span.line);
    el.dataset.mdLineEnd = String(span.endLine);
    root.appendChild(el);
  }
  return root;
}

function renderBlock(block: MarkdownBlock): HTMLElement {
  switch (block.kind) {
    case "heading": {
      const level = Math.min(3, Math.max(1, Math.trunc(block.level)));
      const el = document.createElement(`h${level}`);
      el.className = `web-markdown-heading web-markdown-heading-${level}`;
      el.textContent = block.text;
      return el;
    }
    case "paragraph": {
      const el = document.createElement("p");
      el.className = "web-markdown-paragraph";
      appendInline(el, block.text);
      return el;
    }
    case "task": {
      const el = document.createElement("div");
      el.className = `web-markdown-task${block.checked ? " is-checked" : ""}`;
      const box = document.createElement("span");
      box.className = "web-markdown-task-box";
      box.setAttribute("aria-hidden", "true");
      box.textContent = block.checked ? "☑" : "☐";
      const text = document.createElement("span");
      appendInline(text, block.text);
      el.append(box, text);
      return el;
    }
    case "code": {
      const figure = document.createElement("figure");
      figure.className = "web-markdown-code";
      if (block.lang) {
        const caption = document.createElement("figcaption");
        caption.textContent = block.lang;
        figure.appendChild(caption);
      }
      const pre = document.createElement("pre");
      const code = document.createElement("code");
      code.textContent = block.code;
      pre.appendChild(code);
      figure.appendChild(pre);
      return figure;
    }
    case "quote": {
      const el = document.createElement("blockquote");
      el.className = "web-markdown-quote";
      appendInline(el, block.text);
      return el;
    }
    case "divider": {
      const el = document.createElement("hr");
      el.className = "web-markdown-divider";
      return el;
    }
  }
}

function appendInline(parent: HTMLElement, text: string): void {
  const pattern = /(`[^`]+`|\*\*[^*]+\*\*|\[[^\]]+\]\(([^)]+)\))/g;
  let last = 0;
  for (const match of text.matchAll(pattern)) {
    if (match.index > last) {
      parent.appendChild(document.createTextNode(text.slice(last, match.index)));
    }
    const token = match[0];
    if (token.startsWith("`")) {
      const code = document.createElement("code");
      code.textContent = token.slice(1, -1);
      parent.appendChild(code);
    } else if (token.startsWith("**")) {
      const strong = document.createElement("strong");
      strong.textContent = token.slice(2, -2);
      parent.appendChild(strong);
    } else {
      const close = token.indexOf("](");
      const label = token.slice(1, close);
      const href = token.slice(close + 2, -1);
      const anchor = document.createElement("a");
      anchor.textContent = label;
      anchor.href = href;
      anchor.rel = "noreferrer";
      parent.appendChild(anchor);
    }
    last = match.index + token.length;
  }
  if (last < text.length) {
    parent.appendChild(document.createTextNode(text.slice(last)));
  }
}

function parseHeading(trimmed: string): MarkdownBlock | null {
  const match = /^(#{1,6})\s+(.+)$/.exec(trimmed);
  if (!match) return null;
  return {
    kind: "heading",
    level: match[1]?.length ?? 1,
    text: (match[2] ?? "").replace(/\s+#+\s*$/, "").trim(),
  };
}

function parseTask(trimmed: string): MarkdownBlock | null {
  const match = /^[-*+]\s+\[([ xX])\]\s*(.*)$/.exec(trimmed);
  if (!match) return null;
  return {
    kind: "task",
    checked: (match[1] ?? " ").toLowerCase() === "x",
    text: match[2] ?? "",
  };
}

function isDivider(trimmed: string): boolean {
  return /^(?:-{3,}|\*{3,}|_{3,})$/.test(trimmed);
}