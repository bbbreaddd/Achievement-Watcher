import { mount, tick, unmount } from 'svelte';
import { describe, expect, it, vi } from 'vitest';
import ConfirmDialog from './ConfirmDialog.svelte';

interface Props {
  title: string;
  message: string;
  confirmLabel: string;
  busy?: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}

function render(props: Props) {
  const target = document.createElement('div');
  document.body.append(target);
  const component = mount(ConfirmDialog, { target, props });
  return { target, component };
}

describe('ConfirmDialog', () => {
  it('starts on the safe action and closes with Escape', async () => {
    const onCancel = vi.fn();
    const view = render({
      title: 'Clear cached information?',
      message: 'Downloaded information will be removed.',
      confirmLabel: 'Clear cache',
      onConfirm: vi.fn(),
      onCancel,
    });

    await tick();
    const cancel = view.target.querySelector<HTMLButtonElement>('button')!;
    expect(document.activeElement).toBe(cancel);
    cancel.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    expect(onCancel).toHaveBeenCalledOnce();
    await unmount(view.component);
    view.target.remove();
  });

  it('does not dismiss a busy destructive action', async () => {
    const onCancel = vi.fn();
    const view = render({
      title: 'Hide this game?',
      message: 'The game will be hidden.',
      confirmLabel: 'Hide game',
      busy: true,
      onConfirm: vi.fn(),
      onCancel,
    });

    view.target.querySelector('[role="alertdialog"]')!
      .dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    expect(onCancel).not.toHaveBeenCalled();
    await unmount(view.component);
    view.target.remove();
  });
});
