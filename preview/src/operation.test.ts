import { describe, expect, it } from 'vitest';
import { operationMessage } from './operation';

describe('operation status', () => {
  it('prioritizes active progress over an older message', () => {
    expect(operationMessage('Settings saved', {
      kind: 'scan', message: 'Scanning configured sources…', completed: 3, total: 8,
    })).toBe('Scanning configured sources 3 of 8');
  });

  it('keeps the last background failure visible after work stops', () => {
    expect(operationMessage('Library is ready', {
      message: '', completed: 0, total: 0, lastError: 'Steam helper unavailable',
    })).toContain('Steam helper unavailable');
  });
});
