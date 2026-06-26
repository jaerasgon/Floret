/**
 * Floret bus — a tiny channel relay living in the dev server (no separate app).
 *
 * Generic transport for the Send / Receive nodes: a browser can't be a server
 * and can't freely reach LAN devices, so nodes POST/GET here over same-origin
 * localhost instead. Each named channel holds the latest payload + content-type
 * + a monotonic version. Values travel as JSON, images as PNG — the bus is
 * payload-agnostic, it just stores bytes + their content-type.
 *
 *   POST /__floret/bus/<channel>        store payload (any content-type); bump version
 *   GET  /__floret/bus/<channel>        → the latest payload (with its content-type)
 *   GET  /__floret/bus/<channel>/poll   → { version, has, contentType }
 *
 * Pair with `server.host: true` so a device (e.g. a Raspberry Pi) can pull a
 * channel across the LAN.
 */
export function floretBus() {
  /** @type {Map<string, { body: Buffer, contentType: string, version: number }>} */
  const channels = new Map();
  const cors = (res) => res.setHeader('Access-Control-Allow-Origin', '*');

  return {
    name: 'floret-bus',
    configureServer(server) {
      server.middlewares.use('/__floret/bus', (req, res, next) => {
        const path = (req.url || '').split('?')[0].replace(/^\/+/, '');
        const slash = path.indexOf('/');
        const name = slash === -1 ? path : path.slice(0, slash);
        const sub = slash === -1 ? '' : path.slice(slash + 1);
        if (!name) { next(); return; }

        if (req.method === 'POST' && !sub) {
          const chunks = [];
          req.on('data', (c) => chunks.push(c));
          req.on('end', () => {
            const prev = channels.get(name);
            channels.set(name, {
              body: Buffer.concat(chunks),
              contentType: req.headers['content-type'] || 'application/octet-stream',
              version: (prev?.version ?? 0) + 1,
            });
            cors(res); res.statusCode = 204; res.end();
          });
          req.on('error', () => { res.statusCode = 400; res.end(); });
          return;
        }
        if (req.method === 'GET' && sub === 'poll') {
          const ch = channels.get(name);
          cors(res); res.setHeader('Content-Type', 'application/json');
          res.end(JSON.stringify({ version: ch?.version ?? 0, has: !!ch, contentType: ch?.contentType ?? null }));
          return;
        }
        if (req.method === 'GET' && !sub) {
          const ch = channels.get(name);
          if (!ch) { res.statusCode = 404; res.end('no channel'); return; }
          cors(res);
          res.setHeader('Content-Type', ch.contentType);
          res.setHeader('Cache-Control', 'no-store');
          res.setHeader('X-Version', String(ch.version));
          res.end(ch.body);
          return;
        }
        next();
      });
    },
  };
}
