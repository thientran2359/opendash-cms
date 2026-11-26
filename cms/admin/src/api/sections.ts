import { tStatic } from '../i18n/index.tsx';
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
  theme_state: 'no_active_theme' | 'active' | 'unsupported_theme_format'
    | 'active_theme_unavailable' | 'admin_only';
  theme_templates: LocalThemeTemplateDescriptor[];
}

export const getLocalTemplateLibrary = () => requestJson<LocalTemplateLibrary>('/api/admin/library', {
  defaultErrorMessage: tStatic('api.sections.load_the_local_template_library'),
});
