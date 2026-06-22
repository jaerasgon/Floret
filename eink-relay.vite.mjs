/**
 * Floret e-ink relay — dev-server middleware (no separate process).
 *
 * The browser's io/eink node POSTs frames here over SAME-ORIGIN localhost
 * (so macOS/Chrome local-network gates never apply); the Raspberry Pi polls
 * and pulls. Holds one frame + monotonic frame/clear counters in memory.
 *
 *   browser node ─POST /__floret/eink/frame──▶ relay (this) ◀─GET poll/frame─ Pi
 *
 * Endpoints (mounted under /__floret/eink):
 *   POST /frame      store the latest PNG; bump frameId
 *   POST /clear      bump clearId (request a panel blank)
 *   GET  /poll       → { frameId, clearId, has }   (Pi diffs against last seen)
 *   GET  /frame.png  → the latest PNG bytes
 *
 * Pair with `server.host: true` so the Pi can reach it across the LAN.
 */
export function einkRelay() {
  /** @type {Buffer | null} */
  let frame = null;
  let frameId = 0;
  let clearId = 0;

  const cors = (res) => res.setHeader('Access-Control-Allow-Origin', '*');

  return {
    name: 'floret-eink-relay',
    configureServer(server) {
      server.middlewares.use('/__floret/eink', (req, res, next) => {
        const path = (req.url || '').split('?')[0];

        if (req.method === 'POST' && path === '/frame') {
          const chunks = [];
          req.on('data', (c) => chunks.push(c));
          req.on('end', () => { frame = Buffer.concat(chunks); frameId++; cors(res); res.statusCode = 204; res.end(); });
          req.on('error', () => { res.statusCode = 400; res.end(); });
          return;
        }
        if (req.method === 'POST' && path === '/clear') {
          clearId++; cors(res); res.statusCode = 204; res.end();
          return;
        }
        if (req.method === 'GET' && path === '/poll') {
          cors(res); res.setHeader('Content-Type', 'application/json');
          res.end(JSON.stringify({ frameId, clearId, has: !!frame }));
          return;
        }
        if (req.method === 'GET' && path === '/frame.png') {
          if (!frame) { res.statusCode = 404; res.end('no frame'); return; }
          cors(res);
          res.setHeader('Content-Type', 'image/png');
          res.setHeader('Cache-Control', 'no-store');
          res.end(frame);
          return;
        }
        next();
      });
    },
  };
}
