import '@testing-library/jest-dom/vitest';
import { expect } from 'vitest';

type AxeResult = {
  violations?: Array<{ id: string }>;
};

expect.extend({
  toHaveNoViolations(received: AxeResult) {
    const violations = received?.violations ?? [];
    const pass = violations.length === 0;

    return {
      pass,
      message: () =>
        pass
          ? 'Expected accessibility violations, but none were found.'
          : `Expected no accessibility violations, but found ${violations.length}: ${violations
              .map((violation) => violation.id)
              .join(', ')}`,
    };
  },
});

class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}

if (typeof HTMLCanvasElement !== 'undefined') {
  Object.defineProperty(HTMLCanvasElement.prototype, 'getContext', {
    configurable: true,
    value: () => null,
  });
}

if (typeof globalThis.ResizeObserver === 'undefined') {
  globalThis.ResizeObserver = ResizeObserverStub as unknown as typeof ResizeObserver;
}

declare module 'vitest' {
  interface Assertion {
    toHaveNoViolations(): void;
  }
}
