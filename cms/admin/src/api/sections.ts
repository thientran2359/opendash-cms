import { requestJson } from './client.ts';

/** Template role catalog remains available in S02; private Section CRUD and
 * its detached document API were superseded by Pattern CPT. */
export interface LocalThemeTemplateDescriptor {
  role: string;
  theme_id: string;
  revision_id: string;
  revision_digest: string;
  can_apply: boolean;
  reason: string | null;
}

export interface LocalTemplateLibrary {
  active_theme_id: string | null;
  theme_state: 'no_active_theme' | 'active' | 'legacy_theme_unsupported'
    | 'active_theme_unavailable' | 'admin_only';
  theme_templates: LocalThemeTemplateDescriptor[];
}

export const getLocalTemplateLibrary = () => requestJson<LocalTemplateLibrary>('/api/admin/library', {
  defaultErrorMessage: 'Không tải được thư viện template cục bộ.',
});
