import { createRootRoute, Link, Outlet } from "@tanstack/react-router";
import type { JSX } from "react";
import { Header } from "@/components/Header";

export const Route = createRootRoute({
  component: RootLayout,
});

function RootLayout(): JSX.Element {
  return (
    <div className="min-h-screen bg-background">
      <Header />
      <main className="mx-auto w-full max-w-6xl px-5">
        <Outlet />
      </main>

      {/* Footer */}
      <footer className="border-t border-border/30 mt-24">
        <div className="mx-auto max-w-6xl px-5 py-6">
          <div className="flex flex-col md:flex-row items-center justify-between gap-4">
            <div className="flex items-center gap-2 text-xs text-muted-foreground/50">
              <img
                src="/termy_icon.png"
                alt="Termy"
                width={16}
                height={16}
                className="rounded-sm opacity-50"
              />
              <span className="font-medium">Termy</span>
              <span className="text-muted-foreground/20">/</span>
              <span>Open source terminal emulator</span>
            </div>
            <div className="flex items-center gap-5 text-xs text-muted-foreground/40">
              <Link
                to="/docs"
                className="hover:text-foreground transition-colors"
              >
                Docs
              </Link>
              <a
                href="https://discord.gg/4VDBFD7vAp"
                target="_blank"
                rel="noreferrer"
                className="hover:text-foreground transition-colors"
              >
                Discord
              </a>
              <a
                href="https://github.com/lassejlv/termy"
                target="_blank"
                rel="noreferrer"
                className="hover:text-foreground transition-colors"
              >
                GitHub
              </a>
              <a
                href="https://github.com/lassejlv/termy/releases"
                target="_blank"
                rel="noreferrer"
                className="hover:text-foreground transition-colors"
              >
                Releases
              </a>
              <a
                href="https://github.com/lassejlv/termy/issues"
                target="_blank"
                rel="noreferrer"
                className="hover:text-foreground transition-colors"
              >
                Issues
              </a>
            </div>
          </div>
        </div>
      </footer>
    </div>
  );
}
