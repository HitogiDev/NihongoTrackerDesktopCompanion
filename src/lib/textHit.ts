export function textHitAtPoint(element: HTMLElement, x: number, y: number) {
  const document = element.ownerDocument;
  const caret = document.caretRangeFromPoint?.(x, y);
  if (!caret || !element.contains(caret.startContainer)) return null;
  const walker = document.createTreeWalker(element, NodeFilter.SHOW_TEXT);
  const nodes: { node: Text; offset: number }[] = [];
  let offset = 0;
  while (walker.nextNode()) {
    const node = walker.currentNode as Text;
    if (node.parentElement?.closest('button')) continue;
    nodes.push({ node, offset });
    offset += node.length;
  }
  const range = document.createRange();
  function hit(node: Text, start: number, base: number) {
    const length = String.fromCodePoint(node.data.codePointAt(start)!).length;
    range.setStart(node, start);
    range.setEnd(node, start + length);
    for (const rect of range.getClientRects()) {
      if (x >= rect.left && x < rect.right && y >= rect.top && y < rect.bottom) {
        return { offset: base + start, rect,
          vertical: getComputedStyle(element).writingMode.startsWith('vertical') };
      }
    }
    return null;
  }
  // A caret is an insertion boundary, so test the glyphs on both sides of it.
  const current = nodes.find(({ node }) => node === caret.startContainer);
  if (current) {
    const text = current.node.data;
    let start = caret.startOffset;
    if (start > 0 && start < text.length && /[\uDC00-\uDFFF]/.test(text[start])) start--;
    if (start < text.length) {
      const result = hit(current.node, start, current.offset);
      if (result) return result;
    }
    if (start > 0) {
      let previous = start - 1;
      if (previous > 0 && /[\uDC00-\uDFFF]/.test(text[previous])) previous--;
      const result = hit(current.node, previous, current.offset);
      if (result) return result;
    }
  }
  for (const { node, offset: base } of nodes) {
    for (let start = 0; start < node.length;) {
      const result = hit(node, start, base);
      if (result) return result;
      start += String.fromCodePoint(node.data.codePointAt(start)!).length;
    }
  }
  return null;
}
