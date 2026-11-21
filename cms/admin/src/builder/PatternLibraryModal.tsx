import { useEffect, useRef, useState } from 'react';
import { getThemePattern, listThemePatterns, type ThemePatternItem } from '../api/patterns.ts';
import type { BuilderDocument } from './document.ts';

interface Props {
  onClose: () => void;
  onInsert: (document: BuilderDocument, title: string) => boolean;
  onApiError: (error: unknown, fallback: string) => void;
}

/** Theme Library is a read-only source. Only the parent's explicit insertion
 * handler is allowed to change the unsaved Builder document. */
export function PatternLibraryModal({ onClose, onInsert, onApiError }: Props) {
  const [category, setCategory] = useState('all');
  const [search, setSearch] = useState('');
  const [query, setQuery] = useState('');
  const [categories, setCategories] = useState<string[]>(['all']);
  const [items, setItems] = useState<ThemePatternItem[]>([]);
  const [loading, setLoading] = useState(true);
  const [inserting, setInserting] = useState<string | null>(null);
  const [error, setError] = useState('');
  const searchRef = useRef<HTMLInputElement>(null);
  const dialogRef = useRef<HTMLDivElement>(null);
  const insertAbort = useRef<AbortController | null>(null);

  useEffect(() => {
    const previouslyFocused = document.activeElement as HTMLElement | null;
    searchRef.current?.focus();
    return () => {
      insertAbort.current?.abort();
      previouslyFocused?.focus();
    };
  }, []);

  useEffect(() => {
    const timer = window.setTimeout(() => setQuery(search), 250);
    return () => window.clearTimeout(timer);
  }, [search]);

  useEffect(() => {
    const controller = new AbortController();
    setLoading(true);
    setError('');
    listThemePatterns(category, query, controller.signal)
      .then(catalog => {
        if (controller.signal.aborted) return;
        setCategories(catalog.categories);
        setItems(catalog.items);
      })
      .catch(cause => {
        if (controller.signal.aborted) return;
        setError('Không tải được danh sách mẫu. Hãy thử tìm kiếm lại.');
        setItems([]);
        onApiError(cause, 'Không tải được thư viện Pattern.');
      })
      .finally(() => { if (!controller.signal.aborted) setLoading(false); });
    return () => controller.abort();
  }, [category, query]);

  const insert = async (item: ThemePatternItem) => {
    if (inserting) return;
    const controller = new AbortController();
    insertAbort.current = controller;
    setInserting(item.key);
    setError('');
    try {
      const result = await getThemePattern(item.key, controller.signal);
      if (controller.signal.aborted) return;
      if (result.source !== 'theme' || result.key !== item.key) {
        throw new Error('Mẫu trả về không trùng với mục đã chọn.');
      }
      if (onInsert(result.document, result.title)) onClose();
      else setError('Mẫu chưa được chèn. Kiểm tra thông báo của Builder.');
    } catch (cause) {
      if (controller.signal.aborted) return;
      setError('Không thể chèn mẫu. Bố cục hiện tại vẫn được giữ.');
      onApiError(cause, 'Không tải được mẫu từ theme.');
    } finally {
      if (!controller.signal.aborted) setInserting(null);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-slate-950/55 p-3 sm:p-6"
      data-testid="pattern-library-overlay" onMouseDown={event => {
        if (event.target === event.currentTarget && !inserting) onClose();
      }}>
      <div ref={dialogRef} role="dialog" aria-modal="true" aria-labelledby="pattern-library-heading"
        className="flex h-[min(90vh,850px)] w-full max-w-6xl flex-col overflow-hidden rounded-2xl border border-slate-200 bg-white shadow-2xl"
        data-testid="pattern-library-modal"
        onKeyDown={event => {
          if (event.key === 'Escape' && !inserting) { event.preventDefault(); onClose(); }
          if (event.key !== 'Tab') return;
          const focusable = Array.from(dialogRef.current?.querySelectorAll<HTMLElement>(
            'button:not(:disabled), input:not(:disabled), select:not(:disabled), a[href]'
          ) ?? []);
          if (!focusable.length) return;
          if (event.shiftKey && document.activeElement === focusable[0]) {
            event.preventDefault();
            focusable[focusable.length - 1].focus();
          } else if (!event.shiftKey && document.activeElement === focusable[focusable.length - 1]) {
            event.preventDefault();
            focusable[0].focus();
          }
        }}>
        <header className="flex flex-wrap items-start justify-between gap-3 border-b border-slate-200 p-5">
          <div>
            <h2 id="pattern-library-heading" className="!m-0 text-xl font-semibold text-slate-900">
              Explore all patterns
            </h2>
            <p className="mt-1 text-sm text-slate-600">Mẫu từ theme đang kích hoạt. Chèn một bản sao để chỉnh sửa độc lập.</p>
          </div>
          <button type="button" className="btn-secondary" aria-label="Đóng thư viện Pattern"
            disabled={Boolean(inserting)} onClick={onClose}>Đóng</button>
        </header>
        <div className="border-b border-slate-200 p-4 sm:px-5">
          <label htmlFor="pattern-library-search" className="mb-1 block text-xs font-semibold text-slate-700">
            Tìm mẫu
          </label>
          <input ref={searchRef} id="pattern-library-search" type="search"
            className="w-full max-w-lg text-sm" placeholder="Tìm theo tên, mô tả hoặc danh mục…"
            value={search} onChange={event => setSearch(event.target.value)} />
        </div>
        <div className="flex min-h-0 flex-1 flex-col sm:flex-row">
          <nav aria-label="Danh mục Pattern" className="flex shrink-0 gap-1 overflow-x-auto border-b border-slate-200 p-3 sm:w-52 sm:flex-col sm:overflow-y-auto sm:border-b-0 sm:border-r">
            {categories.map(name => <button key={name} type="button"
              className={'shrink-0 rounded-lg px-3 py-2 text-left text-sm ' +
                (name === category ? 'bg-sky-100 font-semibold text-sky-900' : 'text-slate-700 hover:bg-slate-100')}
              aria-pressed={name === category} onClick={() => setCategory(name)}>
              {name === 'all' ? 'Tất cả' : name}
            </button>)}
          </nav>
          <div className="min-h-0 flex-1 overflow-y-auto p-4 sm:p-5">
            {error && <p role="alert" className="mb-3 rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-800">{error}</p>}
            {loading ? <p role="status" className="text-sm text-slate-600">Đang tải thư viện mẫu…</p>
              : items.length === 0
                ? <p className="rounded-lg border border-dashed border-slate-300 p-8 text-center text-sm text-slate-600">
                  Không có mẫu phù hợp trong theme đang kích hoạt.
                </p>
                : <div className="grid grid-cols-1 gap-4 md:grid-cols-2 xl:grid-cols-3" data-testid="pattern-library-grid">
                  {items.map(item => <article key={item.id}
                    className="flex min-w-0 flex-col overflow-hidden rounded-xl border border-slate-200 bg-white shadow-sm">
                    {item.preview_url?.startsWith('/theme/') && !item.preview_url.startsWith('//')
                      ? <img src={item.preview_url} alt={'Xem trước: ' + item.title}
                          loading="lazy" className="h-40 w-full bg-slate-100 object-cover" />
                      : <div className="flex h-40 items-center justify-center bg-slate-100 text-sm text-slate-500">
                        {item.category}
                      </div>}
                    <div className="flex flex-1 flex-col gap-2 p-4">
                      <span className="text-xs font-medium uppercase tracking-wide text-sky-700">{item.category}</span>
                      <h3 className="!m-0 text-base font-semibold text-slate-900">{item.title}</h3>
                      {item.description && <p className="text-sm text-slate-600">{item.description}</p>}
                      <button type="button" className="mt-auto w-full" disabled={Boolean(inserting)}
                        data-testid={'insert-theme-pattern-' + item.key}
                        onClick={() => void insert(item)}>
                        {inserting === item.key ? 'Đang chèn…' : 'Chèn bản sao'}
                      </button>
                    </div>
                  </article>)}
                </div>}
          </div>
        </div>
      </div>
    </div>
  );
}
