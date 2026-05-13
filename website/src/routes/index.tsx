import { createFileRoute, Link } from '@tanstack/react-router';
import { HomeLayout } from 'fumadocs-ui/layouts/home';
import {
  Zap,
  Settings2,
  Cpu,
  Palette,
  Feather,
  LayoutGrid,
} from 'lucide-react';
import { baseOptions } from '@/lib/layout.shared';

export const Route = createFileRoute('/')({
  component: Home,
});

const features = [
  {
    icon: Zap,
    title: 'Fast',
    description: 'GPU-accelerated rendering with instant startup.',
  },
  {
    icon: Settings2,
    title: 'Configurable',
    description: 'One TOML file. Full control over everything.',
  },
  {
    icon: Cpu,
    title: 'Native',
    description: 'Runs natively on macOS, Windows, and Linux.',
  },
  {
    icon: Palette,
    title: 'Themable',
    description: 'Built-in themes or create your own.',
  },
  {
    icon: Feather,
    title: 'Lightweight',
    description: '18MB memory footprint. No Electron bloat.',
  },
  {
    icon: LayoutGrid,
    title: 'Powerful',
    description: 'Splits, tabs, and multiplexing built-in.',
  },
];

function Home() {
  return (
    <HomeLayout {...baseOptions()}>
      <main className="flex flex-1 flex-col">
        <section className="mx-auto flex w-full max-w-5xl flex-col items-center px-6 pt-24 pb-16 text-center md:pt-32">
          <h1 className="text-balance font-medium text-4xl tracking-tight md:text-6xl">
            A fast, native terminal.
          </h1>
          <p className="mt-5 max-w-xl text-balance text-fd-muted-foreground md:text-lg">
            Termy is a modern terminal emulator that gets out of your way.
            GPU-accelerated, configurable, and lightweight.
          </p>
          <div className="mt-8 flex flex-wrap items-center justify-center gap-3">
            <Link
              to="/docs/$"
              params={{ _splat: '' }}
              className="rounded-md bg-fd-primary px-4 py-2 text-sm font-medium text-fd-primary-foreground transition-opacity hover:opacity-90"
            >
              Read the docs
            </Link>
            <a
              href="https://github.com/lassejlv/termy"
              target="_blank"
              rel="noreferrer"
              className="rounded-md border border-fd-border bg-fd-card px-4 py-2 text-sm font-medium text-fd-foreground transition-colors hover:bg-fd-accent"
            >
              GitHub
            </a>
          </div>
        </section>

        <section className="mx-auto w-full max-w-5xl px-6 pb-24">
          <div className="grid grid-cols-1 gap-px overflow-hidden rounded-xl border border-fd-border bg-fd-border sm:grid-cols-2 lg:grid-cols-3">
            {features.map((f) => (
              <div
                key={f.title}
                className="flex flex-col gap-3 bg-fd-background p-6"
              >
                <f.icon className="size-5 text-fd-primary" strokeWidth={1.75} />
                <h3 className="font-medium text-fd-foreground">{f.title}</h3>
                <p className="text-sm leading-relaxed text-fd-muted-foreground">
                  {f.description}
                </p>
              </div>
            ))}
          </div>
        </section>
      </main>
    </HomeLayout>
  );
}
