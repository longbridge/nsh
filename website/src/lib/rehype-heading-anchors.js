import { visit } from 'unist-util-visit';
import { contentLang } from './markdown.js';

// Every heading already carries an id, which is what makes a deep link work.
// This adds the affordance: a link the reader can click to put that address in
// the URL bar, sitting in the margin so it never shifts the heading text.
export function rehypeHeadingAnchors() {
  return (tree, file) => {
    const lang = contentLang(file?.path ?? file?.history?.[0] ?? '');
    visit(tree, 'element', (node) => {
      if (!/^h[2-4]$/.test(node.tagName)) return;
      const id = node.properties?.id;
      if (!id) return;

      node.children.unshift({
        type: 'element',
        tagName: 'a',
        properties: {
          className: ['heading-anchor'],
          href: `#${id}`,
          ariaLabel: { en: 'Link to this section', 'zh-CN': '本节链接', ko: '이 절의 링크' }[lang],
        },
        children: [{ type: 'text', value: '#' }],
      });
    });
  };
}
