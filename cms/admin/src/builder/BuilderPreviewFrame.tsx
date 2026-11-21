import { useEffect, useRef, useState } from 'react';
import * as api from '../api/index.ts';
import { type BuilderDocument, type BuilderViewport, VIEWPORT_WIDTHS, locate } from './document.ts';

const MEDIA_PREVIEW_LIMIT = 64;
const MEDIA_PREVIEW_BYTES_LIMIT = 48 * 1024 * 1024;
const PRIVATE_MEDIA_PATH = /^\/api\/admin\/media\/([a-zA-Z0-9_-]{1,128})\/file$/;

const BRIDGE_SCRIPT = [
  '(function(){',
  'var mNonce=document.querySelector(\'meta[name="od-studio-nonce"]\');',
  'var nonce=mNonce?mNonce.getAttribute("content")||"":"";',
  'if(!nonce)return;',
  'function restoreScroll(){',
  'var ms=document.querySelector(\'meta[name="od-studio-scroll"]\');',
  'if(!ms)return;',
  'var parts=(ms.getAttribute("content")||"").split(",");',
  'var sx=parseInt(parts[0]||"0",10);var sy=parseInt(parts[1]||"0",10);',
  'if(isFinite(sx)&&isFinite(sy)&&(sx>0||sy>0))window.scrollTo(sx,sy);',
  '}',
  'function applyAttr(attr,id,doScroll){',
  'var prev=document.querySelectorAll("[data-od-editable=\\"1\\"]["+attr+"]");',
  'for(var i=0;i<prev.length;i++)prev[i].removeAttribute(attr);',
  'if(!id)return;',
  'var nodes=document.querySelectorAll("[data-od-editable=\\"1\\"][data-node-id]");',
  'for(var j=0;j<nodes.length;j++){',
  'if(nodes[j].getAttribute("data-node-id")===id){',
  'nodes[j].setAttribute(attr,"1");',
  'if(doScroll&&nodes[j].scrollIntoView){nodes[j].scrollIntoView({block:"nearest",behavior:"smooth"});}',
  'break;',
  '}',
  '}',
  '}',
  'restoreScroll();',
  'window.addEventListener("load",restoreScroll,{once:true});',
  'var mSel=document.querySelector(\'meta[name="od-studio-selected"]\');',
  'if(mSel){var initId=mSel.getAttribute("content");if(initId)applyAttr("data-od-selected",initId,false);}',
  'document.addEventListener("mouseover",function(e){',
  'var t=e.target&&e.target.closest?e.target.closest("[data-od-editable=\\"1\\"][data-node-id]"):null;',
  'var id=t?t.getAttribute("data-node-id"):null;',
  'applyAttr("data-od-hover",id,false);',
  'window.parent.postMessage({type:"od-studio-hover",nonce:nonce,nodeId:id,kind:t?(t.getAttribute("data-od-kind")||""):null},"*");',
  '},true);',
  'document.addEventListener("mouseout",function(e){',
  'var rel=e.relatedTarget&&e.relatedTarget.closest?e.relatedTarget.closest("[data-od-editable=\\"1\\"][data-node-id]"):null;',
  'if(!rel){',
  'applyAttr("data-od-hover",null,false);',
  'window.parent.postMessage({type:"od-studio-hover",nonce:nonce,nodeId:null,kind:null},"*");',
  '}',
  '},true);',
  'document.addEventListener("click",function(e){',
  'var el=e.target&&e.target.closest?e.target.closest("[data-od-editable=\\"1\\"][data-node-id]"):null;',
  'var ctl=e.target&&e.target.closest?e.target.closest("a,button,input,select,textarea,form"):null;',
  'if(el||ctl){e.preventDefault();e.stopPropagation();}',
  'if(!el)return;',
  'var nodeId=el.getAttribute("data-node-id");',
  'if(!nodeId)return;',
  'applyAttr("data-od-selected",nodeId,false);',
  'window.parent.postMessage({type:"od-studio-pick",nonce:nonce,nodeId:nodeId,kind:el.getAttribute("data-od-kind")||""},"*");',
  '},true);',
  'document.addEventListener("submit",function(e){e.preventDefault();e.stopPropagation();},true);',
  'var st=null;',
  'window.addEventListener("scroll",function(){',
  'if(st)clearTimeout(st);',
  'st=setTimeout(function(){',
  'window.parent.postMessage({type:"od-studio-scroll",nonce:nonce,x:Math.max(0,Math.round(window.scrollX||0)),y:Math.max(0,Math.round(window.scrollY||0))},"*");',
  '},80);',
  '},{passive:true});',
  'window.addEventListener("message",function(e){',
  'if(e.source!==window.parent||!e.data||typeof e.data!=="object"||e.data.nonce!==nonce)return;',
  'if(e.data.type==="od-highlight"){',
  'var sid=typeof e.data.selectedId==="string"?e.data.selectedId:null;',
  'applyAttr("data-od-selected",sid,Boolean(e.data.scroll));',
  '}',
  '});',
  '})();',
].join('\n');

const BRIDGE_SCRIPT_SHA256 = 'UiM/eIdicljiszHJtbLUkiT/TvTl34+g3v9/B6oYKjA=';

function generateStudioNonce(): string {
  const bytes = new Uint8Array(16);
  crypto.getRandomValues(bytes);
  return Array.from(bytes, b => b.toString(16).padStart(2, '0')).join('');
}

function rewriteRootRelativeUrl(raw: string | null, origin: string): string | null {
  if (!raw || !origin) return null;
  if (raw.startsWith('/theme/') || raw.startsWith('/media/')) {
    return `${origin}${raw}`;
  }
  return null;
}

export interface PreparePreviewOptions {
  interactive?: boolean;
  nonce?: string;
  scroll?: { x: number; y: number };
  selectedId?: string | null;
  origin?: string;
}

/**
 * Prepares server-rendered preview HTML for an opaque-origin sandbox iframe:
 * - Resolves authenticated private `/api/admin/media/{id}/file` images as data URLs
 *   (because opaque origins do not transmit SameSite session cookies).
 * - Strips any scripts/CSP tags from parsed HTML as defense-in-depth.
 * - Rewrites `/theme/...` and `/media/...` URLs to explicit `origin` URLs so CSS,
 *   fonts, and media load cleanly in `sandbox="allow-scripts"` (without allow-same-origin).
 * - Prepends a static SHA-256 script-src CSP and injects the nonced canvas bridge.
 */
export async function preparePreviewMarkup(
  html: string,
  signal: AbortSignal,
  options: PreparePreviewOptions = {},
): Promise<{ html: string; skipped: number }> {
  const parsed = new DOMParser().parseFromString(html, 'text/html');

  // Defense-in-depth: strip any script tags and untrusted meta directives first.
  parsed.querySelectorAll('script').forEach(node => node.remove());
  parsed.querySelectorAll('meta').forEach(node => {
    const httpEquiv = (node.getAttribute('http-equiv') ?? '').toLowerCase();
    const name = (node.getAttribute('name') ?? '').toLowerCase();
    if (httpEquiv === 'content-security-policy' || name.startsWith('od-studio-')) {
      node.remove();
    }
  });

  // Resolve private admin media paths into inline raster data URLs.
  const privateTargets = [
    ...Array.from(parsed.querySelectorAll('img[src]')).map(el => ({ el, attr: 'src' })),
    ...Array.from(parsed.querySelectorAll('video[poster]')).map(el => ({ el, attr: 'poster' })),
  ];
  const mediaPaths = [
    ...new Set(
      privateTargets
        .map(t => t.el.getAttribute(t.attr) ?? '')
        .filter(path => PRIVATE_MEDIA_PATH.test(path)),
    ),
  ];
  const replacements = new Map<string, string>();
  let skipped = Math.max(0, mediaPaths.length - MEDIA_PREVIEW_LIMIT);
  let bytes = 0;
  for (const path of mediaPaths.slice(0, MEDIA_PREVIEW_LIMIT)) {
    if (signal.aborted) throw new DOMException('Preview đã hủy.', 'AbortError');
    let blob: Blob;
    try {
      blob = await api.getPrivatePreviewMedia(path, signal);
    } catch (cause) {
      if (cause instanceof api.ApiClientError && cause.status === 401) throw cause;
      if (signal.aborted) throw cause;
      skipped += 1;
      continue;
    }
    if (
      !['image/png', 'image/jpeg', 'image/webp'].includes(blob.type) ||
      !blob.size ||
      bytes + blob.size > MEDIA_PREVIEW_BYTES_LIMIT
    ) {
      skipped += 1;
      continue;
    }
    bytes += blob.size;
    const dataUrl = await new Promise<string>((resolve, reject) => {
      const reader = new FileReader();
      reader.onload = () => resolve(String(reader.result));
      reader.onerror = () => reject(reader.error ?? new Error('Không đọc được ảnh Preview.'));
      reader.onabort = () => reject(new DOMException('Preview đã hủy.', 'AbortError'));
      if (signal.aborted) reader.abort();
      else reader.readAsDataURL(blob);
    });
    replacements.set(path, dataUrl);
  }
  privateTargets.forEach(({ el, attr }) => {
    const replacement = replacements.get(el.getAttribute(attr) ?? '');
    if (replacement) el.setAttribute(attr, replacement);
  });

  // Rewrite root-relative `/theme/...` & `/media/...` URLs to `${origin}/...`.
  const resolvedOrigin = options.origin ?? (typeof window !== 'undefined' ? window.location.origin : '');
  if (resolvedOrigin) {
    const urlAttributes: Array<[string, string]> = [
      ['link[href]', 'href'],
      ['img[src]', 'src'],
      ['video[src]', 'src'],
      ['video[poster]', 'poster'],
      ['source[src]', 'src'],
    ];
    for (const [selector, attr] of urlAttributes) {
      parsed.querySelectorAll(selector).forEach(el => {
        const rewritten = rewriteRootRelativeUrl(el.getAttribute(attr), resolvedOrigin);
        if (rewritten) el.setAttribute(attr, rewritten);
      });
    }
  }

  let head = parsed.querySelector('head');
  if (!head) {
    head = parsed.createElement('head');
    parsed.documentElement.insertBefore(head, parsed.body);
  }
  let body = parsed.querySelector('body');
  if (!body) {
    body = parsed.createElement('body');
    parsed.documentElement.appendChild(body);
  }

  const originSource = resolvedOrigin ? ` ${resolvedOrigin}` : '';
  const interactive = Boolean(options.interactive);
  const scriptSrcDirective = interactive ? `'sha256-${BRIDGE_SCRIPT_SHA256}'` : `'none'`;
  const cspContent =
    `default-src 'none'; ` +
    `style-src 'unsafe-inline'${originSource}; ` +
    `img-src data: blob:${originSource}; ` +
    `media-src data: blob:${originSource}; ` +
    `font-src data:${originSource}; ` +
    `script-src ${scriptSrcDirective}; ` +
    `object-src 'none'; ` +
    `base-uri 'none'; ` +
    `form-action 'none';`;

  const cspMeta = parsed.createElement('meta');
  cspMeta.setAttribute('http-equiv', 'Content-Security-Policy');
  cspMeta.setAttribute('content', cspContent);
  head.insertBefore(cspMeta, head.firstChild);

  if (interactive) {
    const nonce = options.nonce || generateStudioNonce();
    const scrollX = Math.max(0, Math.round(options.scroll?.x ?? 0));
    const scrollY = Math.max(0, Math.round(options.scroll?.y ?? 0));

    const nonceMeta = parsed.createElement('meta');
    nonceMeta.setAttribute('name', 'od-studio-nonce');
    nonceMeta.setAttribute('content', nonce);
    head.appendChild(nonceMeta);

    const scrollMeta = parsed.createElement('meta');
    scrollMeta.setAttribute('name', 'od-studio-scroll');
    scrollMeta.setAttribute('content', `${scrollX},${scrollY}`);
    head.appendChild(scrollMeta);

    if (options.selectedId) {
      const selMeta = parsed.createElement('meta');
      selMeta.setAttribute('name', 'od-studio-selected');
      selMeta.setAttribute('content', options.selectedId);
      head.appendChild(selMeta);
    }

    const highlightStyle = parsed.createElement('style');
    highlightStyle.textContent = [
      '[data-od-editable="1"]{cursor:pointer;transition:outline-color .1s ease;}',
      '[data-od-editable="1"][data-od-hover="1"]{outline:2px dashed #3b82f6;outline-offset:2px;}',
      '[data-od-editable="1"][data-od-selected="1"]{outline:2px solid #1d4ed8;outline-offset:2px;box-shadow:0 0 0 2px rgba(37,99,235,0.18);}',
    ].join('\n');
    head.appendChild(highlightStyle);

    const bridgeEl = parsed.createElement('script');
    bridgeEl.textContent = BRIDGE_SCRIPT;
    body.appendChild(bridgeEl);
  }

  return { html: '<!doctype html>' + parsed.documentElement.outerHTML, skipped };
}

export interface BuilderPreviewFrameProps {
  html: string;
  viewport: BuilderViewport;
  interactive?: boolean;
  selectedId?: string | null;
  document?: BuilderDocument;
  onSelectNode?: (id: string) => void;
  onHoverNode?: (id: string | null) => void;
  onApiError?: (error: unknown, fallback: string) => void;
  title?: string;
  className?: string;
  frameClassName?: string;
  onPrepared?: (stats: { skipped: number }) => void;
}

export function BuilderPreviewFrame({
  html,
  viewport,
  interactive = false,
  selectedId = null,
  document: builderDocument,
  onSelectNode,
  onHoverNode,
  onApiError,
  title = 'Preview Builder (sandbox opaque)',
  className = '',
  frameClassName = '!h-[650px] !border-0',
  onPrepared,
}: BuilderPreviewFrameProps) {
  const [preparedHtml, setPreparedHtml] = useState<string>('');
  const [preparing, setPreparing] = useState<boolean>(false);
  const iframeRef = useRef<HTMLIFrameElement | null>(null);
  const nonceRef = useRef<string>(generateStudioNonce());
  const scrollRef = useRef<{ x: number; y: number }>({ x: 0, y: 0 });
  const selectedIdRef = useRef<string | null>(selectedId);
  selectedIdRef.current = selectedId;
  const documentRef = useRef<BuilderDocument | undefined>(builderDocument);
  documentRef.current = builderDocument;
  const lastPickedFromIframeRef = useRef<string | null>(null);
  const seqRef = useRef(0);
  const onSelectNodeRef = useRef(onSelectNode);
  onSelectNodeRef.current = onSelectNode;
  const onHoverNodeRef = useRef(onHoverNode);
  onHoverNodeRef.current = onHoverNode;
  const onApiErrorRef = useRef(onApiError);
  onApiErrorRef.current = onApiError;
  const onPreparedRef = useRef(onPrepared);
  onPreparedRef.current = onPrepared;

  useEffect(() => {
    const seq = ++seqRef.current;
    if (!html) {
      setPreparedHtml('');
      setPreparing(false);
      return;
    }
    const controller = new AbortController();
    const nextNonce = generateStudioNonce();
    setPreparing(true);
    void preparePreviewMarkup(html, controller.signal, {
      interactive,
      nonce: nextNonce,
      scroll: scrollRef.current,
      selectedId: selectedIdRef.current,
    })
      .then(result => {
        if (controller.signal.aborted || seq !== seqRef.current) return;
        nonceRef.current = nextNonce;
        setPreparedHtml(result.html);
        onPreparedRef.current?.({ skipped: result.skipped });
      })
      .catch(cause => {
        if (controller.signal.aborted || seq !== seqRef.current) return;
        setPreparedHtml('');
        onApiErrorRef.current?.(cause, 'Không thể chuẩn bị dữ liệu Preview.');
      })
      .finally(() => {
        if (seq === seqRef.current) setPreparing(false);
      });
    return () => {
      controller.abort();
    };
  }, [html, interactive]);

  useEffect(() => {
    if (!interactive) return;
    const onMessage = (event: MessageEvent<unknown>) => {
      const targetWindow = iframeRef.current?.contentWindow;
      if (!targetWindow || event.source !== targetWindow) return;
      if (!event.data || typeof event.data !== 'object') return;
      const data = event.data as Record<string, unknown>;
      if (data.nonce !== nonceRef.current) return;

      if (data.type === 'od-studio-scroll') {
        const x = typeof data.x === 'number' && Number.isFinite(data.x) ? Math.max(0, Math.round(data.x)) : 0;
        const y = typeof data.y === 'number' && Number.isFinite(data.y) ? Math.max(0, Math.round(data.y)) : 0;
        scrollRef.current = { x, y };
        return;
      }

      if (data.type === 'od-studio-pick') {
        const nodeId = typeof data.nodeId === 'string' ? data.nodeId : null;
        if (!nodeId) return;
        const doc = documentRef.current;
        if (doc && !locate(doc, nodeId)) return;
        lastPickedFromIframeRef.current = nodeId;
        onSelectNodeRef.current?.(nodeId);
        return;
      }

      if (data.type === 'od-studio-hover') {
        if (data.nodeId === null) {
          onHoverNodeRef.current?.(null);
          return;
        }
        const nodeId = typeof data.nodeId === 'string' ? data.nodeId : null;
        if (!nodeId) return;
        const doc = documentRef.current;
        if (doc && !locate(doc, nodeId)) return;
        onHoverNodeRef.current?.(nodeId);
      }
    };

    window.addEventListener('message', onMessage);
    return () => window.removeEventListener('message', onMessage);
  }, [interactive]);

  useEffect(() => {
    if (!interactive) return;
    const targetWindow = iframeRef.current?.contentWindow;
    if (!targetWindow) return;
    const shouldScroll = lastPickedFromIframeRef.current !== selectedId;
    if (lastPickedFromIframeRef.current === selectedId) {
      lastPickedFromIframeRef.current = null;
    }
    targetWindow.postMessage(
      {
        type: 'od-highlight',
        nonce: nonceRef.current,
        selectedId: selectedId ?? null,
        scroll: shouldScroll,
      },
      '*',
    );
  }, [interactive, selectedId, preparedHtml]);

  const width = VIEWPORT_WIDTHS[viewport];

  return (
    <div
      className={`overflow-auto ${className}`.trim()}
      data-testid="builder-preview-viewport"
      data-viewport={viewport}
      data-viewport-width={width}
      aria-busy={preparing}
    >
      <iframe
        ref={iframeRef}
        id="canvas"
        title={title}
        sandbox={interactive ? 'allow-scripts' : ''}
        referrerPolicy="no-referrer"
        className={`mx-auto bg-white shadow-xs ${frameClassName}`.trim()}
        style={{
          width: viewport === 'desktop' ? '100%' : width,
          minWidth: viewport === 'desktop' ? undefined : width,
          maxWidth: viewport === 'desktop' ? width : 'none',
          display: 'block',
        }}
        srcDoc={preparedHtml}
      />
    </div>
  );
}
