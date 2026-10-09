// Minimal Markdown -> HTML for the model documentation (headings, paragraphs,
// lists, code blocks, tables, bold/italic/inline code, links, sub/superscript
// via HTML passthrough). Not a general renderer.

function inline(s: string): string {
  return s
    .replace(/&(?![a-z]+;|#\d+;)/g, "&amp;")
    .replace(/<(?!\/?(sub|sup|br|b|i|em|strong)\b)/g, "&lt;")
    .replace(/`([^`]+)`/g, (_, c) => `<code>${c.replace(/</g, "&lt;")}</code>`)
    .replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>")
    .replace(/\*([^*]+)\*/g, "<em>$1</em>")
    .replace(/\[([^\]]+)\]\(([^)]+)\)/g, '<a href="$2" target="_blank" rel="noopener">$1</a>');
}

export function renderMarkdown(md: string): string {
  const lines = md.split(/\r?\n/);
  const out: string[] = [];
  let i = 0;
  const para: string[] = [];
  const flush = (): void => { if (para.length) { out.push(`<p>${inline(para.join(" "))}</p>`); para.length = 0; } };
  while (i < lines.length) {
    const l = lines[i];
    if (/^```/.test(l)) {
      flush();
      const buf: string[] = [];
      i++;
      while (i < lines.length && !/^```/.test(lines[i])) buf.push(lines[i++]);
      i++;
      out.push(`<pre><code>${buf.join("\n").replace(/&/g, "&amp;").replace(/</g, "&lt;")}</code></pre>`);
      continue;
    }
    const h = l.match(/^(#{1,4})\s+(.*)$/);
    if (h) { flush(); const lvl = h[1].length; const id = h[2].toLowerCase().replace(/[^a-z0-9]+/g, "-"); out.push(`<h${lvl} id="${id}">${inline(h[2])}</h${lvl}>`); i++; continue; }
    if (/^\s*[-*]\s+/.test(l)) {
      flush();
      out.push("<ul>");
      while (i < lines.length && /^\s*[-*]\s+/.test(lines[i])) out.push(`<li>${inline(lines[i++].replace(/^\s*[-*]\s+/, ""))}</li>`);
      out.push("</ul>");
      continue;
    }
    if (/^\s*\d+\.\s+/.test(l)) {
      flush();
      out.push("<ol>");
      while (i < lines.length && /^\s*\d+\.\s+/.test(lines[i])) out.push(`<li>${inline(lines[i++].replace(/^\s*\d+\.\s+/, ""))}</li>`);
      out.push("</ol>");
      continue;
    }
    if (/^\|/.test(l)) {
      flush();
      const rows: string[] = [];
      while (i < lines.length && /^\|/.test(lines[i])) rows.push(lines[i++]);
      const cells = (r: string): string[] => r.replace(/^\||\|$/g, "").split("|").map((c) => c.trim());
      const head = cells(rows[0]);
      const body = rows.slice(1).filter((r) => !/^\|\s*:?-+/.test(r));
      out.push("<table><thead><tr>" + head.map((c) => `<th>${inline(c)}</th>`).join("") + "</tr></thead><tbody>");
      for (const r of body) out.push("<tr>" + cells(r).map((c) => `<td>${inline(c)}</td>`).join("") + "</tr>");
      out.push("</tbody></table>");
      continue;
    }
    if (/^\s*$/.test(l)) { flush(); i++; continue; }
    if (/^---+$/.test(l)) { flush(); out.push("<hr>"); i++; continue; }
    para.push(l.trim());
    i++;
  }
  flush();
  return out.join("\n");
}
