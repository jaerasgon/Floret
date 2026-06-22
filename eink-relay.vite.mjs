/**
 * Floret e-ink frame relay — a dev-server middleware (no separate server).
 *
 * The browser running Floret POSTs the latest render frame (PNG) to
 * `/__floret/frame`; a Raspberry Pi on the LAN GETs `/__floret/frame.png`
 * on a timer and pushes it to its Waveshare e-paper panel. One frame is
 * held in memory — e-ink updates every few minutes, so there's no history
 * to keep. See tools/eink/ for the Pi side.
 *
 * Dev-only (rides on `vite dev`). Pair with `server.host: true` so the Pi
 * can reach it across the network.
 */
export function einkRelay() {
  /** @type {Buffer | null} */
  let frame = null;
  let at = 0;
  return {
    name: 'floret-eink-relay',
    configureServer(server) {
      server.middlewares.use('/__floret', (req, res, next) => {
        const url = req.url || '';
        // Browser → relay: store the latest frame.
        if (req.method === 'POST' && url.startsWith('/frame')) {
          const chunks = [];
          req.on('data', (c) => chunks.push(c));
          req.on('end', () => {
            frame = Buffer.concat(chunks);
            at = Date.now();
            res.statusCode = 204;
            res.end();
          });
          req.on('error', () => { res.statusCode = 400; res.end(); });
          return;
        }
        // Pi → relay: fetch the latest frame.
        if (req.method === 'GET' && url.startsWith('/frame.png')) {
          if (!frame) { res.statusCode = 404; res.end('no frame yet'); return; }
          res.setHeader('Content-Type', 'image/png');
          res.setHeader('Cache-Control', 'no-store');
          res.setHeader('Access-Control-Allow-Origin', '*');
          res.setHeader('X-Frame-Age-Ms', String(Date.now() - at));
          res.end(frame);
          return;
        }
        // Lightweight liveness probe for the Pi.
        if (req.method === 'GET' && url.startsWith('/frame-age')) {
          res.setHeader('Content-Type', 'application/json');
          res.setHeader('Access-Control-Allow-Origin', '*');
          res.end(JSON.stringify({ has: !!frame, ageMs: frame ? Date.now() - at : null }));
          return;
        }
        next();
      });
    },
  };
}
