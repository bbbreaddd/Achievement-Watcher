import type { NotificationMode } from './types';

interface DeliveryReceipt {
  transport: string;
  success: boolean;
  error?: string;
}

export function notificationStatusMessage(receipt: DeliveryReceipt, mode: NotificationMode): string {
  if (!receipt.success) {
    return `Notification failed through ${transportLabel(receipt.transport)}: ${receipt.error ?? 'unknown error'}`;
  }
  if (receipt.transport === 'native' && mode === 'overlay_with_native_fallback') {
    return 'Windows fallback used because the custom popup was unavailable';
  }
  return `${transportLabel(receipt.transport)} notification delivered`;
}

function transportLabel(transport: string): string {
  switch (transport) {
    case 'overlay': return 'Custom popup';
    case 'native': return 'Windows';
    case 'game_bar': return 'Xbox Game Bar';
    default: return transport || 'Unknown transport';
  }
}
