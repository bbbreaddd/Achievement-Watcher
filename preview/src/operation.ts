import type { OperationSnapshot } from './types';

export function operationMessage(
  message: string,
  operation: OperationSnapshot | null,
  liveUpdateErrors: string[] = [],
): string {
  if (operation?.kind) {
    return operation.total > 0
      ? `${operation.message.replace(/…$/, '')} ${operation.completed} of ${operation.total}`
      : operation.message;
  }
  const backgroundMessage = operation?.lastError
    ? `${message} · Last background error: ${operation.lastError}`
    : message;
  return liveUpdateErrors.length
    ? `${backgroundMessage} · Live updates unavailable: ${liveUpdateErrors.join(', ')}`
    : backgroundMessage;
}
