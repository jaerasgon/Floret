// Barrel — the sample catalogue was split into ./samples/* (helpers,
// per-category build files, and the metadata catalogue). This file keeps
// the original import path '../persistence/samples' stable.
export type { Sample } from './samples/helpers';
export { SAMPLE_CATEGORIES, SAMPLES, buildBuiltInSampleLibraries } from './samples/catalogue';
