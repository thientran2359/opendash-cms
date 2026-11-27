import React, { useEffect, useRef, useState } from 'react';
import {
  type ContentDefaultSort,
  type ContentPublicationPolicy,
  type ContentSupport,
  type ContentTypeDefinitionWithSettings,
} from '../../api/content.ts';
import type { TaxonomyDefinition } from '../../api/taxonomy.ts';
import { AdminAlert, AdminButton } from '../../components/AdminPrimitives.tsx';
import { CoreIconSvg } from '../../components/CoreIcon.tsx';
import { CORE_ICONS } from '../../fields/icons.ts';
import { __, useI18n } from '../../i18n/index.tsx';

const INPUT_CLASS =
  'h-9 w-full min-w-0 rounded-md border border-zinc-300 bg-white px-3 text-sm text-zinc-900 focus-visible:outline-2 focus-visible:outline-od-accent disabled:opacity-50';
const SELECT_CLASS =
  'h-9 w-full min-w-0 rounded-md border border-zinc-300 bg-white px-2.5 text-sm text-zinc-900 focus-visible:outline-2 focus-visible:outline-od-accent disabled:opacity-50';

const snakeKeyPattern = /^[a-z][a-z0-9_]{0,63}$/;
const kebabBasePattern = /^[a-z0-9](?:[a-z0-9-]{0,62}[a-z0-9])?$/;

export interface ContentTypeCreationDraft {
  key: string;
  label: string;
  singularLabel: string;
  description: string;
  isPublic: boolean;
  hasArchive: boolean;
  hierarchical: boolean;
  publication: ContentPublicationPolicy;
  supports: ContentSupport[];
  archivePerPage: number;
  defaultSort: ContentDefaultSort;
  excludeFromSitemap: boolean;
  menuName: string;
  menuIcon: string;
  menuPosition: number;
  showInMenu: boolean;
  rewriteBase: string;
  taxonomies: string[];
}

export const blankContentTypeCreationDraft = (): ContentTypeCreationDraft => ({
  key: '',
  label: '',
  singularLabel: '',
  description: '',
  isPublic: true,
  hasArchive: true,
  hierarchical: false,
  publication: 'immediate',
  supports: ['editor', 'featured_image'],
  archivePerPage: 12,
  defaultSort: 'published_at_desc',
  excludeFromSitemap: false,
  menuName: '',
  menuIcon: '',
  menuPosition: 50,
  showInMenu: true,
  rewriteBase: '',
  taxonomies: [],
});

export const isContentTypeCreationDraftDirty = (draft: ContentTypeCreationDraft): boolean =>
  Boolean(
    draft.key ||
      draft.label ||
      draft.singularLabel ||
      draft.description ||
      !draft.isPublic ||
      !draft.hasArchive ||
      draft.hierarchical ||
      draft.publication !== 'immediate' ||
      draft.supports.length !== 2 ||
      !draft.supports.includes('editor') ||
      !draft.supports.includes('featured_image') ||
      draft.archivePerPage !== 12 ||
      draft.defaultSort !== 'published_at_desc' ||
      draft.excludeFromSitemap ||
      draft.menuName ||
      draft.menuIcon ||
      draft.menuPosition !== 50 ||
      !draft.showInMenu ||
      draft.rewriteBase ||
      draft.taxonomies.length > 0,
  );

const copyDraft = (draft: ContentTypeCreationDraft): ContentTypeCreationDraft => ({
  ...draft,
  supports: [...draft.supports],
  taxonomies: [...draft.taxonomies],
});

interface ContentTypeFormProps {
  disabled: boolean;
  availableTaxonomies?: TaxonomyDefinition[];
  onCreate: (input: ContentTypeDefinitionWithSettings) => Promise<boolean>;
  onDirtyChange: (dirty: boolean) => void;
  initialDraft?: ContentTypeCreationDraft | null;
  onDraftChange?: (draft: ContentTypeCreationDraft) => void;
}

export const ContentTypeForm: React.FC<ContentTypeFormProps> = ({
  disabled,
  availableTaxonomies = [],
  onCreate,
  onDirtyChange,
  initialDraft,
  onDraftChange,
}) => {
  useI18n();
  const [draft, setDraft] = useState<ContentTypeCreationDraft>(() =>
    copyDraft(initialDraft ?? blankContentTypeCreationDraft()),
  );
  const draftRef = useRef(draft);
  const [error, setError] = useState('');
  const dirty = isContentTypeCreationDraftDirty(draft);

  useEffect(() => onDirtyChange(dirty), [dirty, onDirtyChange]);

  const updateDraft = (next: ContentTypeCreationDraft) => {
    draftRef.current = next;
    setDraft(next);
    onDraftChange?.(copyDraft(next));
  };
  const update = (patch: Partial<ContentTypeCreationDraft>) =>
    updateDraft({ ...draftRef.current, ...patch });

  const toggleSupport = (support: ContentSupport, enabled: boolean) => {
    update({
      supports: enabled
        ? [...draftRef.current.supports.filter(value => value !== support), support]
        : draftRef.current.supports.filter(value => value !== support),
    });
  };

  const toggleTaxonomy = (taxKey: string, enabled: boolean) => {
    update({
      taxonomies: enabled
        ? Array.from(new Set([...draftRef.current.taxonomies, taxKey]))
        : draftRef.current.taxonomies.filter(k => k !== taxKey),
    });
  };

  const buildSchema = (): ContentTypeDefinitionWithSettings => {
    const key = draft.key.trim();
    const label = draft.label.trim();
    if (!snakeKeyPattern.test(key)) {
      throw new Error(__("The CPT key must start with a lowercase letter and contain only lowercase letters, digits and underscores (up to 64 characters)."));
    }
    if (!label) throw new Error(__("Enter a content type name."));
    if (label.length > 100) throw new Error(__("The content type name must be at most 100 characters."));
    const perPage = Number(draft.archivePerPage);
    if (!Number.isInteger(perPage) || perPage < 1 || perPage > 100) {
      throw new Error(__("Items per archive page must be between 1 and 100."));
    }
    const menuPos = Number(draft.menuPosition);
    if (!Number.isInteger(menuPos) || menuPos < 0 || menuPos > 1000) {
      throw new Error(__("Menu position must be between 0 and 1000."));
    }
    const trimmedBase = draft.rewriteBase.trim();
    if (trimmedBase && !kebabBasePattern.test(trimmedBase)) {
      throw new Error(__("Rewrite base must use lowercase letters, numbers, and hyphens only (no slashes or placeholders)."));
    }

    return {
      key,
      label,
      singular_label: draft.singularLabel.trim() || null,
      description: draft.description.trim() || null,
      ...(draft.taxonomies.length > 0 ? { taxonomies: draft.taxonomies } : {}),
      ...(trimmedBase ? { rewrite_base: trimmedBase } : {}),
      settings: {
        public: draft.isPublic,
        has_archive: draft.hasArchive,
        hierarchical: draft.hierarchical,
        publication: draft.publication,
        supports: draft.supports,
        archive_per_page: perPage,
        default_sort: draft.defaultSort,
        exclude_from_sitemap: draft.excludeFromSitemap,
        menu_name: draft.menuName.trim() || null,
        menu_icon: draft.menuIcon || null,
        menu_position: menuPos,
        show_in_menu: draft.showInMenu,
        list_columns: [],
      },
    };
  };

  const submit = async (event: React.FormEvent) => {
    event.preventDefault();
    setError('');
    try {
      const schema = buildSchema();
      const created = await onCreate(schema);
      if (created) {
        updateDraft(blankContentTypeCreationDraft());
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : __("Could not read the CPT configuration."));
    }
  };

  const supportOptions: Array<{ key: ContentSupport; label: string }> = [
    { key: 'editor', label: __("Editor (editor)") },
    { key: 'featured_image', label: __("Featured image (featured_image)") },
    { key: 'builder', label: __("Visual Builder (builder)") },
    { key: 'excerpt', label: __("Excerpt (excerpt)") },
    { key: 'menu_order', label: __("Menu order (menu_order)") },
    { key: 'custom_template', label: __("Custom template selector") },
  ];

  return (
    <form onSubmit={submit} className="space-y-4">
      <div>
        <h3 className="text-base font-semibold text-zinc-950">{__("Register a new content type")}</h3>
        <p className="mt-0.5 text-xs text-zinc-600">
          {__("Define CPT identity, hierarchy, capabilities and menu visibility. Custom fields are attached separately via Field Groups.")}
        </p>
      </div>

      <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
        <label htmlFor="content-type-key" className="flex flex-col gap-1 text-xs font-medium text-zinc-800">
          <span>{__("CPT key *")}</span>
          <input
            id="content-type-key"
            type="text"
            className={`${INPUT_CLASS} font-mono`}
            value={draft.key}
            disabled={disabled}
            maxLength={64}
            onChange={e => update({ key: e.target.value })}
            placeholder={__("e.g. portfolio_item")}
          />
        </label>
        <label htmlFor="content-type-label" className="flex flex-col gap-1 text-xs font-medium text-zinc-800">
          <span>{__("Plural display name *")}</span>
          <input
            id="content-type-label"
            type="text"
            className={INPUT_CLASS}
            value={draft.label}
            disabled={disabled}
            maxLength={100}
            onChange={e => update({ label: e.target.value })}
            placeholder={__("Projects")}
          />
        </label>
        <label htmlFor="content-type-singular" className="flex flex-col gap-1 text-xs font-medium text-zinc-800">
          <span>{__("Singular label")}</span>
          <input
            id="content-type-singular"
            type="text"
            className={INPUT_CLASS}
            value={draft.singularLabel}
            disabled={disabled}
            maxLength={100}
            onChange={e => update({ singularLabel: e.target.value })}
            placeholder={__("Project")}
          />
        </label>
      </div>

      <label htmlFor="content-type-description" className="flex flex-col gap-1 text-xs font-medium text-zinc-800">
        <span>{__("Description")}</span>
        <input
          id="content-type-description"
          type="text"
          className={INPUT_CLASS}
          value={draft.description}
          disabled={disabled}
          maxLength={300}
          onChange={e => update({ description: e.target.value })}
          placeholder={__("Short description for editors and admin tools")}
        />
      </label>

      <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
        <fieldset className="space-y-3 rounded-lg border border-zinc-200 bg-white p-3.5">
          <legend className="px-1 text-xs font-semibold uppercase tracking-wide text-zinc-700">{__("Content type behavior")}</legend>
          <label className="flex items-center gap-2 text-sm text-zinc-800">
            <input type="checkbox" checked={draft.isPublic} disabled={disabled}
              onChange={e => update({ isPublic: e.target.checked })} />
            <span>{__("Public entries (public)")}</span>
          </label>
          <label className="flex items-center gap-2 text-sm text-zinc-800">
            <input type="checkbox" checked={draft.hasArchive} disabled={disabled}
              onChange={e => update({ hasArchive: e.target.checked })} />
            <span>{__("Enable archive (has_archive)")}</span>
          </label>
          <label className="flex items-center gap-2 text-sm text-zinc-800">
            <input type="checkbox" checked={draft.hierarchical} disabled={disabled}
              onChange={e => update({ hierarchical: e.target.checked })} />
            <span>{__("Hierarchical parent-child tree (hierarchical)")}</span>
          </label>
          <label className="flex items-center gap-2 text-sm text-zinc-800">
            <input type="checkbox" checked={draft.excludeFromSitemap} disabled={disabled}
              onChange={e => update({ excludeFromSitemap: e.target.checked })} />
            <span>{__("Exclude from sitemap.xml")}</span>
          </label>
          <div className="grid grid-cols-1 gap-2 sm:grid-cols-2">
            <label className="flex flex-col gap-1 text-xs font-medium text-zinc-800">
              <span>{__("Publishing workflow")}</span>
              <select className={SELECT_CLASS} value={draft.publication} disabled={disabled}
                onChange={e => update({ publication: e.target.value as ContentPublicationPolicy })}>
                <option value="immediate">{__("Publish immediately (immediate)")}</option>
                <option value="draft_publish">{__("Save draft / Publish")}</option>
              </select>
            </label>
            <label className="flex flex-col gap-1 text-xs font-medium text-zinc-800">
              <span>{__("URL rewrite base")}</span>
              <input type="text" className={`${INPUT_CLASS} font-mono`} value={draft.rewriteBase}
                disabled={disabled} maxLength={64} placeholder={draft.key || 'projects'}
                onChange={e => update({ rewriteBase: e.target.value })} />
            </label>
          </div>
          <div className="grid grid-cols-1 gap-2 sm:grid-cols-2">
            <label className="flex flex-col gap-1 text-xs font-medium text-zinc-800">
              <span>{__("Archive posts per page")}</span>
              <input type="number" min={1} max={100} className={INPUT_CLASS} value={draft.archivePerPage}
                disabled={disabled} onChange={e => update({ archivePerPage: Number(e.target.value) || 12 })} />
            </label>
            <label className="flex flex-col gap-1 text-xs font-medium text-zinc-800">
              <span>{__("Default archive sort")}</span>
              <select className={SELECT_CLASS} value={draft.defaultSort} disabled={disabled}
                onChange={e => update({ defaultSort: e.target.value as ContentDefaultSort })}>
                <option value="published_at_desc">{__("Newest published")}</option>
                <option value="created_at_desc">{__("Newest created")}</option>
                <option value="updated_at_desc">{__("Recently updated")}</option>
                <option value="title_asc">{__("Title (A-Z)")}</option>
                <option value="menu_order_asc">{__("Menu order ascending")}</option>
              </select>
            </label>
          </div>
        </fieldset>

        <div className="space-y-4">
          <fieldset className="space-y-2 rounded-lg border border-zinc-200 bg-white p-3.5">
            <legend className="px-1 text-xs font-semibold uppercase tracking-wide text-zinc-700">{__("Editor screen features")}</legend>
            <div className="grid grid-cols-1 gap-1.5 sm:grid-cols-2">
              {supportOptions.map(opt => (
                <label key={opt.key} className="flex items-center gap-2 text-sm text-zinc-800">
                  <input type="checkbox" checked={draft.supports.includes(opt.key)} disabled={disabled}
                    onChange={e => toggleSupport(opt.key, e.target.checked)} />
                  <span>{opt.label}</span>
                </label>
              ))}
            </div>
          </fieldset>

          <fieldset className="space-y-2.5 rounded-lg border border-zinc-200 bg-white p-3.5">
            <legend className="px-1 text-xs font-semibold uppercase tracking-wide text-zinc-700">{__("Sidebar menu visibility")}</legend>
            <label className="flex items-center gap-2 text-sm text-zinc-800">
              <input type="checkbox" checked={draft.showInMenu} disabled={disabled}
                onChange={e => update({ showInMenu: e.target.checked })} />
              <span>{__("Show in admin sidebar")}</span>
            </label>
            <div className="grid grid-cols-1 gap-2 sm:grid-cols-3">
              <label className="flex flex-col gap-1 text-xs font-medium text-zinc-800">
                <span>{__("Menu label override")}</span>
                <input type="text" className={INPUT_CLASS} value={draft.menuName} disabled={disabled}
                  maxLength={100} onChange={e => update({ menuName: e.target.value })} />
              </label>
              <label className="flex flex-col gap-1 text-xs font-medium text-zinc-800">
                <span>{__("Menu icon")}</span>
                <div className="flex items-center gap-1.5">
                  <span className="inline-flex h-9 w-9 shrink-0 items-center justify-center rounded border border-zinc-200 bg-zinc-50 text-zinc-700">
                    {draft.menuIcon ? <CoreIconSvg name={draft.menuIcon} size={16} /> : '—'}
                  </span>
                  <select className={SELECT_CLASS} value={draft.menuIcon} disabled={disabled}
                    onChange={e => update({ menuIcon: e.target.value })}>
                    <option value="">{__("Default")}</option>
                    {CORE_ICONS.map(icon => (
                      <option key={icon.name} value={icon.name}>{icon.name}</option>
                    ))}
                  </select>
                </div>
              </label>
              <label className="flex flex-col gap-1 text-xs font-medium text-zinc-800">
                <span>{__("Position (0-1000)")}</span>
                <input type="number" min={0} max={1000} className={INPUT_CLASS} value={draft.menuPosition}
                  disabled={disabled} onChange={e => update({ menuPosition: Number(e.target.value) || 0 })} />
              </label>
            </div>
          </fieldset>
        </div>
      </div>

      {availableTaxonomies.length > 0 && (
        <fieldset className="space-y-2 rounded-lg border border-zinc-200 bg-white p-3.5">
          <legend className="px-1 text-xs font-semibold uppercase tracking-wide text-zinc-700">{__("Attach taxonomies")}</legend>
          <div className="flex flex-wrap gap-2">
            {availableTaxonomies.filter(t => t.provider_status === 'active' && t.declaration_state === 'declared').map(tax => (
              <label key={tax.key} className="inline-flex items-center gap-1.5 rounded border border-zinc-200 bg-zinc-50 px-2.5 py-1 text-xs text-zinc-800">
                <input type="checkbox" checked={draft.taxonomies.includes(tax.key)} disabled={disabled}
                  onChange={e => toggleTaxonomy(tax.key, e.target.checked)} />
                <span>{tax.label} ({tax.key})</span>
              </label>
            ))}
          </div>
        </fieldset>
      )}

      {error && <AdminAlert tone="error">{error}</AdminAlert>}
      <div>
        <AdminButton type="submit" variant="primary" size="md" disabled={disabled}>
          {__("Create content type")}
        </AdminButton>
      </div>
    </form>
  );
};
