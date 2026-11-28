import React from 'react';
import type { AdminLocaleState } from '../api/locales.ts';
import { SingleEditWorkspace } from '../features/editor/SingleEditWorkspace.tsx';
import type { AuthUser } from '../types.ts';
import type { ParsedRoute } from './AdminShell.tsx';

export function draftUuidFromHash(hash: string): string | null {
  const param = new URLSearchParams(hash.split('?')[1] ?? '').get('draft');
  return param && /^[a-zA-Z0-9_-]{8,64}$/.test(param) ? param : null;
}

export interface BuildContentWorkspacesParams {
  route: ParsedRoute;
  contentRevision: number;
  user: AuthUser | null;
  siteInstanceId: string;
  authGeneration: number;
  csrfToken: string | null;
  localeState: AdminLocaleState;
  onApiError: (error: unknown, fallback: string) => void;
  onNavigateHash: (hash: string) => void;
  onContentDirtyChange: (dirty: boolean) => void;
  onPostsDirtyChange: (dirty: boolean) => void;
  onPagesDirtyChange: (dirty: boolean) => void;
}

export function buildContentWorkspaces({
  route,
  contentRevision,
  user,
  siteInstanceId,
  authGeneration,
  csrfToken,
  localeState,
  onApiError,
  onNavigateHash,
  onContentDirtyChange,
  onPostsDirtyChange,
  onPagesDirtyChange,
}: BuildContentWorkspacesParams): {
  contentContent: React.ReactNode;
  postsContent: React.ReactNode;
  pagesContent: React.ReactNode;
} {
  const contentContent =
    route.view === 'cpt' && route.typeKey ? (
      <SingleEditWorkspace
        key={`cpt-${route.typeKey}-${contentRevision}`}
        typeKey={route.typeKey}
        mode={
          route.sub === 'new'
            ? 'new'
            : route.sub === 'edit'
              ? 'edit'
              : 'list'
        }
        entryId={route.entryId ?? null}
        user={user}
        siteInstanceId={siteInstanceId}
        authGeneration={authGeneration}
        draftUuid={draftUuidFromHash(route.rawHash) ?? undefined}
        routeHash={route.rawHash}
        isActive={route.view === 'cpt'}
        csrfToken={csrfToken}
        localeState={localeState}
        onApiError={onApiError}
        onDirtyChange={onContentDirtyChange}
        onNavigateHash={onNavigateHash}
      />
    ) : null;

  const postsContent = (
    <SingleEditWorkspace
      key={'posts-' + contentRevision}
      typeKey="post"
      mode={
        route.view === 'posts'
          ? route.sub === 'new'
            ? 'new'
            : route.sub === 'edit'
              ? 'edit'
              : 'list'
          : 'list'
      }
      entryId={route.view === 'posts' ? (route.entryId ?? null) : null}
      user={user}
      siteInstanceId={siteInstanceId}
      authGeneration={authGeneration}
      draftUuid={draftUuidFromHash(route.rawHash) ?? undefined}
      routeHash={route.view === 'posts' ? route.rawHash : '#/posts'}
      isActive={route.view === 'posts'}
      csrfToken={csrfToken}
      localeState={localeState}
      onApiError={onApiError}
      onDirtyChange={onPostsDirtyChange}
      onNavigateHash={onNavigateHash}
    />
  );

  const pagesContent = (
    <SingleEditWorkspace
      key={'pages-' + contentRevision}
      typeKey="page"
      mode={
        route.view === 'pages'
          ? route.sub === 'new'
            ? 'new'
            : route.sub === 'edit'
              ? 'edit'
              : 'list'
          : 'list'
      }
      entryId={route.view === 'pages' ? (route.entryId ?? null) : null}
      user={user}
      siteInstanceId={siteInstanceId}
      authGeneration={authGeneration}
      draftUuid={draftUuidFromHash(route.rawHash) ?? undefined}
      routeHash={route.view === 'pages' ? route.rawHash : '#/pages'}
      isActive={route.view === 'pages'}
      csrfToken={csrfToken}
      localeState={localeState}
      onApiError={onApiError}
      onDirtyChange={onPagesDirtyChange}
      onNavigateHash={onNavigateHash}
    />
  );

  return { contentContent, postsContent, pagesContent };
}
