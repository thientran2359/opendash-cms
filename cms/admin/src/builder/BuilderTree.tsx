import React, { useEffect, useRef, useState } from 'react';
import {
  type BuilderDocument, type NodeLocation, type ElementType,
  allNodes, nodeTitle,
} from './document.ts';

interface Props {
  document: BuilderDocument;
  selectedId: string | null;
  elementType: ElementType;
  onSelect: (id: string) => void;
  onAdd: (parentId: string | null, kind: 'section' | 'row' | 'column' | 'element') => void;
  onMove: (id: string, direction: -1 | 1) => void;
  onDelete: (id: string) => void;
}

export function BuilderTree({ document, selectedId, elementType, onSelect, onAdd, onMove, onDelete }: Props) {
  const nodes = allNodes(document);
  const [focusedId, setFocusedId] = useState<string | null>(selectedId);
  const buttons = useRef(new Map<string, HTMLButtonElement>());
  useEffect(() => {
    if (selectedId && selectedId !== focusedId) setFocusedId(selectedId);
  }, [selectedId]);
  useEffect(() => {
    if (focusedId && !nodes.some(item => item.node.id === focusedId)) setFocusedId(nodes[0]?.node.id ?? null);
  }, [nodes.map(item => item.node.id).join('|')]);

  const focusAndSelect = (id: string) => {
    setFocusedId(id);
    onSelect(id);
    buttons.current.get(id)?.focus();
  };
  const handleKey = (event: React.KeyboardEvent<HTMLButtonElement>, item: NodeLocation) => {
    const index = nodes.findIndex(entry => entry.node.id === item.node.id);
    let target: string | undefined;
    if (event.altKey && (event.key === 'ArrowUp' || event.key === 'ArrowDown')) {
      event.preventDefault();
      onMove(item.node.id, event.key === 'ArrowUp' ? -1 : 1);
      return;
    }
    switch (event.key) {
      case 'ArrowDown': target = nodes[index + 1]?.node.id; break;
      case 'ArrowUp': target = nodes[index - 1]?.node.id; break;
      case 'ArrowLeft': target = item.parentId ?? undefined; break;
      case 'ArrowRight': target = nodes.find(entry => entry.parentId === item.node.id)?.node.id; break;
      case 'Home': target = nodes[0]?.node.id; break;
      case 'End': target = nodes[nodes.length - 1]?.node.id; break;
      case 'Enter':
      case ' ':
        event.preventDefault();
        focusAndSelect(item.node.id);
        return;
      default: return;
    }
    event.preventDefault();
    if (target) focusAndSelect(target);
  };
  const renderLevel = (parentId: string | null, depth: number): React.ReactNode => (
    <ul role={parentId ? 'group' : 'tree'} aria-label={parentId ? undefined : 'Cấu trúc homepage'} className="space-y-1">
      {nodes.filter(entry => entry.parentId === parentId).map(item => {
        const nextKind = ({ section: 'row', row: 'column', column: 'element' } as const)[item.kind as 'section' | 'row' | 'column'];
        const label = nodeTitle(item);
        return (
          <li role="none" key={item.node.id}>
            <div className={'min-w-0 overflow-hidden rounded-lg p-1 ' +
              (selectedId === item.node.id ? 'bg-sky-100' : 'hover:bg-slate-50')}>
              <div className="flex min-w-0 items-center" style={{ paddingLeft: depth * 9 }}>
              <button
                ref={element => {
                  if (element) buttons.current.set(item.node.id, element);
                  else buttons.current.delete(item.node.id);
                }}
                type="button" role="treeitem" aria-level={depth + 1} aria-selected={selectedId === item.node.id}
                aria-label={label + ' ' + item.node.id} aria-expanded={item.kind === 'element' ? undefined : true}
                tabIndex={(focusedId ?? nodes[0]?.node.id) === item.node.id ? 0 : -1}
                onFocus={() => setFocusedId(item.node.id)}
                onClick={() => onSelect(item.node.id)}
                onKeyDown={event => handleKey(event, item)}
                data-node-id={item.node.id}
                className={'min-w-0 w-full truncate rounded px-2 py-2 text-left text-xs font-medium ' +
                  (selectedId === item.node.id ? 'bg-sky-700 text-white' : 'bg-transparent text-slate-700')}
              >
                <span aria-hidden="true" className="mr-2 text-slate-400">{item.kind === 'element' ? '▣' : '▾'}</span>{label}
              </button>
              </div>
              {selectedId === item.node.id && (
                <div className="mt-1 flex min-w-0 flex-wrap justify-end gap-1">
                  {nextKind && <button type="button" className="btn-secondary !px-2 !py-1 text-xs"
                    aria-label={'Thêm ' + nextKind + ' vào ' + label}
                    title={'Thêm ' + nextKind} onClick={() => onAdd(item.node.id, nextKind)}>+</button>}
                  <button type="button" className="btn-secondary !px-1.5 !py-1 text-xs" title="Di chuyển lên (Alt+↑)"
                    aria-label={'Di chuyển ' + label + ' lên'} onClick={() => onMove(item.node.id, -1)}>↑</button>
                  <button type="button" className="btn-secondary !px-1.5 !py-1 text-xs" title="Di chuyển xuống (Alt+↓)"
                    aria-label={'Di chuyển ' + label + ' xuống'} onClick={() => onMove(item.node.id, 1)}>↓</button>
                  <button type="button" className="btn-danger !px-1.5 !py-1 text-xs"
                    aria-label={'Xóa ' + label} onClick={() => onDelete(item.node.id)}>×</button>
                </div>
              )}
            </div>
            {item.kind !== 'element' && renderLevel(item.node.id, depth + 1)}
          </li>
        );
      })}
    </ul>
  );
  return (
    <section className="rounded-xl border border-slate-200 bg-white p-3" aria-label="Cây cấu trúc">
      <div className="mb-3 flex flex-wrap items-center justify-between gap-2">
        <h2 className="!m-0 text-sm font-semibold text-slate-900">Cấu trúc trang</h2>
        <button type="button" className="btn-secondary !px-2 !py-1.5 text-xs"
          onClick={() => onAdd(null, 'section')}>+ Section</button>
      </div>
      {document.sections.length ? renderLevel(null, 0) : (
        <p className="text-sm text-slate-500">Chưa có Section. Chọn “+ Section” để bắt đầu.</p>
      )}
      <p className="mt-3 text-xs leading-relaxed text-slate-500">
        Dùng ↑/↓ chọn node, ←/→ đi giữa các tầng, Alt+↑/↓ đổi thứ tự.
        Element mới dùng loại “{elementType}”.
      </p>
    </section>
  );
}
