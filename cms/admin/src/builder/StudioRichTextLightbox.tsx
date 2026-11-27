import { useI18n } from '../i18n/index.tsx';
import { useEffect, useId, useRef, useState } from 'react';
import { createPortal } from 'react-dom';

interface StudioRichTextLightboxProps {
  open: boolean;
  title: string;
  value: string;
  maxBytes?: number | null;
  disabled?: boolean;
  onChange: (nextHtml: string) => void;
  onClose: () => void;
}

function escapeText(raw: string): string {
  return raw
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;');
}

function isSafeLinkHref(raw: string): boolean {
  const s = raw.trim();
  if (!s || s.length > 2048 || /[\s\0\x01-\x1f\x7f]/.test(s)) return false;
  if (s === '#' || /^#[a-zA-Z0-9_-]{1,128}$/.test(s)) return true;
  if (s.startsWith('/') && !s.startsWith('//') && !s.includes('\\') && !s.includes('..')) return true;
  const lower = s.toLowerCase();
  if (lower.startsWith('mailto:') || lower.startsWith('tel:')) return s.length > 5;
  return lower.startsWith('http://') || lower.startsWith('https://');
}

function serializeChildNodes(node: Node, insideBlock: boolean): string {
  if (node.nodeType === Node.TEXT_NODE) {
    return escapeText(node.textContent ?? '');
  }
  if (node.nodeType !== Node.ELEMENT_NODE) return '';
  const el = node as HTMLElement;
  const tag = el.tagName.toLowerCase();
  if (tag === 'br') return '<br>';
  if (tag === 'script' || tag === 'style') return '';

  const mapped = tag === 'b' ? 'strong' : tag === 'i' ? 'em' : tag;
  const isBlockTag = mapped === 'p' || mapped === 'blockquote' || mapped === 'ul' || mapped === 'ol' || mapped === 'li';
  const inner = Array.from(el.childNodes)
    .map(child => serializeChildNodes(child, insideBlock || isBlockTag))
    .join('');

  if (mapped === 'div') {
    const clean = inner.trim();
    if (!clean) return '';
    return insideBlock ? clean : `<p>${clean}</p>`;
  }
  if (mapped === 'a') {
    const href = el.getAttribute('href') ?? '';
    if (!isSafeLinkHref(href)) return inner;
    return `<a href="${escapeText(href.trim())}" rel="noopener noreferrer">${inner}</a>`;
  }
  if (['strong', 'em', 'u', 'p', 'ul', 'ol', 'li', 'blockquote'].includes(mapped)) {
    if ((mapped === 'p' || mapped === 'blockquote') && !inner.trim()) return '';
    return `<${mapped}>${inner}</${mapped}>`;
  }
  return inner;
}

export function serializeCleanRichHtml(root: HTMLElement): string {
  const parts: string[] = [];
  let inlineBuffer = '';
  const flushInline = () => {
    const trimmed = inlineBuffer.trim();
    if (trimmed) parts.push(`<p>${trimmed}</p>`);
    inlineBuffer = '';
  };

  for (const child of Array.from(root.childNodes)) {
    if (child.nodeType === Node.TEXT_NODE) {
      inlineBuffer += escapeText(child.textContent ?? '');
      continue;
    }
    if (child.nodeType !== Node.ELEMENT_NODE) continue;
    const tag = (child as HTMLElement).tagName.toLowerCase();
    if (['p', 'div', 'ul', 'ol', 'blockquote'].includes(tag)) {
      flushInline();
      const serialized = serializeChildNodes(child, false);
      if (serialized) parts.push(serialized);
    } else {
      inlineBuffer += serializeChildNodes(child, true);
    }
  }
  flushInline();
  return parts.join('');
}

export function stripRichHtmlToPreview(html: string): string {
  if (!html) return '';
  if (typeof DOMParser !== 'undefined') {
    try {
      const doc = new DOMParser().parseFromString(html, 'text/html');
      return (doc.body.textContent ?? '').replace(/\s+/g, ' ').trim();
    } catch {
      // Fallback below if parser unavailable
    }
  }
  return html.replace(/<[^>]+>/g, ' ').replace(/\s+/g, ' ').trim();
}

export function StudioRichTextLightbox({
  open, title, value, maxBytes, disabled = false, onChange, onClose,
}: StudioRichTextLightboxProps) {
  const { __, _x } = useI18n();
  const headingId = useId();
  const editorRef = useRef<HTMLDivElement>(null);
  const savedRangeRef = useRef<Range | null>(null);
  const [mode, setMode] = useState<'visual' | 'html'>('visual');
  const [showLinkInput, setShowLinkInput] = useState(false);
  const [linkUrl, setLinkUrl] = useState('https://');
  const [linkError, setLinkError] = useState('');

  useEffect(() => {
    if (!open) return;
    setMode('visual');
    setShowLinkInput(false);
    setLinkError('');
  }, [open]);

  useEffect(() => {
    if (!open || mode !== 'visual') return;
    const el = editorRef.current;
    if (!el) return;
    if (window.document.activeElement !== el) {
      el.innerHTML = value || '<p></p>';
    }
  }, [open, mode, value]);

  useEffect(() => {
    if (!open) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.preventDefault();
        onClose();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [open, onClose]);

  if (!open || typeof window === 'undefined') return null;

  const byteCount = new TextEncoder().encode(value).length;
  const overLimit = maxBytes != null && byteCount > maxBytes;

  const syncFromVisual = () => {
    const el = editorRef.current;
    if (!el || disabled) return;
    onChange(serializeCleanRichHtml(el));
  };

  const execCmd = (command: string, arg?: string) => {
    if (disabled || !editorRef.current) return;
    editorRef.current.focus();
    window.document.execCommand(command, false, arg);
    syncFromVisual();
  };

  const openLinkBar = () => {
    if (disabled) return;
    const sel = window.getSelection();
    if (sel && sel.rangeCount > 0) {
      savedRangeRef.current = sel.getRangeAt(0).cloneRange();
    }
    setLinkError('');
    setShowLinkInput(prev => !prev);
  };

  const applyLink = () => {
    const clean = linkUrl.trim();
    if (!isSafeLinkHref(clean)) {
      setLinkError(__("Invalid URL (use /..., https://..., http://..., #..., mailto: or tel:)."));
      return;
    }
    const el = editorRef.current;
    if (!el) return;
    el.focus();
    const sel = window.getSelection();
    if (savedRangeRef.current && sel) {
      sel.removeAllRanges();
      sel.addRange(savedRangeRef.current);
    }
    window.document.execCommand('createLink', false, clean);
    syncFromVisual();
    setShowLinkInput(false);
    setLinkError('');
  };

  const toolBtn = (label: string, titleText: string, onClick: () => void, active = false) => (
    <button
      key={label}
      type="button"
      title={titleText}
      disabled={disabled}
      onMouseDown={e => e.preventDefault()}
      onClick={onClick}
      className={'rounded px-2 py-1 text-xs font-semibold transition-colors disabled:opacity-40 ' +
        (active ? 'bg-sky-600 text-white' : 'border border-zinc-200 bg-white text-zinc-700 hover:bg-zinc-100')}
    >
      {label}
    </button>
  );

  return createPortal(
    <div
      className="fixed inset-0 z-[80] flex items-center justify-center bg-black/45 p-4"
      role="dialog"
      aria-modal="true"
      aria-labelledby={headingId}
      data-testid="studio-richtext-lightbox"
      onMouseDown={e => { if (e.target === e.currentTarget) onClose(); }}
    >
      <div className="flex max-h-[85vh] w-full max-w-2xl flex-col overflow-hidden rounded-xl border border-zinc-200 bg-white shadow-2xl">
        <header className="flex flex-wrap items-center justify-between gap-2 border-b border-zinc-200 bg-zinc-50 px-4 py-2.5">
          <div className="min-w-0">
            <h2 id={headingId} className="!m-0 truncate text-sm font-semibold text-zinc-900">
              {__("Text Editor · {{title}}", { title })}
            </h2>
          </div>
          <div className="flex items-center gap-2">
            <div className="inline-flex rounded-md border border-zinc-200 bg-white p-0.5 text-xs">
              <button
                type="button"
                onClick={() => setMode('visual')}
                className={'rounded px-2.5 py-1 font-medium ' +
                  (mode === 'visual' ? 'bg-sky-600 text-white' : 'text-zinc-600 hover:text-zinc-900')}
              >{__("Visual")}
              </button>
              <button
                type="button"
                onClick={() => setMode('html')}
                className={'rounded px-2.5 py-1 font-medium ' +
                  (mode === 'html' ? 'bg-sky-600 text-white' : 'text-zinc-600 hover:text-zinc-900')}
              >{__("HTML source")}
              </button>
            </div>
            <button
              type="button"
              onClick={onClose}
              data-testid="studio-richtext-done"
              className="rounded-md bg-sky-600 px-3 py-1.5 text-xs font-semibold text-white hover:bg-sky-700"
            >{__("Done")}
            </button>
          </div>
        </header>

        {mode === 'visual' ? (
          <div className="flex min-h-0 flex-1 flex-col">
            <div className="flex flex-wrap items-center gap-1.5 border-b border-zinc-200 bg-zinc-50/70 px-3 py-2" role="toolbar" aria-label={__("Text formatting tools")}>
              {toolBtn('B', __("Bold (Ctrl/Cmd+B)"), () => execCmd('bold'))}
              {toolBtn('I', __("Italic (Ctrl/Cmd+I)"), () => execCmd('italic'))}
              {toolBtn('U', __("Underline (Ctrl/Cmd+U)"), () => execCmd('underline'))}
              <span className="mx-0.5 h-4 w-px bg-zinc-300" aria-hidden="true" />
              {toolBtn(__("¶ Paragraph"), __("Paragraph"), () => execCmd('formatBlock', 'p'))}
              {toolBtn(__("❝ Quote"), __("Block quote"), () => execCmd('formatBlock', 'blockquote'))}
              {toolBtn(__("• List"), __("Bulleted list"), () => execCmd('insertUnorderedList'))}
              {toolBtn(__("1. List"), __("Numbered list"), () => execCmd('insertOrderedList'))}
              <span className="mx-0.5 h-4 w-px bg-zinc-300" aria-hidden="true" />
              {toolBtn(__("🔗 Link"), _x("Insert link", "builder.studio_rich_text_lightbox"), openLinkBar, showLinkInput)}
              {toolBtn(__("✕ Clear formatting"), __("Clear selected formatting"), () => execCmd('removeFormat'))}
            </div>

            {showLinkInput && (
              <div className="flex flex-wrap items-center gap-2 border-b border-sky-100 bg-sky-50/70 px-3 py-2 text-xs">
                <input
                  type="text"
                  value={linkUrl}
                  onChange={e => setLinkUrl(e.target.value)}
                  placeholder={__("https://... or /path")}
                  className="h-7 min-w-0 flex-1 rounded border border-zinc-300 bg-white px-2 text-xs"
                />
                <button
                  type="button"
                  onClick={applyLink}
                  className="rounded bg-sky-600 px-2.5 py-1 font-semibold text-white hover:bg-sky-700"
                >{_x("Insert link", "builder.studio_rich_text_lightbox")}
                </button>
                <button
                  type="button"
                  onClick={() => execCmd('unlink')}
                  className="rounded border border-zinc-300 bg-white px-2 py-1 text-zinc-700 hover:bg-zinc-100"
                >{__("Remove link")}
                </button>
                {linkError && <p className="w-full text-[11px] text-rose-700">{linkError}</p>}
              </div>
            )}

            <div
              ref={editorRef}
              contentEditable={!disabled}
              suppressContentEditableWarning
              onInput={syncFromVisual}
              onBlur={syncFromVisual}
              data-testid="studio-richtext-visual-canvas"
              className="min-h-[260px] flex-1 overflow-y-auto px-5 py-4 text-sm leading-relaxed text-zinc-900 focus:outline-none [&_a]:text-sky-600 [&_a]:underline [&_blockquote]:border-l-2 [&_blockquote]:border-zinc-300 [&_blockquote]:pl-3 [&_blockquote]:italic [&_ol]:list-decimal [&_ol]:pl-5 [&_p]:mb-2 [&_ul]:list-disc [&_ul]:pl-5"
            />
          </div>
        ) : (
          <div className="flex min-h-0 flex-1 flex-col p-4">
            <textarea
              value={value}
              disabled={disabled}
              rows={12}
              data-testid="studio-richtext-html-source"
              onChange={e => onChange(e.target.value)}
              className="min-h-[260px] w-full flex-1 rounded-lg border border-zinc-300 bg-zinc-50 p-3 font-mono text-xs text-zinc-900 focus:outline-2 focus:outline-sky-600"
            />
          </div>
        )}

        <footer className="flex flex-wrap items-center justify-between gap-2 border-t border-zinc-200 bg-zinc-50 px-4 py-2 text-[11px] text-zinc-500">
          <span>{__("Supported: p, br, strong, em, u, ul, ol, li, blockquote, a")}</span>
          <span className={overLimit ? 'font-semibold text-amber-700' : ''}>
            {byteCount} bytes{maxBytes ? ` / ${maxBytes} bytes` : ''}
          </span>
        </footer>
      </div>
    </div>,
    window.document.body,
  );
}
