export type StructuredContent =
  | string
  | number
  | null
  | StructuredContent[]
  | StructuredContentElement;

export interface StructuredContentElement {
  tag?: unknown;
  content?: unknown;
  data?: unknown;
  style?: unknown;
  lang?: unknown;
  title?: unknown;
  href?: unknown;
}

export interface LookupRequest {
  query: string;
  offset?: number;
  reading?: string;
}

export type Glossary = string | StructuredContent;
