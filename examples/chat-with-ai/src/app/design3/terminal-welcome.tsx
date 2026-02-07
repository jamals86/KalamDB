'use client';

import { Terminal, ArrowRight } from 'lucide-react';

interface TerminalWelcomeProps {
  onNewConversation: () => void;
}

export function TerminalWelcome({ onNewConversation }: TerminalWelcomeProps) {
  return (
    <div className="flex-1 flex items-center justify-center bg-[#0a0a0f]">
      <div className="text-center max-w-lg space-y-8 px-4">
        {/* ASCII art style header */}
        <div className="space-y-2">
          <pre className="text-emerald-600 text-[10px] leading-tight">
{`
  ██╗  ██╗ █████╗ ██╗      █████╗ ███╗   ███╗
  ██║ ██╔╝██╔══██╗██║     ██╔══██╗████╗ ████║
  █████╔╝ ███████║██║     ███████║██╔████╔██║
  ██╔═██╗ ██╔══██║██║     ██╔══██║██║╚██╔╝██║
  ██║  ██╗██║  ██║███████╗██║  ██║██║ ╚═╝ ██║
  ╚═╝  ╚═╝╚═╝  ╚═╝╚══════╝╚═╝  ╚═╝╚═╝     ╚═╝
`}
          </pre>
        </div>

        <div className="space-y-3">
          <div className="flex items-center gap-2 justify-center">
            <Terminal className="h-5 w-5 text-emerald-400" />
            <h2 className="text-lg text-emerald-300">KalamDB Chat Terminal</h2>
          </div>
          <p className="text-sm text-emerald-700 leading-relaxed">
            Real-time messaging via CDC topics. Messages are processed through
            the KalamDB event pipeline with streaming response rendering.
          </p>
        </div>

        <button
          onClick={onNewConversation}
          className="inline-flex items-center gap-2 px-6 py-2.5 border border-emerald-700 rounded text-sm text-emerald-400 hover:bg-emerald-500/10 hover:border-emerald-500 transition-all group"
        >
          <span>Initialize Session</span>
          <ArrowRight className="h-4 w-4 group-hover:translate-x-1 transition-transform" />
        </button>

        <div className="grid grid-cols-3 gap-4 pt-2">
          {[
            { label: 'PROTOCOL', value: 'WebSocket' },
            { label: 'PIPELINE', value: 'CDC Topics' },
            { label: 'RENDER', value: 'Streaming' },
          ].map((item) => (
            <div key={item.label} className="text-center">
              <p className="text-[10px] text-emerald-800 tracking-wider">{item.label}</p>
              <p className="text-xs text-emerald-500 mt-0.5">{item.value}</p>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
