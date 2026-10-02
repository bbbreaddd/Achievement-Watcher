import { describe, expect, it } from 'vitest';
import { notificationStatusMessage } from './notification-status';

describe('notification delivery status', () => {
  it('names native delivery without assuming an operating system or fallback reason', () => {
    expect(notificationStatusMessage({ transport: 'native', success: true }))
      .toBe('System notification delivered');
  });

  it('names transport failures and preserves their reason', () => {
    expect(notificationStatusMessage({ transport: 'overlay', success: false, error: 'renderer timed out' }))
      .toBe('Notification failed through Custom popup: renderer timed out');
  });
});
