import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { after, describe, it } from 'node:test';
import { fileURLToPath, pathToFileURL } from 'node:url';
import ts from 'typescript';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const SRC_FIELDS_DIR = path.resolve(__dirname, '../src/fields');

const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'opendash-field-engine-'));
fs.writeFileSync(path.join(tmpDir, 'package.json'), JSON.stringify({ type: 'module' }));

function transpileTsFile(sourcePath, name) {
  const source = fs.readFileSync(sourcePath, 'utf8').replace(
    /(from\s+['"]\.\/[a-zA-Z0-9_]+)\.ts(['"])/g,
    '$1.mjs$2',
  ).replace(
    /(from\s+['"]\.\.\/[a-zA-Z0-9_/-]+)\.ts(['"])/g,
    (m, p1, p2) => `${p1}.mjs${p2}`,
  );
  const { outputText } = ts.transpileModule(source, {
    compilerOptions: {
      module: ts.ModuleKind.ESNext,
      target: ts.ScriptTarget.ES2022,
      isolatedModules: true,
    },
    fileName: sourcePath,
  });
  const outPath = path.join(tmpDir, `${name}.mjs`);
  fs.mkdirSync(path.dirname(outPath), { recursive: true });
  fs.writeFileSync(outPath, outputText, 'utf8');
  return pathToFileURL(outPath).href;
}

const SRC_FIELD_GROUPS_DIR = path.resolve(__dirname, '../src/features/field-groups');
const typesUrl = transpileTsFile(path.join(SRC_FIELDS_DIR, 'types.ts'), 'fields/types');
const iconsUrl = transpileTsFile(path.join(SRC_FIELDS_DIR, 'icons.ts'), 'fields/icons');
const helpersUrl = transpileTsFile(path.join(SRC_FIELDS_DIR, 'helpers.ts'), 'fields/helpers');
const conditionsUrl = transpileTsFile(path.join(SRC_FIELDS_DIR, 'conditions.ts'), 'fields/conditions');
const locEvalUrl = transpileTsFile(path.join(SRC_FIELD_GROUPS_DIR, 'locationRulesEvaluator.ts'), 'features/field-groups/locationRulesEvaluator');
const builderHelpersUrl = transpileTsFile(path.join(SRC_FIELD_GROUPS_DIR, 'fieldBuilderHelpers.ts'), 'features/field-groups/fieldBuilderHelpers');

await import(typesUrl);
const icons = await import(iconsUrl);
const helpers = await import(helpersUrl);
const conditions = await import(conditionsUrl);
const locEval = await import(locEvalUrl);
const builderHelpers = await import(builderHelpersUrl);

after(() => {
  fs.rmSync(tmpDir, { recursive: true, force: true });
});

describe('CORE_ICONS parity', () => {
  it('exposes exactly 48 unique icons with safe SVG primitives', () => {
    assert.equal(icons.CORE_ICONS.length, 48);
    assert.equal(new Set(icons.CORE_ICON_NAMES).size, 48);
    assert.equal(icons.lookupCoreIconPaths('star')?.includes('<path'), true);
    assert.equal(icons.lookupCoreIconPaths('missing-icon'), null);
  });
});

describe('helpers.ts pure logic', () => {
  it('computes canonicalEmptyValue across all 25 data types and 3 UI layouts', () => {
    const cases = [
      [{ type: 'text', key: 'a', label: 'A' }, ''],
      [{ type: 'textarea', key: 'a', label: 'A' }, ''],
      [{ type: 'email', key: 'a', label: 'A' }, ''],
      [{ type: 'url', key: 'a', label: 'A' }, ''],
      [{ type: 'richtext', key: 'a', label: 'A' }, ''],
      [{ type: 'color', key: 'a', label: 'A' }, ''],
      [{ type: 'icon', key: 'a', label: 'A' }, ''],
      [{ type: 'date', key: 'a', label: 'A' }, ''],
      [{ type: 'datetime', key: 'a', label: 'A' }, ''],
      [{ type: 'time', key: 'a', label: 'A' }, ''],
      [{ type: 'radio_buttons', key: 'a', label: 'A' }, ''],
      [{ type: 'select', key: 'a', label: 'A' }, ''],
      [{ type: 'select', key: 'a', label: 'A', multiple: true }, []],
      [{ type: 'checkbox', key: 'a', label: 'A' }, []],
      [{ type: 'gallery', key: 'a', label: 'A' }, []],
      [{ type: 'repeater', key: 'a', label: 'A' }, []],
      [{ type: 'entry_ref', key: 'a', label: 'A' }, null],
      [{ type: 'entry_ref', key: 'a', label: 'A', multiple: true }, []],
      [{ type: 'term_ref', key: 'a', label: 'A' }, null],
      [{ type: 'term_ref', key: 'a', label: 'A', multiple: true }, []],
      [{ type: 'boolean', key: 'a', label: 'A' }, false],
      [{ type: 'group', key: 'a', label: 'A' }, {}],
      [{ type: 'spacing', key: 'a', label: 'A' }, {}],
      [{ type: 'link', key: 'a', label: 'A' }, { url: '', label: '', new_tab: false, nofollow: false }],
      [{ type: 'number', key: 'a', label: 'A' }, null],
      [{ type: 'range', key: 'a', label: 'A' }, null],
      [{ type: 'media', key: 'a', label: 'A' }, null],
      [{ type: 'secret', key: 'a', label: 'A' }, null],
      [{ type: 'tab', key: 't', label: 'T' }, null],
      [{ type: 'accordion', key: 'ac', label: 'Ac' }, null],
      [{ type: 'message', key: 'm', label: 'M' }, null],
    ];
    for (const [spec, expected] of cases) {
      assert.deepEqual(helpers.canonicalEmptyValue(spec), expected);
    }
  });

  it('skips UI-only items in initialValuesFromFields and flattenDataFields while cloning nested objects', () => {
    const specs = [
      { type: 'tab', key: 't1', label: 'General' },
      { type: 'accordion', key: 'a1', label: 'Box' },
      { type: 'message', key: 'm1', label: 'Info', body: 'Hello' },
      { type: 'text', key: 'title', label: 'Title', default: 'Default' },
      {
        type: 'group',
        key: 'card',
        label: 'Card',
        fields: [
          { type: 'boolean', key: 'visible', label: 'Visible', default: true },
          { type: 'message', key: 'sub_msg', label: 'Sub', body: 'Note' },
        ],
      },
      { type: 'accordion', key: 'a_end', label: '', endpoint: true },
    ];

    const flat = helpers.flattenDataFields(specs);
    assert.deepEqual(flat.map(f => f.key), ['title', 'card']);
    const leaves = helpers.collectLeafDataFields(specs);
    assert.deepEqual(leaves.map(f => f.key), ['title', 'visible']);

    const initDefault = helpers.initialValuesFromFields(specs);
    assert.deepEqual(initDefault, { title: 'Default', card: { visible: true } });
    assert.equal('t1' in initDefault, false);

    const saved = { card: { visible: false } };
    const merged = helpers.initialValuesFromFields(specs, saved);
    assert.deepEqual(merged, { title: 'Default', card: { visible: false } });
    merged.card.visible = true;
    assert.equal(saved.card.visible, false);
  });

  it('normalizes spacing values for padding vs negative margin and -0', () => {
    assert.deepEqual(
      helpers.normalizeSpacingValue({ top: -16, right: -0, bottom: 300, left: 8 }, 'padding'),
      { top: 0, right: 0, bottom: 256, left: 8 },
    );
    assert.equal(
      Object.is(helpers.normalizeSpacingValue({ top: -0 }, 'margin').top, -0),
      false,
    );
    assert.deepEqual(
      helpers.normalizeSpacingValue({ top: -64, right: 32, bottom: -300 }, 'margin', -256, 256),
      { top: -64, right: 32, bottom: -256 },
    );
    assert.deepEqual(
      helpers.normalizeSpacingValue({ top: -200, bottom: -300 }, 'margin'),
      { top: -128, bottom: -128 },
    );
  });

  it('formats and parses datetime across UTC, +07:00, and DST timezones', () => {
    assert.equal(
      helpers.formatDateTimeForSiteTz('2026-11-26T08:30:00Z', 'Asia/Ho_Chi_Minh'),
      '2026-11-26T15:30',
    );
    assert.equal(
      helpers.parseSiteTzToUtcIso('2026-11-26T15:30', 'Asia/Ho_Chi_Minh'),
      '2026-11-26T08:30:00Z',
    );
    // Summer EDT (-04:00) vs Winter EST (-05:00)
    assert.equal(
      helpers.parseSiteTzToUtcIso('2026-07-01T10:00', 'America/New_York'),
      '2026-07-01T14:00:00Z',
    );
    assert.equal(
      helpers.parseSiteTzToUtcIso('2026-12-01T10:00', 'America/New_York'),
      '2026-12-01T15:00:00Z',
    );
    // Rejects nonexistent calendar dates
    assert.equal(helpers.parseSiteTzToUtcIso('2026-02-30T10:00', 'UTC'), '');
  });
});

describe('conditions.ts DNF evaluation parity with Rust', () => {
  it('evaluates all 8 operators in DNF OR/AND form', () => {
    const specs = [
      { type: 'select', key: 'mode', label: 'Mode', options: [{ value: 'a', label: 'A' }, { value: 'b', label: 'B' }] },
      { type: 'number', key: 'count', label: 'Count' },
      { type: 'text', key: 'note', label: 'Note' },
      { type: 'checkbox', key: 'tags', label: 'Tags', options: [{ value: 'x', label: 'X' }] },
    ];
    const values = { mode: 'b', count: 15, note: 'keep_value', tags: ['x'] };

    assert.equal(conditions.evaluateConditions([[{ field: 'mode', op: 'eq', value: 'b' }]], specs, values, specs, values), true);
    assert.equal(conditions.evaluateConditions([[{ field: 'mode', op: 'neq', value: 'a' }]], specs, values, specs, values), true);
    assert.equal(conditions.evaluateConditions([[{ field: 'mode', op: 'in', value: ['b', 'c'] }]], specs, values, specs, values), true);
    assert.equal(conditions.evaluateConditions([[{ field: 'tags', op: 'contains', value: 'x' }]], specs, values, specs, values), true);
    assert.equal(conditions.evaluateConditions([[{ field: 'note', op: 'contains', value: 'keep' }]], specs, values, specs, values), true);
    assert.equal(conditions.evaluateConditions([[{ field: 'note', op: 'not_empty' }]], specs, values, specs, values), true);
    assert.equal(conditions.evaluateConditions([[{ field: 'count', op: 'gt', value: 10 }, { field: 'count', op: 'lt', value: 20 }]], specs, values, specs, values), true);

    // DNF OR: false group || true group => true
    assert.equal(
      conditions.evaluateConditions(
        [
          [{ field: 'mode', op: 'eq', value: 'a' }],
          [{ field: 'count', op: 'gt', value: 10 }],
        ],
        specs,
        values,
        specs,
        values,
      ),
      true,
    );

    values.note = '   ';
    assert.equal(conditions.evaluateConditions([[{ field: 'note', op: 'empty' }]], specs, values, specs, values), true);
  });

  it('compares datetime via UTC millis (.000Z vs Z) and time via seconds', () => {
    const specs = [
      { type: 'datetime', key: 'pub', label: 'Pub' },
      { type: 'time', key: 'tm', label: 'Time' },
      { type: 'date', key: 'dt', label: 'Date' },
    ];
    const values = {
      pub: '2026-11-26T10:00:00Z',
      tm: '09:30:00',
      dt: '2026-11-26',
    };

    assert.equal(conditions.evaluateConditions([[{ field: 'pub', op: 'gt', value: '2026-11-26T10:00:00.000Z' }]], specs, values, specs, values), false);
    assert.equal(conditions.evaluateConditions([[{ field: 'pub', op: 'gt', value: '2026-11-26T09:59:59.500Z' }]], specs, values, specs, values), true);
    assert.equal(conditions.evaluateConditions([[{ field: 'tm', op: 'lt', value: '09:30' }]], specs, values, specs, values), false);
    assert.equal(conditions.evaluateConditions([[{ field: 'tm', op: 'lt', value: '09:30:01' }]], specs, values, specs, values), true);
    assert.equal(conditions.evaluateConditions([[{ field: 'dt', op: 'gt', value: '2026-11-25' }]], specs, values, specs, values), true);
  });

  it('handles cascade A -> B -> C preserved values and sibling vs root scope fallback', () => {
    const rootSpecs = [
      { type: 'boolean', key: 'show_b', label: 'Show B', default: false },
      { type: 'text', key: 'b', label: 'B', conditions: [[{ field: 'show_b', op: 'eq', value: true }]] },
      { type: 'text', key: 'c', label: 'C', conditions: [[{ field: 'b', op: 'contains', value: 'keep' }]] },
    ];
    const rootValues = { show_b: false, b: 'keep_saved', c: '' };

    // B is hidden, yet C reading B still sees the preserved value
    assert.equal(conditions.evaluateConditions(rootSpecs[1].conditions, rootSpecs, rootValues, rootSpecs, rootValues), false);
    assert.equal(conditions.evaluateConditions(rootSpecs[2].conditions, rootSpecs, rootValues, rootSpecs, rootValues), true);

    // Nested child prefers same-key sibling over same-key root
    const childSpecs = [{ type: 'text', key: 'b', label: 'Child B' }];
    const childValues = { b: 'sibling_only' };
    assert.equal(
      conditions.evaluateConditions([[{ field: 'b', op: 'eq', value: 'sibling_only' }]], childSpecs, childValues, rootSpecs, rootValues),
      true,
    );
    // Child falls back to root show_b
    assert.equal(
      conditions.evaluateConditions([[{ field: 'show_b', op: 'eq', value: false }]], childSpecs, childValues, rootSpecs, rootValues),
      true,
    );
  });
});

describe('locationRulesEvaluator.ts DNF parity with Rust', () => {
  it('evaluates Content, Term, OptionsPage, EntryTaxonomy, SpecialPage, EntryId, and UserRole in OR-of-ANDs', () => {
    const contentRules = [
      [
        { param: 'content_type', op: 'eq', value: 'page' },
        { param: 'page_layout', op: 'eq', value: 'landing' },
      ],
      [
        { param: 'content_type', op: 'eq', value: 'article' },
        { param: 'entry_taxonomy', op: 'eq', value: 'category:12' },
        { param: 'user_role', op: 'eq', value: 'admin' },
      ],
    ];
    assert.equal(locEval.evaluateLocationRules(contentRules, {
      content_type: 'page',
      page_layout: 'landing',
      user_role: 'editor',
    }), true);
    assert.equal(locEval.evaluateLocationRules(contentRules, {
      content_type: 'page',
      page_layout: 'default',
      user_role: 'admin',
    }), false);
    assert.equal(locEval.evaluateLocationRules(contentRules, {
      content_type: 'article',
      entry_term_pairs: [['category', 12]],
      user_role: 'admin',
    }), true);
    assert.equal(locEval.evaluateLocationRules(contentRules, {
      content_type: 'article',
      entry_term_pairs: [['category', 12]],
      user_role: 'editor',
    }), false);
    assert.equal(locEval.evaluateLocationRules(
      [[{ param: 'special_page', op: 'eq', value: 'homepage' }, { param: 'entry_id', op: 'neq', value: '99' }]],
      { content_type: 'page', special_page: 'homepage', entry_id: 5, user_role: 'editor' },
    ), true);
    assert.equal(locEval.evaluateLocationRules(
      [[{ param: 'taxonomy_term', op: 'eq', value: 'all' }]],
      { taxonomy_term: 'genre', user_role: 'editor' },
    ), true);
    assert.equal(locEval.evaluateLocationRules(
      [[{ param: 'options_page', op: 'eq', value: 'custom:company_info' }]],
      { options_page: 'custom:company_info', user_role: 'editor' },
    ), true);
  });

  it('infers domains, detects admin-only restriction, and serializes options safely', () => {
    assert.equal(builderHelpers.inferDomainFromLocation([[{ param: 'taxonomy_term', op: 'eq', value: 'category' }]]), 'term');
    assert.equal(builderHelpers.inferDomainFromLocation([[{ param: 'options_page', op: 'eq', value: 'site_global' }]]), 'options_page');
    assert.equal(builderHelpers.isLocationRestrictedToAdminOnly([
      [{ param: 'content_type', op: 'eq', value: 'post' }, { param: 'user_role', op: 'eq', value: 'admin' }],
    ]), true);
    assert.equal(builderHelpers.isLocationRestrictedToAdminOnly([
      [{ param: 'content_type', op: 'eq', value: 'post' }, { param: 'user_role', op: 'eq', value: 'admin' }],
      [{ param: 'content_type', op: 'eq', value: 'page' }],
    ]), false);
    const parsed = builderHelpers.parseSelectOptions('red|Red\nblue|Blue|star');
    assert.deepEqual(parsed, [{ value: 'red', label: 'Red' }, { value: 'blue', label: 'Blue', icon: 'star' }]);
    assert.equal(builderHelpers.serializeSelectOptions(parsed), 'red|Red\nblue|Blue|star');
  });
});
