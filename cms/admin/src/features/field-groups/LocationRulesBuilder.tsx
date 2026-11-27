import React, { useEffect, useState } from 'react';
import { listContentTypes, type OwnedContentTypeWithSettings } from '../../api/content.ts';
import { listTaxonomyDefinitions, type TaxonomyDefinition } from '../../api/taxonomy.ts';
import { getActiveThemeLayouts, getThemes, type ThemeLayoutCatalog } from '../../api/themes.ts';
import type {
  FieldGroupDomain,
  LocationOp,
  LocationParam,
  LocationRule,
  LocationRuleGroups,
} from '../content/types.ts';
import { AdminBadge } from '../../components/AdminPrimitives.tsx';
import { __, useI18n } from '../../i18n/index.tsx';
import { inferDomainFromLocation } from './fieldBuilderHelpers.ts';

const CONTROL =
  'h-8 w-full min-w-0 rounded border border-zinc-300 bg-white px-2 text-xs text-zinc-900 focus-visible:outline-2 focus-visible:outline-od-accent';

interface LocationRulesBuilderProps {
  location: LocationRuleGroups;
  disabled?: boolean;
  onChange: (next: LocationRuleGroups) => void;
}

function defaultRuleForDomain(domain: FieldGroupDomain): LocationRule {
  if (domain === 'term') return { param: 'taxonomy_term', op: 'eq', value: 'category' };
  if (domain === 'options_page') return { param: 'options_page', op: 'eq', value: 'site_global' };
  return { param: 'content_type', op: 'eq', value: 'post' };
}

function allowedParamsForDomain(domain: FieldGroupDomain): Array<{ param: LocationParam; label: string }> {
  if (domain === 'term') {
    return [{ param: 'taxonomy_term', label: 'Taxonomy Term Screen' }];
  }
  if (domain === 'options_page') {
    return [{ param: 'options_page', label: 'Options Page' }];
  }
  return [
    { param: 'content_type', label: 'Content Type' },
    { param: 'page_layout', label: 'Page Layout' },
    { param: 'custom_template', label: 'Custom Template' },
    { param: 'special_page', label: 'Special Page' },
    { param: 'entry_taxonomy', label: 'Entry Has Term' },
    { param: 'entry_id', label: 'Entry ID' },
    { param: 'user_role', label: 'User Role' },
  ];
}

export const LocationRulesBuilder: React.FC<LocationRulesBuilderProps> = ({
  location,
  disabled = false,
  onChange,
}) => {
  useI18n();
  const domain = inferDomainFromLocation(location);
  const [contentTypes, setContentTypes] = useState<OwnedContentTypeWithSettings[]>([]);
  const [taxonomies, setTaxonomies] = useState<TaxonomyDefinition[]>([]);
  const [layouts, setLayouts] = useState<ThemeLayoutCatalog | null>(null);
  const [themeIds, setThemeIds] = useState<string[]>([]);

  useEffect(() => {
    let alive = true;
    void Promise.all([
      listContentTypes().catch(() => []),
      listTaxonomyDefinitions().catch(() => []),
      getActiveThemeLayouts().catch(() => null),
      getThemes().catch(() => null),
    ]).then(([cts, taxes, themeLayoutSnap, themesSnap]) => {
      if (!alive) return;
      setContentTypes(cts.filter(ct => ct.descriptor.key !== 'pattern' && ct.descriptor.key !== 'section'));
      setTaxonomies(taxes);
      setLayouts(themeLayoutSnap);
      if (themesSnap) {
        const uniq = Array.from(new Set(themesSnap.revisions.map(r => r.theme_id)));
        setThemeIds(uniq);
      }
    });
    return () => { alive = false; };
  }, []);

  const switchDomain = (nextDomain: FieldGroupDomain) => {
    if (nextDomain === domain) return;
    onChange([[defaultRuleForDomain(nextDomain)]]);
  };

  const updateBranch = (orIdx: number, nextBranch: LocationRule[]) => {
    const next = location
      .map((branch, idx) => (idx === orIdx ? nextBranch : branch))
      .filter(branch => branch.length > 0);
    if (next.length === 0) return;
    onChange(next);
  };

  const renderValueControl = (rule: LocationRule, orIdx: number, andIdx: number) => {
    const setValue = (val: string) =>
      updateBranch(orIdx, location[orIdx].map((r, i) => (i === andIdx ? { ...r, value: val } : r)));

    switch (rule.param) {
      case 'content_type':
        return (
          <select aria-label={__("Rule value")} className={CONTROL} value={rule.value} disabled={disabled}
            onChange={e => setValue(e.target.value)}>
            {rule.op === 'eq' && <option value="all">{__("All content types (all)")}</option>}
            {contentTypes.map(ct => (
              <option key={ct.descriptor.key} value={ct.descriptor.key}>
                {ct.descriptor.label} ({ct.descriptor.key})
              </option>
            ))}
            {rule.value && rule.value !== 'all' && !contentTypes.some(ct => ct.descriptor.key === rule.value) && (
              <option value={rule.value}>{rule.value}</option>
            )}
          </select>
        );
      case 'taxonomy_term':
        return (
          <select aria-label={__("Rule value")} className={CONTROL} value={rule.value} disabled={disabled}
            onChange={e => setValue(e.target.value)}>
            {rule.op === 'eq' && <option value="all">{__("All taxonomies (all)")}</option>}
            {taxonomies.map(tax => (
              <option key={tax.key} value={tax.key}>{tax.label} ({tax.key})</option>
            ))}
            {rule.value && rule.value !== 'all' && !taxonomies.some(t => t.key === rule.value) && (
              <option value={rule.value}>{rule.value}</option>
            )}
          </select>
        );
      case 'special_page':
        return (
          <select aria-label={__("Rule value")} className={CONTROL} value={rule.value} disabled={disabled}
            onChange={e => setValue(e.target.value)}>
            <option value="homepage">{__("Homepage (homepage)")}</option>
            <option value="posts_page">{__("Posts Page (posts_page)")}</option>
          </select>
        );
      case 'user_role':
        return (
          <select aria-label={__("Rule value")} className={CONTROL} value={rule.value} disabled={disabled}
            onChange={e => setValue(e.target.value)}>
            <option value="admin">{__("Administrator (admin)")}</option>
            <option value="editor">{__("Editor (editor)")}</option>
          </select>
        );
      case 'page_layout': {
        const list = layouts?.page_layouts ?? [];
        return (
          <input type="text" list={`page-layouts-${orIdx}-${andIdx}`} aria-label={__("Rule value")}
            className={`${CONTROL} font-mono`} value={rule.value} disabled={disabled}
            onChange={e => setValue(e.target.value)}
            placeholder={list[0]?.key ?? 'landing'} />
        );
      }
      case 'custom_template': {
        const list = layouts?.custom_templates ?? [];
        return (
          <input type="text" list={`custom-templates-${orIdx}-${andIdx}`} aria-label={__("Rule value")}
            className={`${CONTROL} font-mono`} value={rule.value} disabled={disabled}
            onChange={e => setValue(e.target.value)}
            placeholder={list[0]?.custom_key ?? 'article_wide'} />
        );
      }
      case 'options_page':
        return (
          <div className="space-y-1">
            <input type="text" aria-label={__("Options page target")} className={`${CONTROL} font-mono`}
              value={rule.value} disabled={disabled}
              placeholder="site_global / theme:<id> / custom:<key>"
              onChange={e => setValue(e.target.value)} />
            {themeIds.length > 0 && (
              <div className="flex flex-wrap gap-1 text-[10px]">
                <button type="button" disabled={disabled} onClick={() => setValue('site_global')}
                  className="rounded border border-zinc-200 bg-white px-1.5 py-0.5 text-zinc-600 hover:bg-zinc-50">
                  site_global
                </button>
                {themeIds.slice(0, 3).map(tid => (
                  <button key={tid} type="button" disabled={disabled} onClick={() => setValue(`theme:${tid}`)}
                    className="rounded border border-zinc-200 bg-white px-1.5 py-0.5 text-zinc-600 hover:bg-zinc-50">
                    theme:{tid}
                  </button>
                ))}
              </div>
            )}
          </div>
        );
      case 'entry_taxonomy':
        return (
          <input type="text" aria-label={__("Rule value")} className={`${CONTROL} font-mono`}
            value={rule.value} disabled={disabled} placeholder="category:1"
            onChange={e => setValue(e.target.value)} />
        );
      case 'entry_id':
        return (
          <input type="number" min={1} aria-label={__("Rule value")} className={CONTROL}
            value={rule.value} disabled={disabled} placeholder="123"
            onChange={e => setValue(e.target.value)} />
        );
    }
  };

  const paramChoices = allowedParamsForDomain(domain);

  return (
    <fieldset className="space-y-3 rounded-lg border border-zinc-200 bg-white p-3.5" data-testid="location-rules-builder">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div className="flex flex-wrap items-center gap-2">
          <legend className="text-xs font-semibold uppercase tracking-wide text-zinc-700">
            {__("Location rules")}
          </legend>
          <AdminBadge tone="neutral">{domain}</AdminBadge>
        </div>
        <div className="flex flex-wrap items-center gap-1.5 text-xs">
          {([
            ['content', __("Content")],
            ['term', __("Taxonomy Term")],
            ['options_page', __("Options Page")],
          ] as const).map(([dom, label]) => (
            <button
              key={dom}
              type="button"
              disabled={disabled}
              onClick={() => switchDomain(dom)}
              className={
                'h-7 rounded px-2.5 text-xs font-medium ' +
                (domain === dom ? 'bg-zinc-900 text-white' : 'border border-zinc-300 bg-white text-zinc-700 hover:bg-zinc-50')
              }
            >
              {label}
            </button>
          ))}
        </div>
      </div>

      <p className="text-xs text-zinc-500">
        {__("Every rule within a group uses AND; multiple groups use OR. Rules inside a group cannot mix Content, Term, and Options Page targets.")}
      </p>

      <div className="space-y-2.5">
        {location.map((branch, orIdx) => (
          <div key={orIdx} className="space-y-2 rounded-md border border-zinc-200 bg-zinc-50/70 p-2.5">
            <div className="flex items-center justify-between text-[11px] font-semibold uppercase tracking-wider text-zinc-500">
              <span>{orIdx === 0 ? __("Show this field group if") : __("OR")}</span>
              {location.length > 1 && (
                <button type="button" disabled={disabled}
                  onClick={() => updateBranch(orIdx, [])}
                  className="text-rose-600 hover:underline">
                  {__("Remove OR group")}
                </button>
              )}
            </div>

            {branch.map((rule, andIdx) => (
              <div key={andIdx} className="grid grid-cols-1 gap-2 sm:grid-cols-[170px_110px_1fr_auto]">
                <select
                  aria-label={__("Location parameter")}
                  className={CONTROL}
                  value={rule.param}
                  disabled={disabled}
                  onChange={e => {
                    const param = e.target.value as LocationParam;
                    const defaultVal =
                      param === 'content_type' ? 'post'
                        : param === 'taxonomy_term' ? 'category'
                          : param === 'options_page' ? 'site_global'
                            : param === 'special_page' ? 'homepage'
                              : param === 'user_role' ? 'admin'
                                : param === 'entry_taxonomy' ? 'category:1'
                                  : param === 'entry_id' ? '1' : '';
                    updateBranch(
                      orIdx,
                      branch.map((r, i) => (i === andIdx ? { param, op: 'eq', value: defaultVal } : r)),
                    );
                  }}
                >
                  {paramChoices.map(p => (
                    <option key={p.param} value={p.param}>{p.label}</option>
                  ))}
                </select>

                <select
                  aria-label={__("Location operator")}
                  className={CONTROL}
                  value={rule.op}
                  disabled={disabled || domain === 'options_page'}
                  onChange={e =>
                    updateBranch(
                      orIdx,
                      branch.map((r, i) => (i === andIdx ? { ...r, op: e.target.value as LocationOp } : r)),
                    )
                  }
                >
                  <option value="eq">{__("is equal to")}</option>
                  {domain !== 'options_page' && <option value="neq">{__("is not equal to")}</option>}
                </select>

                {renderValueControl(rule, orIdx, andIdx)}

                <div className="flex items-start gap-1">
                  <button
                    type="button"
                    disabled={disabled || branch.length >= 16}
                    onClick={() => updateBranch(orIdx, [...branch, defaultRuleForDomain(domain)])}
                    className="h-8 rounded border border-zinc-300 bg-white px-2 text-[11px] font-medium text-zinc-700 hover:bg-zinc-50"
                  >
                    + AND
                  </button>
                  {branch.length > 1 && (
                    <button
                      type="button"
                      disabled={disabled}
                      onClick={() => updateBranch(orIdx, branch.filter((_, i) => i !== andIdx))}
                      className="h-8 rounded border border-rose-200 bg-white px-2 text-[11px] font-medium text-rose-600 hover:bg-rose-50"
                    >
                      ×
                    </button>
                  )}
                </div>
              </div>
            ))}
          </div>
        ))}
      </div>

      {location.length < 16 && (
        <button
          type="button"
          disabled={disabled}
          onClick={() => onChange([...location, [defaultRuleForDomain(domain)]])}
          className="rounded border border-zinc-300 bg-white px-3 py-1.5 text-xs font-medium text-zinc-800 hover:bg-zinc-50"
        >
          {__("+ Add rule group (OR)")}
        </button>
      )}
    </fieldset>
  );
};
