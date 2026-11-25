import { useI18n } from '../i18n/index.tsx';
import { useEffect, useMemo, useRef, useState } from 'react';
import type { DragEvent, KeyboardEvent, ReactNode } from 'react';
import type { BlockCatalogItemDto } from '../types.ts';
import {
  type BuilderDocument, type BuilderElement, type VisibleNodeLocation,
  isLockedElement, isProtectedNode, nodeTitle, visibleNodesInTree,
} from '../builder/document.ts';

export type StudioDropPosition = 'before' | 'after';
export type StudioMenuRequest = {
  nodeId: string;
  mode: 'open' | 'toggle';
  seq: number;
  anchor?: { x: number; y: number } | null;
};

export interface StudioBlockTreeProps {
  document: BuilderDocument;
  selectedId: string | null;
  blockSchemas?: BlockCatalogItemDto[];
  disabled?: boolean;
  expandedIds: Set<string>;
  onExpandedIdsChange: (next: Set<string>) => void;
  menuRequest?: StudioMenuRequest | null;
  onMenuRequestHandled?: () => void;
  canvasPickSeq?: number;
  onSelect: (id: string) => void;
  onEdit: (id: string) => void;
  onAdd: (parentId: string | null) => void;
  onMove: (id: string, direction: -1 | 1) => void;
  onReorder: (sourceId: string, targetId: string, position: StudioDropPosition) => void;
  /** Validate against reorderSiblingNode / preservesAnchors in the editor before showing a drop target. */
  canReorder: (sourceId: string, targetId: string, position: StudioDropPosition) => boolean;
  onDuplicate: (id: string) => void;
  onCopy: (id: string) => void;
  onPaste: (id: string) => void;
  canPaste: (id: string) => boolean;
  onDelete: (id: string) => void;
  canDelete?: (id: string) => boolean;
  onOpenPatternLibrary?: () => void;
  onOpenMediaPicker?: () => void;
}

type IconName =
  | 'settings' | 'chevron-down' | 'chevron-right' | 'plus' | 'pencil'
  | 'copy' | 'paste' | 'up' | 'down' | 'trash' | 'lock' | 'sliders';

/** Tiny inline SVGs keep the Studio tree independent of an icon package. */
function TreeIcon({ name }: { name: IconName }) {
  const paths: Record<IconName, ReactNode> = {
    settings: <><path d="m9.7 4 .4-1.5h3.8l.4 1.5 1.5.9 1.5-.4 1.9 3.3-1.1 1.1v1.8l1.1 1.1-1.9 3.3-1.5-.4-1.5.9-.4 1.5h-3.8l-.4-1.5-1.5-.9-1.5.4-1.9-3.3 1.1-1.1V8.9L4.8 7.8l1.9-3.3 1.5.4z" /><circle cx="12" cy="10" r="2.3" /></>,
    'chevron-down': <path d="m6 9 6 6 6-6" />,
    'chevron-right': <path d="m9 6 6 6-6 6" />,
    plus: <path d="M12 5v14M5 12h14" />,
    pencil: <><path d="m16 4 4 4-11 11-5 1 1-5z" /><path d="m14 6 4 4" /></>,
    copy: <><rect x="8" y="8" width="12" height="12" rx="2" /><path d="M16 8V6a2 2 0 0 0-2-2H6a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2h2" /></>,
    paste: <><path d="M9 4h6v3H9zM8 5H6a2 2 0 0 0-2 2v13h16V7a2 2 0 0 0-2-2h-2" /><path d="M8 13h8M8 17h6" /></>,
    up: <><path d="m12 5-5 5m5-5 5 5M12 5v14" /></>,
    down: <><path d="m12 19-5-5m5 5 5-5M12 19V5" /></>,
    trash: <><path d="M4 7h16M10 4h4l1 3M6 7l1 13h10l1-13M10 11v6M14 11v6" /></>,
    lock: <><rect x="5" y="10" width="14" height="11" rx="2" /><path d="M8 10V7a4 4 0 1 1 8 0v3" /></>,
    sliders: <><path d="M4 7h16M4 17h16" /><circle cx="9" cy="7" r="2" fill="white" /><circle cx="15" cy="17" r="2" fill="white" /></>,
  };
  return <svg width="15" height="15" viewBox="0 0 24 24" fill="none"
    stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round"
    aria-hidden="true" className="shrink-0">{paths[name]}</svg>;
}

export function StudioBlockTree({
  document, selectedId, blockSchemas = [], disabled = false,
  expandedIds, onExpandedIdsChange, menuRequest, onMenuRequestHandled,
  canvasPickSeq = 0,
  onSelect, onEdit, onAdd, onMove, onReorder, canReorder,
  onDuplicate, onCopy, onPaste, canPaste, onDelete, canDelete,
}: StudioBlockTreeProps) {
  const { t } = useI18n();
  const CONTAINER_LABEL = { section: t("studio.section"), row: t("studio.row"), column: t("studio.column") };
  const nodes = useMemo(() => visibleNodesInTree(document), [document]);
  const byId = useMemo(() => new Map(nodes.map(item => [item.node.id, item])), [nodes]);
  const [focusedId, setFocusedId] = useState<string | null>(selectedId);
  const [openMenuId, setOpenMenuId] = useState<string | null>(null);
  const [menuPos, setMenuPos] = useState<{ top: number; left: number } | null>(null);
  const [dragId, setDragId] = useState<string | null>(null);
  const [drop, setDrop] = useState<{ id: string; position: StudioDropPosition } | null>(null);
  const rowRefs = useRef(new Map<string, HTMLButtonElement>());
  const triggerRefs = useRef(new Map<string, HTMLButtonElement>());
  const menuRefs = useRef(new Map<string, HTMLDivElement>());
  const handledMenuSeq = useRef<number | null>(null);
  const previousCanvasPickSeq = useRef(canvasPickSeq);
  const previousSelectedId = useRef(selectedId);

  const isVisible = (item: VisibleNodeLocation): boolean => {
    let parent = item.visualParentId;
    while (parent) {
      if (!expandedIds.has(parent)) return false;
      parent = byId.get(parent)?.visualParentId ?? null;
    }
    return true;
  };
  const visibleNodes = nodes.filter(isVisible);
  const firstVisibleId = visibleNodes[0]?.node.id ?? null;
  const tabStopId = visibleNodes.some(item => item.node.id === focusedId) ? focusedId : firstVisibleId;

  useEffect(() => {
    if (selectedId && byId.has(selectedId)) setFocusedId(selectedId);
  }, [selectedId, byId]);
  // Canvas picks occur inside an iframe, so document pointerdown cannot close
  // a stale menu. Only react to actual selection transitions: opening a gear
  // menu selects that same node and must remain open.
  useEffect(() => {
    if (previousSelectedId.current === selectedId) return;
    previousSelectedId.current = selectedId;
    if (openMenuId && openMenuId !== selectedId) setOpenMenuId(null);
  }, [selectedId, openMenuId]);
  // Picking the already-selected node inside the iframe must dismiss its menu too.
  // Iframe pointer events do not bubble into the parent document.
  useEffect(() => {
    if (previousCanvasPickSeq.current === canvasPickSeq) return;
    previousCanvasPickSeq.current = canvasPickSeq;
    setOpenMenuId(null);
  }, [canvasPickSeq]);
  useEffect(() => {
    if (focusedId && !byId.has(focusedId)) setFocusedId(firstVisibleId);
    if (openMenuId && !byId.has(openMenuId)) setOpenMenuId(null);
  }, [byId, firstVisibleId, focusedId, openMenuId]);
  useEffect(() => {
    if (disabled && openMenuId) setOpenMenuId(null);
  }, [disabled, openMenuId]);

  const computePopoverPos = (id: string, anchor?: { x: number; y: number } | null): { top: number; left: number } => {
    const vw = typeof window !== 'undefined' ? window.innerWidth : 1280;
    const vh = typeof window !== 'undefined' ? window.innerHeight : 800;
    const menuW = 180;
    const menuH = 230;
    let rawTop = 120;
    let rawLeft = 40;
    if (anchor) {
      rawLeft = anchor.x + 8;
      rawTop = anchor.y + 8;
    } else {
      const rect = triggerRefs.current.get(id)?.getBoundingClientRect();
      if (rect) {
        rawLeft = rect.right + 6;
        rawTop = rect.top;
      }
    }
    return {
      left: Math.max(8, Math.min(rawLeft, vw - menuW - 8)),
      top: Math.max(8, Math.min(rawTop, vh - menuH - 8)),
    };
  };

  // Canvas gear requests bring hidden descendants into view before opening the floating popover.
  useEffect(() => {
    if (!menuRequest || handledMenuSeq.current === menuRequest.seq) return;
    handledMenuSeq.current = menuRequest.seq;
    const target = byId.get(menuRequest.nodeId);
    if (target && !disabled) {
      onSelect(menuRequest.nodeId);
      const next = new Set(expandedIds);
      let parent = target.visualParentId;
      while (parent) {
        next.add(parent);
        parent = byId.get(parent)?.visualParentId ?? null;
      }
      if (next.size !== expandedIds.size) onExpandedIdsChange(next);
      setOpenMenuId(current => {
        const willOpen = !(menuRequest.mode === 'toggle' && current === menuRequest.nodeId);
        if (willOpen) setMenuPos(computePopoverPos(menuRequest.nodeId, menuRequest.anchor));
        return willOpen ? menuRequest.nodeId : null;
      });
      setFocusedId(menuRequest.nodeId);
      rowRefs.current.get(menuRequest.nodeId)?.scrollIntoView({ block: 'nearest' });
    }
    onMenuRequestHandled?.();
  }, [menuRequest, byId, expandedIds, disabled, onSelect, onExpandedIdsChange, onMenuRequestHandled]);

  useEffect(() => {
    if (!openMenuId) return;
    const dismiss = (event: PointerEvent) => {
      if (!menuRefs.current.get(openMenuId)?.contains(event.target as Node)
        && !triggerRefs.current.get(openMenuId)?.contains(event.target as Node))
        setOpenMenuId(null);
    };
    window.document.addEventListener('pointerdown', dismiss);
    return () => window.document.removeEventListener('pointerdown', dismiss);
  }, [openMenuId]);
  useEffect(() => {
    if (!openMenuId) return;
    rowRefs.current.get(openMenuId)?.scrollIntoView({ block: 'nearest' });
    const menu = menuRefs.current.get(openMenuId);
    const firstEnabledItem = menu?.querySelector<HTMLButtonElement>('[role="menuitem"]:not(:disabled)');
    if (firstEnabledItem) firstEnabledItem.focus();
    else menu?.focus();
  }, [openMenuId, expandedIds]);

  const focusRow = (id: string) => {
    setFocusedId(id);
    onSelect(id);
    rowRefs.current.get(id)?.focus();
  };
  const closeMenu = (restoreFocus: boolean) => {
    const id = openMenuId;
    setOpenMenuId(null);
    if (restoreFocus && id) triggerRefs.current.get(id)?.focus();
  };
  const openMenuAt = (id: string, anchor?: { x: number; y: number } | null) => {
    if (disabled) return;
    onSelect(id);
    setFocusedId(id);
    setMenuPos(computePopoverPos(id, anchor));
    setOpenMenuId(id);
  };
  const toggleMenu = (id: string) => {
    if (disabled) return;
    onSelect(id);
    setFocusedId(id);
    setOpenMenuId(current => {
      if (current === id) return null;
      setMenuPos(computePopoverPos(id));
      return id;
    });
  };
  const moveAvailable = (item: VisibleNodeLocation, direction: -1 | 1): boolean => {
    if (disabled || isProtectedNode(document, item.node.id, blockSchemas)) return false;
    const siblings = nodes.filter(entry => entry.visualParentId === item.visualParentId);
    const index = siblings.findIndex(entry => entry.node.id === item.node.id);
    const neighbor = siblings[index + direction];
    return !!neighbor && canReorder(item.node.id, neighbor.node.id, direction < 0 ? 'before' : 'after');
  };
  const handleRowKey = (event: KeyboardEvent<HTMLButtonElement>, item: VisibleNodeLocation) => {
    if (event.key === 'ContextMenu' || (event.shiftKey && event.key === 'F10')) {
      event.preventDefault();
      toggleMenu(item.node.id);
      return;
    }
    if (event.altKey && (event.key === 'ArrowUp' || event.key === 'ArrowDown')) {
      event.preventDefault();
      const direction = event.key === 'ArrowUp' ? -1 : 1;
      if (moveAvailable(item, direction)) onMove(item.node.id, direction);
      return;
    }
    const index = visibleNodes.findIndex(entry => entry.node.id === item.node.id);
    let target: string | undefined;
    switch (event.key) {
      case 'ArrowDown': target = visibleNodes[index + 1]?.node.id; break;
      case 'ArrowUp': target = visibleNodes[index - 1]?.node.id; break;
      case 'ArrowLeft':
        if (item.kind !== 'element' && expandedIds.has(item.node.id)) {
          const next = new Set(expandedIds);
          next.delete(item.node.id);
          onExpandedIdsChange(next);
        } else target = item.visualParentId ?? undefined;
        break;
      case 'ArrowRight':
        if (item.kind !== 'element' && !expandedIds.has(item.node.id)) {
          onExpandedIdsChange(new Set([...expandedIds, item.node.id]));
        } else target = nodes.find(entry => entry.visualParentId === item.node.id)?.node.id;
        break;
      case 'Home': target = firstVisibleId ?? undefined; break;
      case 'End': target = visibleNodes[visibleNodes.length - 1]?.node.id; break;
      case 'Enter':
      case ' ':
        event.preventDefault();
        focusRow(item.node.id);
        return;
      default: return;
    }
    event.preventDefault();
    if (target) focusRow(target);
  };
  const handleMenuKey = (event: KeyboardEvent<HTMLDivElement>) => {
    const items = [...event.currentTarget.querySelectorAll<HTMLButtonElement>('[role="menuitem"]:not(:disabled)')];
    const current = items.indexOf(window.document.activeElement as HTMLButtonElement);
    if (event.key === 'Escape') {
      event.preventDefault();
      closeMenu(true);
    } else if (items.length && ['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) {
      event.preventDefault();
      const index = event.key === 'Home' ? 0 : event.key === 'End' ? items.length - 1
        : event.key === 'ArrowDown' ? (current + 1) % items.length : (current - 1 + items.length) % items.length;
      items[index]?.focus();
    }
  };
  const invoke = (id: string, action: (id: string) => void) => {
    closeMenu(false);
    action(id);
  };

  const handleDragStart = (event: DragEvent<HTMLDivElement>, id: string) => {
    if (disabled || isProtectedNode(document, id, blockSchemas)) {
      event.preventDefault();
      return;
    }
    setDragId(id);
    event.dataTransfer.setData('text/plain', id);
    event.dataTransfer.effectAllowed = 'move';
  };
  const handleDragOver = (event: DragEvent<HTMLDivElement>, targetId: string) => {
    event.stopPropagation();
    if (!dragId || disabled || dragId === targetId) {
      setDrop(null);
      return;
    }
    const rect = event.currentTarget.getBoundingClientRect();
    const middle = rect.top + rect.height / 2;
    const position = event.clientY < middle ? 'before' : 'after';
    if (!canReorder(dragId, targetId, position)) {
      setDrop(null);
      return;
    }
    event.preventDefault();
    event.dataTransfer.dropEffect = 'move';
    setDrop({ id: targetId, position });
  };
  const handleDrop = (event: DragEvent<HTMLDivElement>, targetId: string) => {
    event.stopPropagation();
    event.preventDefault();
    const rect = event.currentTarget.getBoundingClientRect();
    const position = event.clientY < rect.top + rect.height / 2 ? 'before' : 'after';
    if (!disabled && dragId && dragId !== targetId && canReorder(dragId, targetId, position))
      onReorder(dragId, targetId, position);
    setDragId(null);
    setDrop(null);
  };
  const leftBounds = (event: DragEvent<HTMLElement>) => {
    const rect = event.currentTarget.getBoundingClientRect();
    return event.clientX < rect.left || event.clientX > rect.right
      || event.clientY < rect.top || event.clientY > rect.bottom;
  };
  const handleDragLeave = (event: DragEvent<HTMLDivElement>, targetId: string) => {
    event.stopPropagation();
    // Ignore dragleave from nested buttons/menu: the pointer is still in the row.
    if (leftBounds(event)) setDrop(current => current?.id === targetId ? null : current);
  };

  const renderLevel = (parentId: string | null, depth: number): ReactNode => (
    <ul role={parentId ? 'group' : 'tree'} aria-label={parentId ? undefined : t("studio.page_structure")}
      className={'m-0 list-none space-y-0.5 p-0 ' + (parentId ? 'ml-2.5 border-l border-slate-200 pl-1' : '')}>
      {nodes.filter(entry => entry.visualParentId === parentId).map(item => {
        const id = item.node.id;
        const isContainer = item.kind !== 'element';
        const expanded = isContainer && expandedIds.has(id);
        const protectedNode = isProtectedNode(document, id, blockSchemas);
        const locked = item.kind === 'element'
          && isLockedElement(item.node as BuilderElement, blockSchemas);
        const label = nodeTitle(item, blockSchemas);
        const htmlId = item.node.meta?.html_id?.trim() || '';
        const canOperate = !disabled && !protectedNode;
        const up = moveAvailable(item, -1);
        const down = moveAvailable(item, 1);
        const toggleCollapse = () => {
          const next = new Set(expandedIds);
          if (next.has(id)) next.delete(id);
          else next.add(id);
          onExpandedIdsChange(next);
        };
        const menuItem = (title: string, icon: IconName, action: () => void, allowed = true) => (
          <button type="button" role="menuitem" key={title} disabled={!allowed || disabled}
            className="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-xs text-slate-700 hover:bg-slate-100 focus-visible:bg-sky-100 disabled:cursor-not-allowed disabled:opacity-40"
            onClick={action}><TreeIcon name={icon} />{title}</button>
        );
        const dropClass = drop?.id === id
          ? drop.position === 'before' ? ' border-t-2 border-t-blue-500' : ' border-b-2 border-b-blue-500'
          : '';
        return <li role="none" key={id}>
          <div
            draggable={canOperate}
            onDragStart={event => { event.stopPropagation(); handleDragStart(event, id); }}
            onDragOver={event => handleDragOver(event, id)}
            onDragLeave={event => handleDragLeave(event, id)}
            onDrop={event => handleDrop(event, id)}
            onDragEnd={() => { setDragId(null); setDrop(null); }}
            onContextMenu={event => {
              event.preventDefault();
              event.stopPropagation();
              openMenuAt(id, { x: event.clientX, y: event.clientY });
            }}
            className={'rounded-lg border border-transparent ' + dropClass
              + (selectedId === id ? ' bg-sky-50' : ' hover:bg-slate-50')}
          >
            <div className="flex min-w-0 items-center gap-0.5 px-1 py-0.5">
              {isContainer ? (
                <button type="button" aria-label={(expanded ? t("studio.collapse_label", { label }) : t("studio.expand_label", { label }))}
                  aria-expanded={expanded} tabIndex={-1} onClick={toggleCollapse}
                  className="rounded p-1 text-slate-500 hover:bg-slate-200 hover:text-slate-800">
                  <TreeIcon name={expanded ? 'chevron-down' : 'chevron-right'} />
                </button>
              ) : (
                <span className="w-4 shrink-0" aria-hidden="true" />
              )}
              {locked && <span title={t("studio.preserved_block_can_only_be_deleted")} className="text-amber-700">
                <TreeIcon name="lock" /></span>}
              <button type="button" role="treeitem" aria-level={depth + 1} aria-selected={selectedId === id}
                aria-keyshortcuts="Shift+F10"
                aria-expanded={isContainer ? expanded : undefined} aria-label={label}
                ref={element => {
                  if (element) rowRefs.current.set(id, element);
                  else rowRefs.current.delete(id);
                }}
                tabIndex={tabStopId === id ? 0 : -1}
                onFocus={() => setFocusedId(id)}
                onClick={() => onSelect(id)}
                onDoubleClick={() => { if (!disabled && !protectedNode) onEdit(id); }}
                onKeyDown={event => handleRowKey(event, item)}
                data-node-id={id}
                className={'flex min-w-0 flex-1 items-center gap-1.5 rounded px-1 py-1 text-left text-xs font-medium focus-visible:outline-2 focus-visible:outline-sky-600 '
                  + (selectedId === id ? 'text-sky-900' : 'text-slate-700')}>
                <span className="truncate">{label}</span>
                {htmlId && (
                  <span className="shrink-0 rounded bg-slate-100 px-1 py-0.5 font-mono text-[10px] font-normal text-slate-500">
                    #{htmlId}
                  </span>
                )}
              </button>
              <button type="button" ref={element => {
                if (element) triggerRefs.current.set(id, element);
                else triggerRefs.current.delete(id);
              }} aria-label={t("studio.open_actions_for_label", { label })} aria-haspopup="menu" aria-expanded={openMenuId === id}
                tabIndex={-1}
                title={t("studio.actions_for_label", { label })}
                className="shrink-0 rounded p-1 text-slate-400 hover:bg-slate-200 hover:text-slate-800 focus-visible:outline-2 focus-visible:outline-sky-600"
                onClick={() => toggleMenu(id)}>
                <TreeIcon name="settings" />
              </button>
            </div>
            {openMenuId === id && <div role="menu" tabIndex={-1} aria-label={t("studio.actions_for_label", { label })}
              ref={element => {
                if (element) menuRefs.current.set(id, element);
                else menuRefs.current.delete(id);
              }}
              onKeyDown={handleMenuKey}
              style={menuPos ? { top: menuPos.top, left: menuPos.left } : undefined}
              className="fixed z-50 grid w-44 grid-cols-1 rounded-lg border border-slate-200 bg-white p-1 shadow-lg ring-1 ring-black/5"
              data-studio-menu-node-id={id}>
              {menuItem(t("studio.edit"), 'pencil', () => invoke(id, onEdit), canOperate)}
              {menuItem(t("studio.duplicate"), 'copy', () => invoke(id, onDuplicate), canOperate)}
              {menuItem(t("studio.copy_options"), 'sliders', () => invoke(id, onCopy), canOperate)}
              {menuItem(t("studio.paste_options"), 'paste', () => invoke(id, onPaste), canOperate && canPaste(id))}
              {menuItem(t("studio.move_up"), 'up', () => invoke(id, nodeId => onMove(nodeId, -1)), up)}
              {menuItem(t("studio.move_down"), 'down', () => invoke(id, nodeId => onMove(nodeId, 1)), down)}
              {menuItem(t("studio.delete"), 'trash', () => invoke(id, onDelete), !disabled && (canDelete?.(id) ?? true))}
            </div>}
          </div>
          {isContainer && expanded && <>
            {renderLevel(id, depth + 1)}
            <button type="button" disabled={disabled} onClick={() => onAdd(id)}
              className="mb-1 ml-3 mt-0.5 flex w-[calc(100%-0.75rem)] items-center gap-1 rounded px-2 py-1 text-left text-xs text-sky-700 hover:bg-sky-50 disabled:opacity-50">
              <TreeIcon name="plus" />{t("studio.add_to_container", { container: CONTAINER_LABEL[item.kind as 'section' | 'row' | 'column'] })}
            </button>
          </>}
        </li>;
      })}
    </ul>
  );
  return <section aria-label={t("studio.page_structure")} className="min-w-0 space-y-2.5 p-0"
    onDragLeave={event => { if (leftBounds(event)) setDrop(null); }}>
    {nodes.length ? renderLevel(null, 0) : (
      <p className="rounded-lg border border-dashed border-slate-200 p-4 text-sm text-slate-500">
        {t("studio.this_page_is_empty_add_a_block_or_section_to_start_the_layout")}</p>
    )}
    <button type="button" disabled={disabled} onClick={() => onAdd(null)}
      className="flex w-full items-center justify-center gap-1.5 rounded-lg border border-sky-200 bg-sky-50 px-3 py-2 text-sm font-semibold text-sky-700 hover:bg-sky-100 disabled:opacity-50">
      <TreeIcon name="plus" />{t("studio.add_block")}
    </button>
    <p className="px-1 text-[10px] text-slate-400">{t("studio.double_click_to_edit_right_click_for_actions_drag_to_reorder")}</p>
  </section>;
}
