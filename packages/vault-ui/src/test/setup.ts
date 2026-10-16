import '@testing-library/jest-dom/vitest';
import { expect } from 'vitest';
import * as matchers from 'vitest-axe/matchers';

const customMatchers = (matchers && typeof matchers === 'object' && 'toHaveNoViolations' in matchers)
  ? matchers
  : (matchers as any)?.default || matchers || {};

if (customMatchers && typeof customMatchers === 'object' && Object.keys(customMatchers).length > 0) {
  expect.extend(customMatchers);
}


class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}

if (typeof globalThis.ResizeObserver === 'undefined') {
  globalThis.ResizeObserver = ResizeObserverStub as unknown as typeof ResizeObserver;
}
