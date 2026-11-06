// Safe markdown renderer for messages (no raw HTML). "Quoted speech" is highlighted
// and *actions* are muted, the usual roleplay look.

import { Children, memo, type ReactNode } from 'react';
import ReactMarkdown, { type Components } from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { stripHtmlComments } from '../lib/utils';

const QUOTE = /("[^"\n]+"|“[^”\n]+”)/g;

function highlightQuotes(children: ReactNode): ReactNode {
  return Children.map(children, (child) => {
    if (typeof child !== 'string') return child;
    return child.split(QUOTE).map((part, i) =>
      i % 2 === 1 ? (
        <span key={i} className="text-quote">
          {part}
        </span>
      ) : (
        part
      ),
    );
  });
}

const components: Components = {
  p: ({ children }) => <p>{highlightQuotes(children)}</p>,
  li: ({ children }) => <li>{highlightQuotes(children)}</li>,
  // Links would navigate the app webview away; show them as text with the URL on hover.
  a: ({ children, href }) => (
    <span className="underline decoration-dotted underline-offset-2" title={href}>
      {children}
    </span>
  ),
};

export const Markdown = memo(function Markdown({ text }: { text: string }) {
  return (
    <div className="message-body">
      <ReactMarkdown remarkPlugins={[remarkGfm]} components={components}>
        {stripHtmlComments(text)}
      </ReactMarkdown>
    </div>
  );
});
