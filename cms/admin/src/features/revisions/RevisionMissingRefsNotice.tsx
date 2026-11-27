import React from 'react';
import { __, _n } from '../../i18n/index.tsx';
import {
  hasAnyMissingRefsInfo,
  hasBlockingMissingRefs,
  type MissingRefsReport,
} from '../../api/revisions.ts';

export interface RevisionMissingRefsNoticeProps {
  report: MissingRefsReport;
  heading?: string;
  variant?: 'warning' | 'info';
  testId?: string;
}

export const RevisionMissingRefsNotice: React.FC<RevisionMissingRefsNoticeProps> = ({
  report,
  heading,
  variant = 'warning',
  testId,
}) => {
  if (!hasAnyMissingRefsInfo(report)) return null;

  const isWarning = variant === 'warning' && hasBlockingMissingRefs(report);
  const boxClass = isWarning
    ? 'rounded-lg border border-amber-300 bg-amber-50 p-3 text-xs text-amber-950 space-y-1.5'
    : 'rounded-lg border border-zinc-200 bg-zinc-50 p-3 text-xs text-zinc-800 space-y-1.5';

  const items: string[] = [];

  if (report.missing_media.length > 0) {
    items.push(
      __("Missing media ({{value1}}): {{value2}}", {
        value1: report.missing_media.length,
        value2: report.missing_media
          .slice(0, 5)
          .map(m => `${m.media_id} (${m.location})`)
          .join(', '),
      }),
    );
  }
  if (report.missing_term_ids.length > 0) {
    items.push(
      _n(
        "Missing taxonomy term ID: {{value1}}",
        "Missing taxonomy term IDs: {{value1}}",
        report.missing_term_ids.length,
        { value1: report.missing_term_ids.slice(0, 8).join(', ') },
      ),
    );
  }
  if (report.missing_entry_ids.length > 0) {
    items.push(
      __("Missing referenced entry IDs: {{value1}}", {
        value1: report.missing_entry_ids.slice(0, 8).join(', '),
      }),
    );
  }
  if (report.missing_pattern_ids.length > 0) {
    items.push(
      __("Missing Pattern IDs: {{value1}}", {
        value1: report.missing_pattern_ids.slice(0, 8).join(', '),
      }),
    );
  }
  if (report.missing_contact_form_ids.length > 0) {
    items.push(
      __("Missing contact form IDs: {{value1}}", {
        value1: report.missing_contact_form_ids.slice(0, 8).join(', '),
      }),
    );
  }
  if (report.missing_dynamic_options.length > 0) {
    items.push(
      __("Missing dynamic options ({{value1}}): {{value2}}", {
        value1: report.missing_dynamic_options.length,
        value2: report.missing_dynamic_options
          .slice(0, 5)
          .map(opt => `${opt.field_path}=${opt.value}`)
          .join(', '),
      }),
    );
  }
  if (report.missing_list_refs.length > 0) {
    items.push(
      __("Invalid List block filters ({{value1}}): {{value2}}", {
        value1: report.missing_list_refs.length,
        value2: report.missing_list_refs
          .slice(0, 5)
          .map(item => `${item.node_id} (${item.reason})`)
          .join(', '),
      }),
    );
  }
  if (report.missing_block_types.length > 0) {
    items.push(
      __("Unavailable Builder blocks ({{value1}}): {{value2}}", {
        value1: report.missing_block_types.length,
        value2: report.missing_block_types
          .slice(0, 5)
          .map(item => `${item.block_id} (${item.reason})`)
          .join(', '),
      }),
    );
  }
  if (report.cleared_binding_nodes.length > 0) {
    items.push(
      __("Field bindings that will be cleared ({{value1}}): {{value2}}", {
        value1: report.cleared_binding_nodes.length,
        value2: report.cleared_binding_nodes
          .slice(0, 5)
          .map(item => `${item.node_id} -> ${item.field_key}`)
          .join(', '),
      }),
    );
  }
  if (report.missing_templates.length > 0) {
    items.push(
      __("Missing theme templates/layouts: {{value1}}", {
        value1: report.missing_templates.map(t => `${t.kind}:${t.key}`).join(', '),
      }),
    );
  }
  if (report.dropped_field_keys.length > 0) {
    items.push(
      __("Removed schema fields ignored on restore: {{value1}}", {
        value1: report.dropped_field_keys.join(', '),
      }),
    );
  }
  if (report.dropped_block_field_keys.length > 0) {
    items.push(
      __("Removed block properties ignored on restore: {{value1}}", {
        value1: report.dropped_block_field_keys
          .slice(0, 5)
          .map(item => `${item.block_type}.${item.field_path}`)
          .join(', '),
      }),
    );
  }
  if (report.non_editable_fields.length > 0) {
    items.push(
      __("Restricted fields preserved from current entry: {{value1}}", {
        value1: report.non_editable_fields.join(', '),
      }),
    );
  }

  return (
    <div className={boxClass} data-testid={testId}>
      <p className="font-semibold">
        {heading ??
          (isWarning
            ? __("Missing or changed references detected")
            : __("Snapshot normalization notes"))}
      </p>
      <ul className="list-disc pl-4 space-y-0.5">
        {items.map((line, index) => (
          <li key={index}>{line}</li>
        ))}
      </ul>
    </div>
  );
};
