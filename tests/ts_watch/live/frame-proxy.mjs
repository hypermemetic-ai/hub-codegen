#!/usr/bin/env node
// RETURNS-38 · a loopback proxy in front of a throwaway menger that signs
// each WebSocket upgrade in with a key proof and counts the frames the client
// sends — so a live check sees exactly what went on the wire.
//
//   MENGER_DEPLOY_KEY_FILE=~/.plexus/keys/builder.key \
//     node frame-proxy.mjs --listen 4486 --upstream 4484 --agent builder \
//       --call-module ../../../../menger/deploy/menger-call.mjs
//
// Every client→server text frame is printed as one JSON line on stdout:
// {"frame": <the JSON-RPC message>}. The key never leaves its file; the proof
// names ws://127.0.0.1:<upstream> as its audience. Loopback only.
import net from "node:net";
import path from "node:path";
import { pathToFileURL } from "node:url";

const arg = (name, dflt) => { const i = process.argv.indexOf(`--${name}`); return i > 0 ? process.argv[i + 1] : dflt; };
const listen = Number(arg("listen", "4486"));
const upstream = Number(arg("upstream", "4484"));
const agent = arg("agent", "builder");
const callModule = path.resolve(arg("call-module", "../../../../menger/deploy/menger-call.mjs"));
const { keyProof, seedOf, deployKeyText } = await import(pathToFileURL(callModule).href);
const seed = seedOf(deployKeyText());

/** Parse client frames (masked) out of a byte stream; call `onText` per complete text message. */
function frameReader(onText) {
  let buf = Buffer.alloc(0);
  let parts = [];
  return (chunk) => {
    buf = Buffer.concat([buf, chunk]);
    for (;;) {
      if (buf.length < 2) return;
      const fin = buf[0] & 0x80, op = buf[0] & 0x0f, masked = buf[1] & 0x80;
      let len = buf[1] & 0x7f, off = 2;
      if (len === 126) { if (buf.length < 4) return; len = buf.readUInt16BE(2); off = 4; }
      else if (len === 127) { if (buf.length < 10) return; len = Number(buf.readBigUInt64BE(2)); off = 10; }
      const mk = masked ? buf.subarray(off, off + 4) : null;
      if (masked) off += 4;
      if (buf.length < off + len) return;
      const data = Buffer.from(buf.subarray(off, off + len));
      if (mk) for (let i = 0; i < data.length; i++) data[i] ^= mk[i & 3];
      buf = buf.subarray(off + len);
      if (op === 0x1 || op === 0x0) {
        parts.push(data);
        if (fin) { const text = Buffer.concat(parts).toString("utf8"); parts = []; onText(text); }
      }
    }
  };
}

net.createServer((client) => {
  let head = Buffer.alloc(0);
  const onHead = (chunk) => {
    head = Buffer.concat([head, chunk]);
    const end = head.indexOf("\r\n\r\n");
    if (end < 0) return;
    client.off("data", onHead);
    const rest = head.subarray(end + 4);
    const aud = `ws://127.0.0.1:${upstream}`;
    const lines = head.subarray(0, end).toString("latin1").split("\r\n")
      .filter((l) => !/^authorization:/i.test(l))
      .map((l) => /^host:/i.test(l) ? `Host: 127.0.0.1:${upstream}` : l);
    lines.push(`Authorization: Bearer ${keyProof(seed, aud, agent)}`);
    const server = net.connect({ host: "127.0.0.1", port: upstream }, () => {
      server.write(lines.join("\r\n") + "\r\n\r\n");
      const read = frameReader((text) => {
        let frame; try { frame = JSON.parse(text); } catch { frame = text; }
        process.stdout.write(JSON.stringify({ frame }) + "\n");
      });
      if (rest.length) { read(rest); server.write(rest); }
      client.on("data", (c) => { read(c); server.write(c); });
      server.pipe(client);
    });
    server.on("error", () => client.destroy());
    client.on("error", () => server.destroy());
    client.on("close", () => server.end());
    server.on("close", () => client.end());
  };
  client.on("data", onHead);
}).listen(listen, "127.0.0.1", () => process.stderr.write(`frame-proxy: 127.0.0.1:${listen} → 127.0.0.1:${upstream} as ${agent}\n`));
