import { useI18n } from '../i18n/index.tsx';
import type { BlockPropValue } from '../types.ts';
import type { BuilderElement } from './document.ts';
import { STUDIO_INPUT_CLASS, ToggleSwitchControl } from './StudioFieldControls.tsx';

interface Props {
  element: BuilderElement;
  onChange: (id: string, key: string, value: BlockPropValue | undefined) => void;
}

export function BuilderSearchFields({ element, onChange }: Props) {
  const { __ } = useI18n();

  if (element.type === 'core/search_form') {
    const placeholder = typeof element.props.placeholder === 'string' ? element.props.placeholder : '';
    const submitLabel = typeof element.props.submit_label === 'string'
      ? element.props.submit_label
      : __("Search");
    return (
      <div className="mt-4 space-y-3 border-t border-slate-200 pt-3">
        <h3 className="text-xs font-semibold uppercase tracking-wide text-slate-500">{__("Search form")}</h3>
        <label className="block space-y-1">
          <span className="block text-xs font-medium text-zinc-700">{__("Placeholder")}</span>
          <input
            type="text"
            maxLength={120}
            className={STUDIO_INPUT_CLASS}
            value={placeholder}
            placeholder={__("Search the site...")}
            onChange={(e) => onChange(element.id, 'placeholder', e.target.value)}
          />
        </label>
        <label className="block space-y-1">
          <span className="block text-xs font-medium text-zinc-700">{__("Submit button label")}</span>
          <input
            type="text"
            maxLength={64}
            className={STUDIO_INPUT_CLASS}
            value={submitLabel}
            onChange={(e) => onChange(element.id, 'submit_label', e.target.value)}
          />
        </label>
        <ToggleSwitchControl
          id={`search-form-show-type-${element.id}`}
          label={__("Show type filter")}
          checked={element.props.show_type_filter === true}
          onChange={(checked) => onChange(element.id, 'show_type_filter', checked)}
        />
        <ToggleSwitchControl
          id={`search-form-preserve-type-${element.id}`}
          label={__("Preserve type filter")}
          checked={element.props.preserve_type_filter === true}
          onChange={(checked) => onChange(element.id, 'preserve_type_filter', checked)}
        />
      </div>
    );
  }

  if (element.type === 'core/search_results') {
    const emptyMessage = typeof element.props.empty_message === 'string' ? element.props.empty_message : '';
    return (
      <div className="mt-4 space-y-3 border-t border-slate-200 pt-3">
        <h3 className="text-xs font-semibold uppercase tracking-wide text-slate-500">{__("Search results")}</h3>
        <ToggleSwitchControl
          id={`search-results-facets-${element.id}`}
          label={__("Show content type facets")}
          checked={element.props.show_facets !== false}
          onChange={(checked) => onChange(element.id, 'show_facets', checked)}
        />
        <ToggleSwitchControl
          id={`search-results-excerpt-${element.id}`}
          label={__("Show excerpt / snippet")}
          checked={element.props.show_excerpt !== false}
          onChange={(checked) => onChange(element.id, 'show_excerpt', checked)}
        />
        <label className="block space-y-1">
          <span className="block text-xs font-medium text-zinc-700">{__("Empty state message")}</span>
          <input
            type="text"
            maxLength={240}
            className={STUDIO_INPUT_CLASS}
            value={emptyMessage}
            placeholder={__("No matching results found.")}
            onChange={(e) => onChange(element.id, 'empty_message', e.target.value)}
          />
        </label>
      </div>
    );
  }

  return null;
}
