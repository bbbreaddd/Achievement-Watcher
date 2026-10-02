interface DeliveryReceipt {
  transport: string;
  success: boolean;
  error?: string;
}

export function notificationStatusMessage(receipt: DeliveryReceipt): string {
  if (!receipt.success) {
    return `Notification failed through ${transportLabel(receipt.transport)}: ${receipt.error ?? 'unknown error'}`;
  }
  return `${transportLabel(receipt.transport)} notification delivered`;
}

function transportLabel(transport: string): string {
  switch (transport) {
    case 'overlay': return 'Custom popup';
    case 'native': return 'System';
    case 'game_bar': return 'Xbox Game Bar';
    default: return transport || 'Unknown transport';
  }
}
