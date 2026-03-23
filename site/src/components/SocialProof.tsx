import type { JSX } from "react";

interface Stat {
  value: string;
  label: string;
  suffix?: string;
}

const stats: Stat[] = [
  { value: "8", label: "Startup time", suffix: "ms" },
  { value: "18", label: "Memory usage", suffix: "MB" },
  { value: "100", label: "Built with Rust", suffix: "%" },
];

export function SocialProof(): JSX.Element {
  return (
    <section className="py-16 border-y border-border/30">
      <div className="grid grid-cols-3 gap-8 max-w-2xl mx-auto">
        {stats.map((stat) => (
          <div
            key={stat.label}
            className="text-center animate-blur-in"
            style={{ animationDelay: "100ms" }}
          >
            <div className="text-3xl md:text-4xl font-bold text-primary">
              {stat.value}
              {stat.suffix && (
                <span className="text-lg md:text-xl text-primary/70">
                  {stat.suffix}
                </span>
              )}
            </div>
            <div className="mt-1 text-xs md:text-sm text-muted-foreground">
              {stat.label}
            </div>
          </div>
        ))}
      </div>
      <div className="mt-8 flex flex-wrap items-center justify-center gap-6 text-xs text-muted-foreground/50">
        <div className="flex items-center gap-2">
          <svg className="w-4 h-4" viewBox="0 0 24 24" fill="currentColor">
            <path d="M12 2C6.477 2 2 6.477 2 12c0 4.42 2.87 8.17 6.84 9.5.5.08.66-.23.66-.5v-1.69c-2.77.6-3.36-1.34-3.36-1.34-.46-1.16-1.11-1.47-1.11-1.47-.91-.62.07-.6.07-.6 1 .07 1.53 1.03 1.53 1.03.87 1.52 2.34 1.07 2.91.83.09-.65.35-1.09.63-1.34-2.22-.25-4.55-1.11-4.55-4.92 0-1.11.38-2 1.03-2.71-.1-.25-.45-1.29.1-2.64 0 0 .84-.27 2.75 1.02.79-.22 1.65-.33 2.5-.33.85 0 1.71.11 2.5.33 1.91-1.29 2.75-1.02 2.75-1.02.55 1.35.2 2.39.1 2.64.65.71 1.03 1.6 1.03 2.71 0 3.82-2.34 4.66-4.57 4.91.36.31.69.92.69 1.85V21c0 .27.16.59.67.5C19.14 20.16 22 16.42 22 12A10 10 0 0012 2z" />
          </svg>
          <span>Open Source</span>
        </div>
        <div className="flex items-center gap-2">
          <svg className="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <path strokeLinecap="round" strokeLinejoin="round" d="M9 12.75L11.25 15 15 9.75M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
          </svg>
          <span>GPU Accelerated</span>
        </div>
        <div className="flex items-center gap-2">
          <svg className="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <path strokeLinecap="round" strokeLinejoin="round" d="M9 12.75L11.25 15 15 9.75M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
          </svg>
          <span>Cross Platform</span>
        </div>
      </div>
    </section>
  );
}
