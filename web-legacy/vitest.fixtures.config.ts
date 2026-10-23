/// <reference types="vitest/config" />
import { defineConfig, mergeConfig } from 'vitest/config';
import base from './vite.config';

// Golden-fixture export for the native port (see tools/fixtures/). Kept out
// of the normal test run: it writes files and takes a while.
export default mergeConfig(base, defineConfig({
  test: {
    include: ['tools/fixtures/**/*.fixture.ts'],
    testTimeout: 30 * 60 * 1000,
  },
}));
