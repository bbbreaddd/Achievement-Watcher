import { mount, tick, unmount } from 'svelte';
import { describe, expect, it } from 'vitest';
import type { NotificationEvent } from '../types';
import NotificationCard from './NotificationCard.svelte';

const event: NotificationEvent = {
  id: 1,
  eventKey: 'unlock:portal:transmission-received',
  kind: 'unlock',
  attempts: 0,
  nextAttemptAt: 0,
  observation: {
    sourceId: 'steam-client',
    gameId: '400',
    achievementId: 'ACH.TRANSMISSION_RECEIVED',
    achieved: true,
    hidden: false,
    currentProgress: 0,
    maxProgress: 0,
    unlockTime: 0,
    displayName: 'Transmission Received',
    description: 'Find the hidden radio transmission.',
  },
};

describe('NotificationCard', () => {
  it('uses the same content controls for embedded previews', async () => {
    const target = document.createElement('div');
    document.body.append(target);
    const component = mount(NotificationCard, {
      target,
      props: {
        event,
        preset: 'steam',
        presetConfig: { width: 382, height: 106, durationMs: 4_000 },
        controls: false,
      },
    });

    expect(target.textContent).toContain('Achievement unlocked');
    expect(target.textContent).toContain('Transmission Received');
    expect(target.textContent).toContain('Find the hidden radio transmission.');
    expect(target.querySelector('button')).toBeNull();

    await unmount(component);
    target.remove();
    await tick();
  });
});
