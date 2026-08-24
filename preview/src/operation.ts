import type { OperationSnapshot } from './types';

export function operationMessage(message: string, operation: OperationSnapshot | null): string {
  if (operation?.kind) {
    return operation.total > 0
      ? `${operation.message.replace(/…$/, '')} ${operation.completed} of ${operation.total}`
      : operation.message;
  }
  return operation?.lastError
    ? `${message} · Last background error: ${operation.lastError}`
    : message;
}
