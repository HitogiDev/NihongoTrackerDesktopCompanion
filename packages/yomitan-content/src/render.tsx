import { createElement, Fragment, type ReactNode } from 'react';
import type { LookupRequest, StructuredContentElement } from './types';

const TAGS = new Set(['div', 'span', 'li', 'ul', 'ol', 'ruby', 'rt', 'br']);
const CLASS_STYLES: Record<string, string> = {
  tag: 'badge badge-xs badge-ghost align-middle mr-1',
  'extra-box': 'surface-muted rounded-box px-2 py-1 mt-1',
};
const CONTENT_STYLES: Record<string, string> = {
  'part-of-speech-info': 'text-info',
  'misc-info': 'text-warning',
  'field-info': 'text-accent',
  'example-sentence': 'text-sm',
  'example-keyword': 'font-bold text-primary',
  xref: 'text-sm',
};
const TOKEN = /^[A-Za-z][A-Za-z0-9_-]{0,63}$/;

export function renderStructuredContent(node: unknown, onLookup?: (request: LookupRequest) => void): ReactNode {
  return renderNode(node, onLookup, 'root');
}

function renderNode(node: unknown, onLookup: ((request: LookupRequest) => void) | undefined, key: string): ReactNode {
  if (node === null || node === undefined || typeof node === 'boolean') return null;
  if (typeof node === 'string' || typeof node === 'number') return String(node);
  if (Array.isArray(node)) {
    return node.map((child, index) => <Fragment key={`${key}.${index}`}>{renderNode(child, onLookup, `${key}.${index}`)}</Fragment>);
  }
  if (typeof node !== 'object') return null;

  const element = node as StructuredContentElement;
  const children = renderNode(element.content, onLookup, `${key}.content`);
  const tag = typeof element.tag === 'string' && TAGS.has(element.tag) ? element.tag : 'span';
  const data = isRecord(element.data) ? element.data : {};
  const classes = [
    typeof data.class === 'string' && TOKEN.test(data.class) ? CLASS_STYLES[data.class] : '',
    typeof data.content === 'string' && TOKEN.test(data.content) ? CONTENT_STYLES[data.content] : '',
  ].filter(Boolean).join(' ');
  const props: Record<string, unknown> = { key };
  if (classes) props.className = classes;
  if (typeof element.lang === 'string' && /^[A-Za-z]{2,3}(?:-[A-Za-z0-9]{2,8})*$/.test(element.lang)) props.lang = element.lang;
  if (typeof element.title === 'string' && element.title.length <= 200) props.title = element.title;

  if (element.tag === 'a') {
    const request = lookupRequest(element.href);
    if (request && onLookup) {
      return createElement('button', {
        ...props,
        type: 'button',
        className: [classes, 'link link-primary cursor-pointer text-start'].filter(Boolean).join(' '),
        onClick: () => onLookup(request),
      }, children);
    }
    return createElement('span', props, children);
  }
  return createElement(tag, props, children);
}

function lookupRequest(href: unknown): LookupRequest | undefined {
  if (typeof href !== 'string' || !href.startsWith('?')) return undefined;
  const params = new URLSearchParams(href.slice(1));
  const query = params.get('query');
  if (!query || query.length > 200) return undefined;
  return { query, reading: params.get('reading') ?? undefined };
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
