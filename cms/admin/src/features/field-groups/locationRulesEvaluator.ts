import type {
  LocationOp,
  LocationRule,
  LocationRuleGroups,
} from '../content/types.ts';

export type LocationUserRole = 'admin' | 'editor';

export interface LocationEvalContext {
  content_type?: string | null;
  taxonomy_term?: string | null;
  options_page?: string | null;
  page_layout?: string | null;
  custom_template?: string | null;
  special_page?: string | null;
  entry_term_pairs?: ReadonlyArray<readonly [string, number]>;
  entry_id?: number | null;
  user_role: LocationUserRole;
}

function matchEqOpt(op: LocationOp, matched: boolean): boolean {
  return op === 'eq' ? matched : !matched;
}

function matchKeyOrAll(op: LocationOp, ruleValue: string, actual: string): boolean {
  if (ruleValue === 'all') {
    return op === 'eq';
  }
  return matchEqOpt(op, actual === ruleValue);
}

function parseEntryTaxonomyValue(raw: string): [string, number] | null {
  const colonIdx = raw.indexOf(':');
  if (colonIdx <= 0 || colonIdx === raw.length - 1) return null;
  const taxKey = raw.slice(0, colonIdx);
  const termRaw = raw.slice(colonIdx + 1);
  if (!/^[a-z][a-z0-9_]{0,63}$/.test(taxKey) || !/^[1-9]\d*$/.test(termRaw)) {
    return null;
  }
  const termId = Number(termRaw);
  return Number.isSafeInteger(termId) && termId > 0 ? [taxKey, termId] : null;
}

function parsePositiveId(raw: string): number | null {
  if (!/^[1-9]\d*$/.test(raw)) return null;
  const parsed = Number(raw);
  return Number.isSafeInteger(parsed) && parsed > 0 ? parsed : null;
}

function evalSingleRule(rule: LocationRule, ctx: LocationEvalContext): boolean {
  switch (rule.param) {
    case 'content_type': {
      const ct = ctx.content_type;
      if (!ct || ct === 'pattern' || ct === 'section') return false;
      return matchKeyOrAll(rule.op, rule.value, ct);
    }
    case 'taxonomy_term': {
      const tax = ctx.taxonomy_term;
      if (!tax) return false;
      return matchKeyOrAll(rule.op, rule.value, tax);
    }
    case 'options_page':
      return rule.op === 'eq' && ctx.options_page === rule.value;
    case 'page_layout':
      return matchEqOpt(rule.op, (ctx.page_layout ?? null) === rule.value);
    case 'custom_template':
      return matchEqOpt(rule.op, (ctx.custom_template ?? null) === rule.value);
    case 'special_page':
      return matchEqOpt(rule.op, (ctx.special_page ?? null) === rule.value);
    case 'entry_taxonomy': {
      const parsed = parseEntryTaxonomyValue(rule.value);
      if (!parsed) return false;
      const [taxKey, termId] = parsed;
      const hasPair = Boolean(
        ctx.entry_term_pairs?.some(([t, id]) => t === taxKey && id === termId),
      );
      return matchEqOpt(rule.op, hasPair);
    }
    case 'entry_id': {
      const id = parsePositiveId(rule.value);
      if (id === null) return false;
      return matchEqOpt(rule.op, ctx.entry_id === id);
    }
    case 'user_role':
      return matchEqOpt(rule.op, ctx.user_role === rule.value);
    default:
      return false;
  }
}

/**
 * Evaluates Disjunctive Normal Form (OR-of-ANDs) location rules identically to
 * `opendash_core::content::location_rules::evaluate_location_rules`.
 */
export function evaluateLocationRules(
  groups: LocationRuleGroups | null | undefined,
  ctx: LocationEvalContext,
): boolean {
  if (!groups || groups.length === 0) return true;
  return groups.some(
    branch => branch.length > 0 && branch.every(rule => evalSingleRule(rule, ctx)),
  );
}
