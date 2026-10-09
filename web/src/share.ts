// Shareable URLs: the scenario, spec (when changed) and settings are
// deflate-compressed and base64url-encoded into the URL fragment, so the
// server never sees them and nothing is stored anywhere.

export interface ShareState { scenario?: string; spec?: string; engines?: number; units?: string; charts?: string[] }

async function pump(stream: ReadableStream<Uint8Array>): Promise<Uint8Array> {
  const chunks: Uint8Array[] = [];
  const reader = stream.getReader();
  for (;;) { const { done, value } = await reader.read(); if (done) break; chunks.push(value); }
  const out = new Uint8Array(chunks.reduce((n, c) => n + c.length, 0));
  let o = 0;
  for (const c of chunks) { out.set(c, o); o += c.length; }
  return out;
}

function b64url(bytes: Uint8Array): string {
  let s = "";
  for (const b of bytes) s += String.fromCharCode(b);
  return btoa(s).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

function unb64url(s: string): Uint8Array {
  const b = atob(s.replace(/-/g, "+").replace(/_/g, "/") + "=".repeat((4 - (s.length % 4)) % 4));
  return Uint8Array.from(b, (c) => c.charCodeAt(0));
}

export async function encodeShare(state: ShareState): Promise<string> {
  const json = new TextEncoder().encode(JSON.stringify(state));
  const cs = new CompressionStream("deflate-raw");
  const w = cs.writable.getWriter();
  void w.write(json as unknown as BufferSource); void w.close();
  return b64url(await pump(cs.readable));
}

export async function decodeShare(hash: string): Promise<ShareState | null> {
  const m = hash.match(/[#&]s=([A-Za-z0-9_-]+)/);
  if (!m) return null;
  try {
    const ds = new DecompressionStream("deflate-raw");
    const w = ds.writable.getWriter();
    void w.write(unb64url(m[1]) as unknown as BufferSource); void w.close();
    return JSON.parse(new TextDecoder().decode(await pump(ds.readable))) as ShareState;
  } catch (e) {
    console.warn("share link could not be decoded", e);
    return null;
  }
}
