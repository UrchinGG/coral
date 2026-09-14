import ReactMarkdown from "react-markdown";

export function ReleaseNotes({ children }: { children: string }) {
  return (
    <div className="text-[12px] text-white/40 leading-relaxed">
      <ReactMarkdown
        components={{
          p: ({ children }) => <p className="mb-2 last:mb-0">{children}</p>,
          ul: ({ children }) => <ul className="list-disc list-inside space-y-1 mb-2">{children}</ul>,
          ol: ({ children }) => <ol className="list-decimal list-inside space-y-1 mb-2">{children}</ol>,
          li: ({ children }) => <li>{children}</li>,
          strong: ({ children }) => <strong className="text-white/70 font-semibold">{children}</strong>,
          em: ({ children }) => <em className="italic">{children}</em>,
          a: ({ href, children }) => (
            <a href={href} target="_blank" rel="noreferrer" className="text-white/60 hover:text-white/80 underline underline-offset-2">
              {children}
            </a>
          ),
          code: ({ children }) => <Mono>{children}</Mono>,
          h1: ({ children }) => <h4 className="text-white/70 font-semibold text-sm mt-3 mb-1 first:mt-0">{children}</h4>,
          h2: ({ children }) => <h4 className="text-white/70 font-semibold text-sm mt-3 mb-1 first:mt-0">{children}</h4>,
          h3: ({ children }) => <h4 className="text-white/70 font-semibold text-sm mt-3 mb-1 first:mt-0">{children}</h4>,
        }}
      >
        {children}
      </ReactMarkdown>
    </div>
  );
}

function Mono({ children }: { children: React.ReactNode }) {
  return <code className="text-[11px] text-white/70 bg-white/[0.06] px-1 py-0.5 rounded">{children}</code>;
}
