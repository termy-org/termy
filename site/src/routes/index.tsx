import { createFileRoute } from "@tanstack/react-router";
import { useGitHubRelease } from "@/hooks/useGitHubRelease";
import { Features } from "@/components/Features";
import { Download } from "@/components/Download";
import { Github } from "lucide-react";

export const Route = createFileRoute("/")({
  component: HomePage,
});

function HomePage() {
  const { release, loading, error } = useGitHubRelease();

  return (
    <div className="flex flex-col items-center">
      {/* Hero */}
      <section className="flex flex-col items-center text-center pt-24 pb-32 gap-8">
        <h1 className="text-6xl sm:text-7xl font-bold tracking-tight">
          Termy
        </h1>

        <p className="text-lg sm:text-xl text-neutral-400 max-w-lg">
          A fast, GPU-accelerated terminal emulator. Built with Rust. Open
          source.
        </p>

        <a
          href="https://github.com/lassejlv/termy"
          target="_blank"
          rel="noopener noreferrer"
          className="inline-flex items-center gap-2 px-6 py-3 border border-neutral-700 text-neutral-300 font-medium rounded-lg hover:border-neutral-500 hover:text-white transition-colors"
        >
          <Github size={18} />
          View on GitHub
        </a>
      </section>

      {/* Features */}
      <Features />

      {/* Download */}
      <Download release={release} loading={loading} error={error} />
    </div>
  );
}

