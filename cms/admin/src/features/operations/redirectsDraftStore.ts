import type { ConflictPolicy } from '../../api/redirects.ts';

export type RedirectModalMode = 'create' | 'edit' | 'from_404' | 'csv_import';

export interface RedirectsDraftRecord {
  modalMode: RedirectModalMode;
  editingId: number | null;
  baselineVersion: number | null;
  sourcePath: string;
  targetUrl: string;
  statusCode: 301 | 302;
  enabled: boolean;
  note: string;
  resolveNotFound: boolean;
  csvText: string;
  conflictPolicy: ConflictPolicy;
  staleConflict: boolean;
  dirty: boolean;
}

// Module-level RAM draft store (no localStorage/sessionStorage per OpenDash security rule).
const draftStore = new Map<string, RedirectsDraftRecord>();

export function redirectsDraftKey(siteInstanceId: string, userId: number): string {
  return `${siteInstanceId}:${userId}`;
}

export function getRedirectsDraft(
  siteInstanceId: string,
  userId: number,
): RedirectsDraftRecord | undefined {
  return draftStore.get(redirectsDraftKey(siteInstanceId, userId));
}

export function setRedirectsDraft(
  siteInstanceId: string,
  userId: number,
  draft: RedirectsDraftRecord,
): void {
  draftStore.set(redirectsDraftKey(siteInstanceId, userId), draft);
}

export function discardRedirectsDraft(siteInstanceId: string, userId: number): void {
  draftStore.delete(redirectsDraftKey(siteInstanceId, userId));
}

export function clearRedirectsDrafts(): void {
  draftStore.clear();
}
