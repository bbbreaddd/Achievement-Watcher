import { mount, unmount } from 'svelte';
import { describe, expect, it, vi } from 'vitest';
import TitleBar from './TitleBar.svelte';

describe('TitleBar', () => {
  it('keeps settings separate and describes the current resize action', async () => {
    const target = document.createElement('div');
    document.body.append(target);
    const onMaximize = vi.fn();
    const component = mount(TitleBar, {
      target,
      props: {
        activity: null,
        settingsActive: false,
        maximized: true,
        onMinimize: vi.fn(),
        onSettings: vi.fn(),
        onMaximize,
        onClose: vi.fn(),
      },
    });

    expect(target.querySelector('nav')?.querySelector('[aria-label="Settings"]')).toBeNull();
    const restore = target.querySelector<HTMLButtonElement>('[aria-label="Restore"]')!;
    restore.click();
    expect(onMaximize).toHaveBeenCalledOnce();

    await unmount(component);
    target.remove();
  });
});
