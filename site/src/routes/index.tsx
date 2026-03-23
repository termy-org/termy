import { createFileRoute, Link } from "@tanstack/react-router";
import { useGitHubRelease } from "@/hooks/useGitHubRelease";
import { Features } from "@/components/Features";
import { Download } from "@/components/Download";
import { BookOpen, Github } from "lucide-react";

export const Route = createFileRoute("/")({
  component: HomePage,
});

function HomePage() {
  const { release, loading, error } = useGitHubRelease();

  return (
    <div className="flex flex-col items-center">
      {/* Hero */}
      <section className="w-full flex flex-col items-center text-center pt-48 pb-32">
        {/* Main heading */}
        <h1 className="text-6xl sm:text-7xl font-bold tracking-tight mb-6">
          Termy
        </h1>

        {/* Tagline */}
        <p className="text-lg sm:text-xl text-muted-foreground max-w-xl mb-10">
          A fast, GPU-accelerated terminal emulator built with Rust.
          Minimal, modern, and blazingly fast.
        </p>

        {/* CTAs */}
        <div className="flex flex-col sm:flex-row items-center gap-4">
          <Link
            to="/docs"
            className="inline-flex items-center gap-2 px-6 py-3 bg-primary text-primary-foreground font-medium rounded-lg hover:bg-primary/90 transition-colors"
          >
            <BookOpen className="w-4 h-4" />
            Get Started
          </Link>
          <a
            href="https://github.com/termy-org/termy"
            target="_blank"
            rel="noopener noreferrer"
            className="inline-flex items-center gap-2 px-6 py-3 border border-border/50 text-foreground font-medium rounded-lg hover:bg-secondary/50 transition-colors"
          >
            <Github className="w-4 h-4" />
            View on GitHub
          </a>
        </div>
      </section>

      {/* Features */}
      <Features />

      {/* Download */}
      <Download release={release} loading={loading} error={error} />
    </div>
  );
}
