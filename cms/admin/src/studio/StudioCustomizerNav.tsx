import React from 'react';
import type { AdminLocaleState } from '../api/locales.ts';
import type { MenuRecord, MenuSlotsSnapshot } from '../api/menus.ts';
import type { PatternRecord } from '../api/patterns.ts';
import type {
  GeneralSettingsDraft,
  GlobalSiteOptionsSnapshot,
  ReadingSettings,
  ReadingSettingsDraft,
} from '../api/settings.ts';
import type { StudioTargetGroup, StudioTargetItem } from '../api/studio.ts';
import type { ThemeOptionDef } from '../api/themes.ts';
import { BlockSchemaForm } from '../builder/BlockSchemaForm.tsx';
import { BuilderEditorCore, type StudioBuilderView } from '../builder/BuilderWorkspace.tsx';
import type { StudioMenuRequest } from './StudioBlockTree.tsx';
import type { BuilderDocument, BuilderViewport, StudioBuilderClipboard } from '../builder/document.ts';
import { AdminAlert, AdminBadge, AdminButton } from '../components/AdminPrimitives.tsx';
import { ThemeOptionsForm } from '../features/themes/ThemeOptionsForm.tsx';
import type { StudioPanelId } from '../shell/router.ts';
import type { BlockRegistry, BlockSchemaV1 } from '../types.ts';
import { StudioMenuManager } from './StudioMenuManager.tsx';

interface Props {
  panel: StudioPanelId;
  onPanelChange: (panel: StudioPanelId) => void;
  localeState?: AdminLocaleState;
  previewLocale?: string;
  activeThemeId: string | null;
  generalDraft: GeneralSettingsDraft | null;
  generalDirty: boolean;
  timezones: string[];
  onGeneralChange: (draft: GeneralSettingsDraft) => void;
  csrfToken: string | null;
  globalOptionsSnapshot: GlobalSiteOptionsSnapshot | null;
  globalOptionsDraft: Record<string, unknown> | null;
  globalOptionsDirty: boolean;
  onGlobalOptionsChange: (values: Record<string, unknown>) => void;
  themeOptionsDraft: Record<string, unknown> | null;
  themeOptionsDirty: boolean;
  themeOptionsReloadToken: number;
  onThemeOptionsChange: (
    values: Record<string, unknown>,
    meta: { version: number; dirty: boolean; schema: ThemeOptionDef[] },
  ) => void;
  menuSlots: MenuSlotsSnapshot | null;
  menuSlotsDraft: Record<string, number | null> | null;
  menuSlotsDirty: boolean;
  menusList: MenuRecord[];
  onMenuSlotChange: (slot: string, menuId: number | null) => void;
  onMenusMutated: () => Promise<void> | void;
  readingDraft: ReadingSettingsDraft | null;
  readingDirty: boolean;
  readingSettings: ReadingSettings | null;
  publishedPages: Array<{ id: number; title: string }>;
  onReadingChange: (draft: ReadingSettingsDraft) => void;
  targets: StudioTargetItem[];
  selectedTargetKey: string | null;
  onSelectTarget: (targetKey: string | null) => void;
  templateEditingEnabled: boolean;
  onInitializeTemplateOverride: () => void;
  templateDoc: BuilderDocument;
  templateSelectedNodeId: string | null;
  onSelectTemplateNode: (id: string | null) => void;
  onChangeTemplateDoc: (next: BuilderDocument, nextSelected?: string | null, removedId?: string) => boolean;
  templateDirty: boolean;
  registry: BlockRegistry;
  viewport: BuilderViewport;
  themeRevision: number;
  patterns: PatternRecord[];
  patternsLoaded: boolean;
  onRefreshPatterns: () => void;
  availablePartKeys: string[];
  onBuilderMessage: (msg: string, failed?: boolean) => void;
  onApiError: (error: unknown, fallback: string) => void;
  disabled?: boolean;
  builderView: StudioBuilderView;
  onBuilderViewChange: (view: StudioBuilderView) => void;
  addTargetParentId: string | null;
  onAddTargetParentIdChange: (id: string | null) => void;
  clipboard: StudioBuilderClipboard | null;
  onClipboardChange: (value: StudioBuilderClipboard | null) => void;
  menuRequest?: StudioMenuRequest | null;
  onMenuRequestHandled?: () => void;
  canvasPickSeq?: number;
  openPatternSeq?: number;
}

const INPUT_CLASS =
  'h-9 w-full rounded-md border border-zinc-300 bg-white px-2.5 text-sm text-zinc-900 ' +
  'focus-visible:outline-2 focus-visible:outline-od-accent';

const GROUP_LABELS: Record<StudioTargetGroup, string> = {
  parts: 'Thành phần dùng chung (Parts)',
  core: 'Template Cốt lõi',
  content_types: 'Loại nội dung (CPT)',
  page_layouts: 'Bố cục Trang (Page Layouts)',
  custom_templates: 'Mẫu chi tiết tùy chỉnh',
  overrides: 'Override khác trong CSDL',
};

const GROUP_ORDER: StudioTargetGroup[] = [
  'parts',
  'core',
  'content_types',
  'page_layouts',
  'custom_templates',
  'overrides',
];

function pageChoiceValue(id: string): number | null {
  if (!id) return null;
  const num = Number(id);
  return Number.isSafeInteger(num) && num > 0 ? num : null;
}

function sourceBadge(item: StudioTargetItem): { label: string; tone: 'success' | 'neutral' | 'warning' } {
  if (item.effective_source.kind === 'db_override') {
    return { label: 'Trình dựng trang Override', tone: 'success' };
  }
  if (item.effective_source.kind === 'active_theme') {
    return { label: 'Theme C-SEC', tone: 'neutral' };
  }
  return { label: 'Mặc định Core', tone: 'warning' };
}

export const StudioCustomizerNav: React.FC<Props> = ({
  panel,
  onPanelChange,
  localeState,
  previewLocale,
  activeThemeId,
  generalDraft,
  generalDirty,
  timezones,
  onGeneralChange,
  csrfToken,
  globalOptionsSnapshot,
  globalOptionsDraft,
  globalOptionsDirty,
  onGlobalOptionsChange,
  themeOptionsDraft,
  themeOptionsDirty,
  themeOptionsReloadToken,
  onThemeOptionsChange,
  menuSlots,
  menuSlotsDraft,
  menuSlotsDirty,
  menusList,
  onMenuSlotChange,
  onMenusMutated,
  readingDraft,
  readingDirty,
  readingSettings,
  publishedPages,
  onReadingChange,
  targets,
  selectedTargetKey,
  onSelectTarget,
  templateEditingEnabled,
  onInitializeTemplateOverride,
  templateDoc,
  templateSelectedNodeId,
  onSelectTemplateNode,
  onChangeTemplateDoc,
  templateDirty,
  registry,
  viewport,
  themeRevision,
  patterns,
  patternsLoaded,
  onRefreshPatterns,
  availablePartKeys,
  onBuilderMessage,
  onApiError,
  disabled = false,
  builderView,
  onBuilderViewChange,
  addTargetParentId,
  onAddTargetParentIdChange,
  clipboard,
  onClipboardChange,
  menuRequest,
  onMenuRequestHandled,
  canvasPickSeq,
  openPatternSeq,
}) => {
  const selectedTarget = selectedTargetKey
    ? targets.find(item => item.canonical_target_key === selectedTargetKey) ?? null
    : null;

  if (panel === 'root') {
    const customizerItems: Array<{ id: StudioPanelId; label: string; desc: string; dirty: boolean }> = [
      { id: 'identity', label: 'Nhận diện Site', desc: 'Tiêu đề website, khẩu hiệu & múi giờ', dirty: generalDirty },
      { id: 'theme-options', label: 'Tùy chọn Giao diện', desc: 'Mạng xã hội, thương hiệu & tùy chọn theme', dirty: globalOptionsDirty || themeOptionsDirty },
      { id: 'menus', label: 'Menu & Vị trí', desc: 'Tạo, sửa cây menu & gắn vị trí hiển thị', dirty: menuSlotsDirty },
      { id: 'reading', label: 'Trang chủ & Đọc', desc: 'Chọn trang chủ tĩnh & trang tin tức', dirty: readingDirty },
    ];

    return (
      <nav className="flex h-full flex-col overflow-y-auto" aria-label="Điều hướng Tùy chỉnh giao diện" data-testid="studio-nav-root">
        <div className="space-y-5 p-3">
          <div>
            <p className="mb-1.5 px-2 text-[11px] font-semibold uppercase tracking-wider text-zinc-500">
              Tùy biến Website
            </p>
            <ul className="m-0 list-none space-y-1 p-0">
              {customizerItems.map(item => (
                <li key={item.id}>
                  <button
                    type="button"
                    data-testid={`studio-nav-${item.id}`}
                    onClick={() => onPanelChange(item.id)}
                    className="flex w-full items-center justify-between gap-2 rounded-lg border border-transparent px-3 py-2.5 text-left transition-colors hover:border-zinc-200 hover:bg-zinc-50"
                  >
                    <span className="min-w-0">
                      <span className="flex items-center gap-2 text-sm font-medium text-zinc-900">
                        <span>{item.label}</span>
                        {item.dirty && <span className="h-2 w-2 rounded-full bg-amber-500" title="Có thay đổi chưa lưu" />}
                      </span>
                      <span className="mt-0.5 block truncate text-xs text-zinc-500">{item.desc}</span>
                    </span>
                    <span className="text-zinc-400" aria-hidden="true">›</span>
                  </button>
                </li>
              ))}
            </ul>
          </div>

          <div>
            <p className="mb-1.5 px-2 text-[11px] font-semibold uppercase tracking-wider text-zinc-500">
              Dựng Template
            </p>
            <button
              type="button"
              data-testid="studio-nav-templates"
              onClick={() => onPanelChange('templates')}
              className="flex w-full items-center justify-between gap-2 rounded-lg border border-zinc-200 bg-white px-3 py-2.5 text-left shadow-2xs transition-colors hover:border-zinc-300 hover:bg-zinc-50"
            >
              <span className="min-w-0">
                <span className="flex items-center gap-2 text-sm font-medium text-zinc-900">
                  <span>Template &amp; Trình dựng trang</span>
                  {templateDirty && <span className="h-2 w-2 rounded-full bg-amber-500" title="Template có thay đổi chưa lưu" />}
                </span>
                <span className="mt-0.5 block truncate text-xs text-zinc-500">
                  Header, Footer, Page Layout, Single &amp; Archive
                </span>
              </span>
              <span className="text-zinc-400" aria-hidden="true">›</span>
            </button>
          </div>
        </div>
      </nav>
    );
  }

  if (panel === 'identity') {
    const legacyTz = Boolean(generalDraft && timezones.length > 0 && !timezones.includes(generalDraft.timezone));
    return (
      <div className="flex h-full flex-col overflow-y-auto" data-testid="studio-panel-identity">
        <div className="space-y-4 p-4">
          {!generalDraft ? (
            <p className="text-xs text-zinc-500">Đang tải cài đặt website…</p>
          ) : (
            <>
              <label className="block space-y-1 text-xs font-medium text-zinc-700" htmlFor="studio-site-title">
                <span>Tiêu đề website (site_title)</span>
                <input
                  id="studio-site-title"
                  type="text"
                  maxLength={120}
                  disabled={disabled}
                  className={INPUT_CLASS}
                  value={generalDraft.site_title}
                  onChange={e => onGeneralChange({ ...generalDraft, site_title: e.target.value })}
                />
              </label>

              <label className="block space-y-1 text-xs font-medium text-zinc-700" htmlFor="studio-site-tagline">
                <span>Khẩu hiệu / Mô tả ngắn (tagline)</span>
                <textarea
                  id="studio-site-tagline"
                  rows={3}
                  maxLength={300}
                  disabled={disabled}
                  className="w-full rounded-md border border-zinc-300 bg-white px-2.5 py-2 text-sm text-zinc-900 focus-visible:outline-2 focus-visible:outline-od-accent"
                  value={generalDraft.tagline}
                  onChange={e => onGeneralChange({ ...generalDraft, tagline: e.target.value })}
                />
              </label>

              <label className="block space-y-1 text-xs font-medium text-zinc-700" htmlFor="studio-site-timezone">
                <span>Múi giờ hiển thị</span>
                <select
                  id="studio-site-timezone"
                  disabled={disabled}
                  className={INPUT_CLASS}
                  value={generalDraft.timezone}
                  onChange={e => onGeneralChange({ ...generalDraft, timezone: e.target.value })}
                >
                  {legacyTz && (
                    <option value={generalDraft.timezone}>{generalDraft.timezone} (hiện tại)</option>
                  )}
                  {(timezones.length > 0 ? timezones : [generalDraft.timezone]).map(tz => (
                    <option key={tz} value={tz}>{tz}</option>
                  ))}
                </select>
              </label>
              <p className="text-xs text-zinc-500">
                Thay đổi được xem trước trực tiếp bên phải và chỉ lưu lên website khi bấm <strong>Lưu thay đổi</strong>.
              </p>
            </>
          )}
        </div>
      </div>
    );
  }

  if (panel === 'theme-options') {
    const globalSchema: BlockSchemaV1 | null = globalOptionsSnapshot
      ? {
          schema_version: 1,
          id: 'site/global-options',
          title: 'Tùy chọn toàn trang',
          category: 'content',
          render: 'render.html',
          fields: globalOptionsSnapshot.fields,
        }
      : null;

    return (
      <div className="flex h-full flex-col overflow-y-auto" data-testid="studio-panel-theme-options">
        <div className="space-y-5 p-4">
          <section className="space-y-3" data-testid="global-site-options-section">
            <div className="flex items-center justify-between gap-2">
              <div>
                <h2 className="text-xs font-semibold uppercase tracking-wide text-zinc-800">
                  Tùy chọn toàn trang
                </h2>
                <p className="mt-0.5 text-[11px] text-zinc-500">
                  Dùng chung cho nhiều block &amp; giữ nguyên khi đổi Theme.
                </p>
              </div>
              {globalOptionsSnapshot && (
                <AdminBadge tone="neutral">v{globalOptionsSnapshot.version}</AdminBadge>
              )}
            </div>
            {globalSchema ? (
              <BlockSchemaForm
                schema={globalSchema}
                values={globalOptionsDraft ?? globalOptionsSnapshot?.values ?? {}}
                onChange={onGlobalOptionsChange}
                viewport={viewport}
                csrfToken={csrfToken}
                onError={onApiError}
                disabled={disabled}
              />
            ) : (
              <p className="text-xs text-zinc-500">Đang tải tùy chọn toàn trang…</p>
            )}
          </section>

          <section className="space-y-3 border-t border-zinc-200 pt-4" data-testid="theme-specific-options-section">
            <h2 className="text-xs font-semibold uppercase tracking-wide text-zinc-800">
              Tùy chọn riêng của Theme
            </h2>
            {!activeThemeId ? (
              <AdminAlert tone="info">
                Chưa có Theme v3 đang kích hoạt để cấu hình tùy chọn riêng.
              </AdminAlert>
            ) : (
              <ThemeOptionsForm
                themeId={activeThemeId}
                csrfToken={csrfToken}
                viewport={viewport}
                locale={previewLocale}
                localeState={localeState}
                onApiError={onApiError}
                hideSubmitButton
                externalValues={themeOptionsDraft}
                reloadToken={themeOptionsReloadToken}
                onValuesChange={onThemeOptionsChange}
              />
            )}
          </section>
        </div>
      </div>
    );
  }

  if (panel === 'menus') {
    return (
      <StudioMenuManager
        activeThemeId={activeThemeId}
        menuSlots={menuSlots}
        menuSlotsDraft={menuSlotsDraft}
        menusList={menusList}
        onMenuSlotChange={onMenuSlotChange}
        csrfToken={csrfToken}
        disabled={disabled}
        onMenusMutated={onMenusMutated}
        onApiError={onApiError}
        onMessage={onBuilderMessage}
      />
    );
  }

  if (panel === 'reading') {
    const samePageSelected = Boolean(
      readingDraft && readingDraft.homepage_page_id !== null &&
      readingDraft.homepage_page_id === readingDraft.posts_page_id,
    );
    return (
      <div className="flex h-full flex-col overflow-y-auto" data-testid="studio-panel-reading">
        <div className="space-y-4 p-4">
          {!readingDraft ? (
            <p className="text-xs text-zinc-500">Đang tải cấu hình Reading…</p>
          ) : (
            <>
              <label className="block space-y-1 text-xs font-medium text-zinc-700" htmlFor="studio-reading-homepage">
                <span>Trang chủ (Static Homepage)</span>
                <select
                  id="studio-reading-homepage"
                  disabled={disabled}
                  className={INPUT_CLASS}
                  value={readingDraft.homepage_page_id ?? ''}
                  onChange={e => onReadingChange({ ...readingDraft, homepage_page_id: pageChoiceValue(e.target.value) })}
                >
                  <option value="">— Trang chủ mặc định (Latest Posts) —</option>
                  {readingDraft.homepage_page_id !== null &&
                    !publishedPages.some(p => p.id === readingDraft.homepage_page_id) && (
                      <option value={readingDraft.homepage_page_id}>
                        Page #{readingDraft.homepage_page_id} ({readingSettings?.homepage_status ?? 'hiện tại'})
                      </option>
                    )}
                  {publishedPages.map(page => (
                    <option key={page.id} value={page.id}>{page.title} (#{page.id})</option>
                  ))}
                </select>
              </label>

              <label className="block space-y-1 text-xs font-medium text-zinc-700" htmlFor="studio-reading-posts-page">
                <span>Trang danh sách bài viết (Posts Page)</span>
                <select
                  id="studio-reading-posts-page"
                  disabled={disabled}
                  className={INPUT_CLASS}
                  value={readingDraft.posts_page_id ?? ''}
                  onChange={e => onReadingChange({ ...readingDraft, posts_page_id: pageChoiceValue(e.target.value) })}
                >
                  <option value="">— Chưa chọn trang bài viết —</option>
                  {readingDraft.posts_page_id !== null &&
                    !publishedPages.some(p => p.id === readingDraft.posts_page_id) && (
                      <option value={readingDraft.posts_page_id}>
                        Page #{readingDraft.posts_page_id} ({readingSettings?.posts_page_status ?? 'hiện tại'})
                      </option>
                    )}
                  {publishedPages.map(page => (
                    <option key={page.id} value={page.id}>{page.title} (#{page.id})</option>
                  ))}
                </select>
              </label>

              {samePageSelected && (
                <AdminAlert tone="warning">
                  Trang chủ và Trang bài viết không thể chọn cùng một Page. Vui lòng chọn 2 trang khác nhau trước khi lưu.
                </AdminAlert>
              )}

              <p className="text-xs text-zinc-500">
                Cấu hình Trang chủ &amp; Đọc ảnh hưởng định tuyến trang chủ; bấm <strong>Lưu thay đổi</strong> để áp dụng.
              </p>
            </>
          )}
        </div>
      </div>
    );
  }

  // panel === 'templates'
  if (selectedTarget) {
    const badge = sourceBadge(selectedTarget);
    const showCreateWarning = !selectedTarget.override_id && !templateEditingEnabled;
    const allowRouteContext = ['single', 'page', 'page_layout', 'single_custom'].includes(selectedTarget.target.kind);
    return (
      <div className="flex h-full flex-col overflow-y-auto" data-testid="studio-panel-template-editor">
        <div className="space-y-2.5 px-1.5 py-2">
          <div className="flex flex-wrap items-center justify-between gap-2 rounded-lg border border-zinc-200 bg-zinc-50 px-3 py-2 text-xs">
            <code className="font-mono text-zinc-800">{selectedTarget.canonical_target_key}</code>
            <AdminBadge tone={badge.tone}>{badge.label}</AdminBadge>
          </div>

          {showCreateWarning ? (
            <div className="space-y-3 rounded-xl border border-amber-200 bg-amber-50 p-3.5 text-xs text-amber-950" data-testid="studio-template-override-warning">
              <p className="font-semibold">
                {selectedTarget.has_theme_source
                  ? 'Target đang dùng mẫu gốc (C-SEC HTML) từ Theme đang kích hoạt.'
                  : 'Target hiện chưa có Trình dựng trang Override trong CSDL (đang dùng fallback mặc định).'}
              </p>
              <p className="leading-relaxed">
                Khởi tạo bản dựng Trình dựng trang sẽ tạo một template override mới cho <code>{selectedTarget.canonical_target_key}</code>.
                Sau khi bấm <strong>Lưu thay đổi</strong>, bản dựng này sẽ thay thế mẫu gốc của Theme (có thể hoàn tác bằng <em>Reset mặc định</em>).
              </p>
              <AdminButton
                variant="primary"
                size="sm"
                className="w-full"
                disabled={disabled}
                onClick={onInitializeTemplateOverride}
                data-testid="studio-init-template-override"
              >
                Khởi tạo bản dựng Trình dựng trang
              </AdminButton>
            </div>
          ) : (
            <BuilderEditorCore
              key={selectedTarget.canonical_target_key}
              builderView={builderView}
              onBuilderViewChange={onBuilderViewChange}
              addTargetParentId={addTargetParentId}
              onAddTargetParentIdChange={onAddTargetParentIdChange}
              clipboard={clipboard}
              onClipboardChange={onClipboardChange}
              menuRequest={menuRequest}
              onMenuRequestHandled={onMenuRequestHandled}
              canvasPickSeq={canvasPickSeq}
              openPatternSeq={openPatternSeq}
              document={templateDoc}
              selectedId={templateSelectedNodeId}
              onSelect={onSelectTemplateNode}
              onChangeDocument={onChangeTemplateDoc}
              registry={registry}
              csrfToken={csrfToken}
              viewport={viewport}
              themeRevision={themeRevision}
              patterns={patterns}
              patternsLoaded={patternsLoaded}
              onRefreshPatterns={onRefreshPatterns}
              allowRouteContext={allowRouteContext}
              allowPatternRef={false}
              allowList={true}
              allowContentSlot={!selectedTarget.forbids_content_slot}
              allowTemplatePart={true}
              requiresContentSlot={selectedTarget.requires_content_slot}
              availablePartKeys={availablePartKeys}
              disabled={disabled}
              onMessage={onBuilderMessage}
              onApiError={onApiError}
            />
          )}
        </div>
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col overflow-y-auto" data-testid="studio-panel-templates">
      <div className="space-y-4 p-3">
        {targets.length === 0 ? (
          <p className="px-1 text-xs text-zinc-500">Đang tải danh sách template…</p>
        ) : (
          GROUP_ORDER.map(group => {
            const groupItems = targets.filter(item => item.group === group);
            if (groupItems.length === 0) return null;
            return (
              <div key={group} className="space-y-1">
                <p className="px-2 text-[11px] font-semibold uppercase tracking-wider text-zinc-500">
                  {GROUP_LABELS[group]}
                </p>
                <ul className="m-0 list-none space-y-1 p-0">
                  {groupItems.map(item => {
                    const badge = sourceBadge(item);
                    return (
                      <li key={item.canonical_target_key}>
                        <button
                          type="button"
                          data-testid={`studio-target-${item.canonical_target_key}`}
                          onClick={() => onSelectTarget(item.canonical_target_key)}
                          className="flex w-full items-center justify-between gap-2 rounded-lg border border-zinc-200 bg-white px-2.5 py-2 text-left transition-colors hover:border-zinc-300 hover:bg-zinc-50"
                        >
                          <span className="min-w-0">
                            <span className="block truncate text-xs font-semibold text-zinc-900">{item.label}</span>
                            <code className="mt-0.5 block truncate font-mono text-[10px] text-zinc-500">
                              {item.canonical_target_key}
                            </code>
                          </span>
                          <AdminBadge tone={badge.tone}>{badge.label}</AdminBadge>
                        </button>
                      </li>
                    );
                  })}
                </ul>
              </div>
            );
          })
        )}
      </div>
    </div>
  );
};
