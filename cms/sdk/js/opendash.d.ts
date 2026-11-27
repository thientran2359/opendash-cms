/**
 * OpenDash CMS Unified JS Runtime Type Definitions (`functions.js` & `plugin.js`).
 *
 * Executed inside the isolated `--plugin-worker` powered by `boa_engine`.
 * Top-level evaluation is strictly for handler registration (`od.on`, `od.addFilter`,
 * `od.addAction`, `od.addRoute`). All Host Ops (`od.kv.*`, `od.content.*`, etc.)
 * MUST be invoked inside registered handler callbacks.
 */

export type ExecutionPrincipal = "admin" | "editor" | "public" | "system";
export type EnvelopeKind = "hook" | "filter" | "action" | "manual" | "route";

export interface OdEnvelope<TPayload = Record<string, unknown>> {
  envelope_version: 2;
  kind: EnvelopeKind;
  event: string;
  occurred_at_ms: number;
  correlation_id: string;
  locale?: string | null;
  principal: ExecutionPrincipal;
  dispatch_depth: number;
  payload: TPayload;
}

export type HookEventName =
  | "page/after_save"
  | "content/before_save"
  | "content/after_save"
  | "content/after_delete"
  | "content/after_publish"
  | "content/after_unpublish"
  | "contact/after_submit"
  | "plugin/manual_run"
  | "plugin/after_enable"
  | "plugin/after_disable"
  | "settings/after_save"
  | "theme/after_activate";

export type FilterEventName =
  | "filter:content/prepare_save"
  | "filter:contact/before_submit"
  | "filter:seo/head"
  | "filter:entry/view"
  | "filter:render/head"
  | "filter:render/footer"
  | "filter:render/slot";

export interface OdEntryRecord {
  id: number;
  type_key: string;
  slug: string;
  title: string;
  status: string;
  version: number;
  locale: string;
  values: Record<string, unknown>;
  term_ids: number[];
}

export interface OdContentListParams {
  type_key: string;
  locale?: string;
  limit?: number;
  offset?: number;
  published_only?: boolean;
}

export interface OdContentSaveParams {
  type_key: string;
  entry_id?: number;
  expected_version?: number;
  locale?: string;
  slug?: string;
  title?: string;
  values?: Record<string, unknown>;
  term_ids?: number[];
}

export interface OdContentDeleteParams {
  type_key: string;
  entry_id: number;
  expected_version: number;
}

export interface OdTaxonomyTerm {
  id: number;
  kind: string;
  slug: string;
  name: string;
  parent_id: number | null;
}

export interface OdMediaAsset {
  id: string;
  filename: string;
  mime_type: string;
  url: string;
  width: number | null;
  height: number | null;
  alt: string | null;
}

export interface OdHydratedMedia {
  id: string;
  url: string;
  thumbnail_url: string;
  alt: string;
  width: number | null;
  height: number | null;
  kind: string;
}

export interface OdHydratedLink {
  url: string;
  label: string;
  target: "" | "_blank";
  rel: string;
  new_tab: boolean;
  nofollow: boolean;
}

export interface OdHydratedIcon {
  name: string;
}

export interface OdHydratedSpacing {
  top: number | null;
  right: number | null;
  bottom: number | null;
  left: number | null;
  css: string;
  classes: string;
}

export interface OdHydratedEntryRef {
  id: number;
  type_key: string;
  title: string;
  slug: string;
  url: string;
  cover_url: string | null;
  excerpt: string;
}

export interface OdHydratedTermRef {
  id: number;
  kind: string;
  name: string;
  slug: string;
  url: string;
  featured_image: OdHydratedMedia | null;
}

export type OdHydratedOptionValue =
  | string
  | number
  | boolean
  | null
  | string[]
  | OdHydratedMedia
  | OdHydratedMedia[]
  | OdHydratedLink
  | OdHydratedIcon
  | OdHydratedSpacing
  | OdHydratedEntryRef
  | OdHydratedEntryRef[]
  | OdHydratedTermRef
  | OdHydratedTermRef[]
  | { [key: string]: OdHydratedOptionValue }
  | Array<{ [key: string]: OdHydratedOptionValue }>;

export interface OdHttpRequestParams {
  method: "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD";
  url: string;
  headers?: Record<string, string>;
  body?: string;
  timeout_ms?: number;
}

export interface OdHttpResponse {
  status: number;
  headers: Record<string, string>;
  body: string;
}

export interface OdRenderPayload {
  slot?: string;
  args?: unknown;
  route_kind: string;
  route_path?: string;
  locale: string;
  site: Record<string, unknown>;
  options: Record<string, unknown>;
  entry?: Record<string, unknown>;
  entries?: Record<string, unknown>[];
}

export type OdRenderResult =
  | null
  | undefined
  | string
  | {
      html?: string;
      styles?: string[];
      head_scripts?: string[];
    };

export interface OdRoutePayload {
  route_id: string;
  method: "GET" | "POST";
  path: string;
  subpath: string;
  params: Record<string, string>;
  query: Record<string, string>;
  body?: unknown;
  raw_body?: string;
  signature_headers?: Record<string, string>;
}

export interface OdRouteResponse {
  status?: number;
  content_type?: "text/html" | "application/json" | "text/plain";
  body?: string;
  json?: unknown;
  title?: string;
}

export interface OpenDashHostApi {
  on<TPayload = Record<string, unknown>>(
    event: HookEventName | (string & {}),
    handler: (env: OdEnvelope<TPayload>) => unknown
  ): void;

  addFilter<TPayload = Record<string, unknown>, TResult = unknown>(
    filterName: FilterEventName | (string & {}),
    handler: (env: OdEnvelope<TPayload>) => TResult
  ): void;

  addAction<TPayload = Record<string, unknown>>(
    actionId: string,
    handler: (env: OdEnvelope<TPayload>) => unknown
  ): void;

  addRoute(
    routeId: string,
    handler: (env: OdEnvelope<OdRoutePayload>) => OdRouteResponse
  ): void;

  readonly kv: {
    get<T = unknown>(key: string): T | null;
    set(key: string, value: unknown): void;
    delete(key: string): { deleted: boolean };
    list(prefix?: string, limit?: number): { items: Array<{ key: string; value: unknown }> };
  };

  readonly content: {
    get(typeKey: string, entryId: number, locale?: string): { entry: OdEntryRecord | null };
    list(params: OdContentListParams): { items: OdEntryRecord[]; total: number };
    save(params: OdContentSaveParams): void;
    delete(params: OdContentDeleteParams): { staged: boolean; entry_id: number };
    trashGet(typeKey: string, entryId: number, locale?: string): { entry: Record<string, unknown> | null };
    trashList(
      params:
        | string
        | { type_key: string; locale?: string; limit?: number; offset?: number },
      opts?: { locale?: string; limit?: number; offset?: number }
    ): { items: Array<Record<string, unknown>>; total: number; limit: number; offset: number };
    restore(params: OdContentDeleteParams): { staged: boolean; operation: "restore"; entry_id: number };
    purge(params: OdContentDeleteParams): { staged: boolean; operation: "purge"; entry_id: number };
  };

  readonly taxonomy: {
    list(kind: string, locale?: string, limit?: number): { items: OdTaxonomyTerm[] };
  };

  readonly media: {
    get(mediaId: string): { media: OdMediaAsset | null };
  };

  readonly site: {
    /**
     * Returns merged public global site options + active theme public options
     * hydrated to Unified Field Schema V1 shapes (`media`/`link`/`icon`/
     * `spacing`/`entry_ref`/`term_ref` objects and plain sanitized `richtext` HTML).
     */
    options(locale?: string): { options: Record<string, OdHydratedOptionValue> };
  };

  readonly settings: {
    get(): { settings: Record<string, unknown> };
  };

  readonly secret: {
    get(key: string): { value: string | null };
  };

  readonly http: {
    request(params: OdHttpRequestParams): OdHttpResponse;
  };

  readonly log: {
    info(message: string): void;
    warn(message: string): void;
    error(message: string): void;
  };

  readonly i18n: {
    __(msgid: string, domain?: string): string;
    _x(msgid: string, context: string, domain?: string): string;
    _n(singular: string, plural: string, count: number, domain?: string): string;
  };

  escapeHtml(text: string): string;
  escapeAttr(text: string): string;
}

declare global {
  const od: OpenDashHostApi;
}
