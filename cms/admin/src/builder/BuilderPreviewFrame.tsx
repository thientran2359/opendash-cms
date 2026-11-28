import { useI18n, __ } from '../i18n/index.tsx';
import { useEffect, useRef, useState } from 'react';
import * as api from '../api/index.ts';
import { type BuilderDocument, type BuilderViewport, VIEWPORT_WIDTHS, locate } from './document.ts';
import { applyPreviewMediaRewrites, collectPrivateMediaPaths } from './previewMediaRewrite.ts';

const MEDIA_PREVIEW_LIMIT = 64;
const MEDIA_PREVIEW_BYTES_LIMIT = 48 * 1024 * 1024;

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
  'if(attr==="data-od-selected"){var anc=nodes[j];while(anc){if(anc.tagName==="DETAILS")anc.open=true;anc=anc.parentElement;}}',
  'if(doScroll&&nodes[j].scrollIntoView){nodes[j].scrollIntoView({block:"nearest",behavior:"smooth"});}',
  'break;',
  '}',
  '}',
  '}',
  'restoreScroll();',
  'window.addEventListener("load",restoreScroll,{once:true});',
  'var mSel=document.querySelector(\'meta[name="od-studio-selected"]\');',
  'if(mSel){var initId=mSel.getAttribute("content");if(initId)applyAttr("data-od-selected",initId,false);}',
  'var gearHosts=document.querySelectorAll("[data-od-editable=\\"1\\"][data-node-id]");',
  'var replaced={IMG:1,HR:1,BR:1,INPUT:1,VIDEO:1,AUDIO:1,IFRAME:1,EMBED:1,SOURCE:1};',
  'for(var g=0;g<gearHosts.length;g++){',
  'var host=gearHosts[g];var kind=host.getAttribute("data-od-kind");',
  'if((kind!=="section"&&kind!=="container"&&kind!=="block")||replaced[host.tagName])continue;',
  'var nodeId=host.getAttribute("data-node-id");if(!nodeId)continue;',
  'var gear=document.createElement("button");',
  'gear.type="button";gear.className="od-canvas-gear";',
  'gear.setAttribute("data-od-gear",nodeId);',
  'var gearKind=kind==="section"?"section":kind==="container"?"container":"block";',
  'var labelMeta=document.querySelector(\'meta[name="od-studio-gear-\'+gearKind+\'"]\');',
  'var titleMeta=document.querySelector(\'meta[name="od-studio-gear-title"]\');',
  'gear.setAttribute("aria-label",labelMeta?labelMeta.getAttribute("content")||"":"Block actions");',
  'gear.setAttribute("title",titleMeta?titleMeta.getAttribute("content")||"":"Block actions");',
  'var icon=document.createElementNS("http://www.w3.org/2000/svg","svg");',
  'icon.setAttribute("viewBox","0 0 24 24");',
  'icon.setAttribute("width","16");icon.setAttribute("height","16");',
  'icon.setAttribute("fill","none");icon.setAttribute("stroke","currentColor");',
  'icon.setAttribute("stroke-width","1.8");icon.setAttribute("aria-hidden","true");',
  'var outline=document.createElementNS("http://www.w3.org/2000/svg","path");',
  'outline.setAttribute("d","M10 2h4l.5 2.3 1.8.8 2-1.3 2.8 2.8-1.3 2 .8 1.8L23 10v4l-2.4.5-.8 1.8 1.3 2-2.8 2.8-2-1.3-1.8.8L14 23h-4l-.5-2.4-1.8-.8-2 1.3-2.8-2.8 1.3-2-.8-1.8L1 14v-4l2.4-.5.8-1.8-1.3-2 2.8-2.8 2 1.3 1.8-.8L10 2Z");',
  'var circle=document.createElementNS("http://www.w3.org/2000/svg","circle");',
  'circle.setAttribute("cx","12");circle.setAttribute("cy","12");circle.setAttribute("r","3");',
  'icon.appendChild(outline);icon.appendChild(circle);gear.appendChild(icon);host.appendChild(gear);',
  '}',
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
  'var gear=e.target&&e.target.closest?e.target.closest("[data-od-gear]"):null;',
  'if(gear){',
  'e.preventDefault();e.stopPropagation();',
  'var gearId=gear.getAttribute("data-od-gear");',
  'if(gearId){',
  'applyAttr("data-od-selected",gearId,false);',
  'var gr=gear.getBoundingClientRect();',
  'window.parent.postMessage({type:"od-studio-menu",nonce:nonce,nodeId:gearId,mode:"toggle",rect:{left:Math.round(gr.right),top:Math.round(gr.top)}},"*");',
  '}',
  'return;',
  '}',
  'var el=e.target&&e.target.closest?e.target.closest("[data-od-editable=\\"1\\"][data-node-id]"):null;',
  'var ctl=e.target&&e.target.closest?e.target.closest("a,button,input,select,textarea,form"):null;',
  'if(el||ctl){e.preventDefault();e.stopPropagation();}',
  'if(!el)return;',
  'var nodeId=el.getAttribute("data-node-id");',
  'if(!nodeId)return;',
  'applyAttr("data-od-selected",nodeId,false);',
  'window.parent.postMessage({type:"od-studio-pick",nonce:nonce,nodeId:nodeId,kind:el.getAttribute("data-od-kind")||""},"*");',
  '},true);',
  'document.addEventListener("dblclick",function(e){',
  'if(e.target&&e.target.closest&&e.target.closest("[data-od-gear]"))return;',
  'var el=e.target&&e.target.closest?e.target.closest("[data-od-editable=\\"1\\"][data-node-id]"):null;',
  'if(!el)return;',
  'e.preventDefault();e.stopPropagation();',
  'var nodeId=el.getAttribute("data-node-id");if(!nodeId)return;',
  'applyAttr("data-od-selected",nodeId,false);',
  'window.parent.postMessage({type:"od-studio-edit",nonce:nonce,nodeId:nodeId},"*");',
  '},true);',
  'document.addEventListener("contextmenu",function(e){',
  'var gear=e.target&&e.target.closest?e.target.closest("[data-od-gear]"):null;',
  'var el=gear?null:(e.target&&e.target.closest?e.target.closest("[data-od-editable=\\"1\\"][data-node-id]"):null);',
  'var nodeId=gear?gear.getAttribute("data-od-gear"):(el?el.getAttribute("data-node-id"):null);',
  'if(!nodeId)return;',
  'e.preventDefault();e.stopPropagation();',
  'applyAttr("data-od-selected",nodeId,false);',
  'window.parent.postMessage({type:"od-studio-menu",nonce:nonce,nodeId:nodeId,mode:"open",rect:{left:Math.round(e.clientX||0),top:Math.round(e.clientY||0)}},"*");',
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

const BRIDGE_SCRIPT_SHA256 = 'nhn4sQjJEPssH234n9sJ7uB1zD8gJ1/vb7e2th/SeG8=';

function generateStudioNonce(): string {
  const bytes = new Uint8Array(16);
  crypto.getRandomValues(bytes);
  return Array.from(bytes, b => b.toString(16).padStart(2, '0')).join('');
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
 * - Rewrites `/theme/...`, `/media/...` and the built-in public Tailwind asset to explicit `origin` URLs so CSS,
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
  const scopedStyles = Array.from(parsed.querySelectorAll<HTMLStyleElement>('style[data-od-block-styles], style[data-builder-responsive]'));
  const mediaPaths = collectPrivateMediaPaths(parsed, scopedStyles);
  const replacements = new Map<string, string>();
  let skipped = Math.max(0, mediaPaths.length - MEDIA_PREVIEW_LIMIT);
  let bytes = 0;
  for (const path of mediaPaths.slice(0, MEDIA_PREVIEW_LIMIT)) {
    if (signal.aborted) throw new DOMException(__("Preview was cancelled."), 'AbortError');
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
      !['image/png', 'image/jpeg', 'image/webp', 'video/mp4'].includes(blob.type) ||
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
      reader.onerror = () => reject(reader.error ?? new Error(__("Could not read preview image.")));
      reader.onabort = () => reject(new DOMException(__("Preview was cancelled."), 'AbortError'));
      if (signal.aborted) reader.abort();
      else reader.readAsDataURL(blob);
    });
    replacements.set(path, dataUrl);
  }

  // Rewrite root-relative `/theme/...` & `/media/...` URLs (including srcset) and inline private media.
  const resolvedOrigin = options.origin ?? (typeof window !== 'undefined' ? window.location.origin : '');
  applyPreviewMediaRewrites(parsed, scopedStyles, replacements, resolvedOrigin);

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

    // Keep the bridge script constant; translated strings travel as inert metadata.
    for (const [name, label] of [
      ['section', __("Section actions")],
      ['container', __("Container actions")],
      ['block', __("Block actions")],
      ['title', __("Block actions")],
    ]) {
      const labelMeta = parsed.createElement('meta');
      labelMeta.setAttribute('name', 'od-studio-gear-' + name);
      labelMeta.setAttribute('content', label);
      head.appendChild(labelMeta);
    }

    if (options.selectedId) {
      const selMeta = parsed.createElement('meta');
      selMeta.setAttribute('name', 'od-studio-selected');
      selMeta.setAttribute('content', options.selectedId);
      head.appendChild(selMeta);
    }

    const highlightStyle = parsed.createElement('style');
    highlightStyle.textContent = [
      ':where([data-od-editable="1"]){position:relative;cursor:pointer;transition:outline-color .1s ease;}',
      '[data-od-editable="1"][data-od-hover="1"]{outline:1px dashed #2563eb;outline-offset:-1px;}',
      '[data-od-editable="1"][data-od-selected="1"]{outline:2px solid #2563eb;outline-offset:-1px;}',
      '.od-canvas-gear{position:absolute;top:4px;right:4px;z-index:20;display:flex;align-items:center;justify-content:center;width:24px;height:24px;box-sizing:border-box;margin:0 !important;padding:0;border:1px solid #cbd5e1;border-radius:5px;background:#fff;color:#334155;opacity:0;box-shadow:0 1px 3px rgba(15,23,42,.12);cursor:pointer;line-height:1;transition:opacity .12s ease;}',
      ':where([data-od-editable="1"]:hover, [data-od-editable="1"][data-od-hover="1"], [data-od-editable="1"][data-od-selected="1"]) > .od-canvas-gear, .od-canvas-gear:focus-visible{opacity:.95;}',
      ':where([data-od-kind="section"]) > .od-canvas-gear{left:4px;right:auto;z-index:15;}',
      ':where([data-od-selected="1"]) > .od-canvas-gear{background:#2563eb;border-color:#2563eb;color:#fff;opacity:1;}',
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
  onEditNode?: (id: string) => void;
  onOpenNodeMenu?: (id: string, mode: 'open' | 'toggle', anchor?: { x: number; y: number } | null) => void;
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
  onEditNode,
  onOpenNodeMenu,
  onHoverNode,
  onApiError,
  title,
  className = '',
  frameClassName = '!h-[650px] !border-0',
  onPrepared,
}: BuilderPreviewFrameProps) {
  const { __ } = useI18n();
  const [preparedHtml, setPreparedHtml] = useState<string>('');
  const [preparedSeq, setPreparedSeq] = useState<number>(0);
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
  const onEditNodeRef = useRef(onEditNode);
  onEditNodeRef.current = onEditNode;
  const onOpenNodeMenuRef = useRef(onOpenNodeMenu);
  onOpenNodeMenuRef.current = onOpenNodeMenu;
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
        setPreparedSeq(v => v + 1);
        onPreparedRef.current?.({ skipped: result.skipped });
      })
      .catch(cause => {
        if (controller.signal.aborted || seq !== seqRef.current) return;
        setPreparedHtml('');
        onApiErrorRef.current?.(cause, __("Could not prepare preview data."));
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

      if (data.type === 'od-studio-edit') {
        const nodeId = typeof data.nodeId === 'string' ? data.nodeId : null;
        const doc = documentRef.current;
        if (!nodeId || !doc || !locate(doc, nodeId)) return;
        lastPickedFromIframeRef.current = nodeId;
        onEditNodeRef.current?.(nodeId);
        return;
      }

      if (data.type === 'od-studio-menu') {
        const nodeId = typeof data.nodeId === 'string' ? data.nodeId : null;
        const mode = data.mode === 'open' || data.mode === 'toggle' ? data.mode : null;
        const doc = documentRef.current;
        if (!nodeId || !mode || !doc || !locate(doc, nodeId)) return;
        lastPickedFromIframeRef.current = nodeId;
        let anchor: { x: number; y: number } | null = null;
        const frameRect = iframeRef.current?.getBoundingClientRect();
        const rectObj = data.rect && typeof data.rect === 'object' ? (data.rect as Record<string, unknown>) : null;
        if (frameRect && rectObj && typeof rectObj.left === 'number' && typeof rectObj.top === 'number') {
          anchor = {
            x: Math.round(frameRect.left + rectObj.left),
            y: Math.round(frameRect.top + rectObj.top),
          };
        }
        onOpenNodeMenuRef.current?.(nodeId, mode, anchor);
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

  const sendHighlight = () => {
    if (!interactive) return;
    const targetWindow = iframeRef.current?.contentWindow;
    if (!targetWindow) return;
    const shouldScroll = lastPickedFromIframeRef.current !== selectedIdRef.current;
    if (lastPickedFromIframeRef.current === selectedIdRef.current) {
      lastPickedFromIframeRef.current = null;
    }
    targetWindow.postMessage(
      {
        type: 'od-highlight',
        nonce: nonceRef.current,
        selectedId: selectedIdRef.current ?? null,
        scroll: shouldScroll,
      },
      '*',
    );
  };

  useEffect(() => {
    sendHighlight();
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
      {preparedHtml ? (
        <iframe
          key={`${interactive ? 'interactive' : 'static'}-${preparedSeq}`}
          ref={iframeRef}
          id="canvas"
          title={title ?? __("Builder preview (opaque sandbox)")}
          sandbox={interactive ? 'allow-scripts' : ''}
          referrerPolicy="no-referrer"
          onLoad={sendHighlight}
          className={`mx-auto bg-white shadow-xs ${frameClassName}`.trim()}
          style={{
            width: viewport === 'desktop' ? '100%' : width,
            minWidth: viewport === 'desktop' ? undefined : width,
            maxWidth: 'none',
            display: 'block',
          }}
          srcDoc={preparedHtml}
        />
      ) : (
        <div
          className={`mx-auto flex items-center justify-center bg-white text-xs text-zinc-500 shadow-xs ${frameClassName}`.trim()}
          style={{
            width: viewport === 'desktop' ? '100%' : width,
            minWidth: viewport === 'desktop' ? undefined : width,
            maxWidth: 'none',
          }}
          data-testid="builder-preview-preparing"
        >
          {__("Preparing preview…")}
        </div>
      )}
    </div>
  );
}
