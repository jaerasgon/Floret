# Floret, the earlier web version

This is the first version of Floret, written in TypeScript to run in a browser. It is kept for reference and isn't the app anymore: the app is in [`native/`](../native). Nothing here ships in a release.

To run it:

```
cd web-legacy
npm install
npm run dev
```

Then open <http://localhost:5173>. It shares the images and MediaPipe files in [`public/`](../public) with the app.

The golden-fixture exporter in `tools/fixtures/` (`npm run fixtures:export`) recorded what this version computes, so the app's engine could be checked against it. Those recordings are in [`native/fixtures/`](../native/fixtures).
