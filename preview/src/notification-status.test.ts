import { describe, expect, it } from 'vitest';
import { notificationStatusMessage } from './notification-status';

describe('notification delivery status', () => {
  it('makes native fallback visible instead of reporting generic success', () => {
    expect(notificationStatusMessage({ transport: 'native', success: true }, 'overlay_with_native_fallback'))
      .toBe('Windows fallback used because the custom popup was unavailable');
  });

  it('names transport failures and preserves their reason', () => {
    expect(notificationStatusMessage({ transport: 'overlay', success: false, error: 'renderer timed out' }, 'overlay_only'))
      .toBe('Notification failed through Custom popup: renderer timed out');
  });
});
