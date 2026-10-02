import { convertFileSrc } from '@tauri-apps/api/core';

export function isLocalPath(value: string): boolean {
  return value.startsWith('/') || /^[a-zA-Z]:[\\/]/.test(value) || value.startsWith('\\\\');
}

export function imageUrl(value: string): string {
  return isLocalPath(value) ? convertFileSrc(value) : value;
}
