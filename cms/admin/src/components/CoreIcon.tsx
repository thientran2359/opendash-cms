import React from 'react';
import { lookupCoreIconPaths } from '../fields/icons.ts';

function parseAttrs(rawAttrs: string): Record<string, string> {
  const out: Record<string, string> = {};
  const attrRe = /([a-zA-Z0-9:-]+)="([^"]*)"/g;
  let m: RegExpExecArray | null;
  while ((m = attrRe.exec(rawAttrs)) !== null) {
    out[m[1]] = m[2];
  }
  return out;
}

/**
 * Safely renders static `CORE_ICONS` vector primitives as native React SVG
 * elements without `dangerouslySetInnerHTML`.
 */
export const CoreIconSvg: React.FC<{ name: string; className?: string; size?: number }> = ({
  name,
  className,
  size = 18,
}) => {
  const raw = lookupCoreIconPaths(name);
  if (!raw) return null;
  const elements: React.ReactNode[] = [];
  const tagRe = /<(path|circle|rect)\s+([^>]*?)\/>/g;
  let match: RegExpExecArray | null;
  let index = 0;
  while ((match = tagRe.exec(raw)) !== null) {
    const [, tag, attrStr] = match;
    const attrs = parseAttrs(attrStr);
    const key = `${tag}-${index++}`;
    if (tag === 'path') elements.push(<path key={key} {...attrs} />);
    else if (tag === 'circle') elements.push(<circle key={key} {...attrs} />);
    else if (tag === 'rect') elements.push(<rect key={key} {...attrs} />);
  }
  return (
    <svg
      aria-hidden="true"
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.8}
      strokeLinecap="round"
      strokeLinejoin="round"
      className={className}
    >
      {elements}
    </svg>
  );
};
