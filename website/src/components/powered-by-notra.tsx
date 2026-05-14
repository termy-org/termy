const NOTRA_URL =
  'https://usenotra.com?utm_source=termy&utm_medium=referral&utm_campaign=powered_by';

export function PoweredByNotra() {
  return (
    <div className="mx-auto flex w-full max-w-3xl items-center justify-center px-6 pb-16">
      <a
        href={NOTRA_URL}
        target="_blank"
        rel="noreferrer"
        className="group inline-flex items-center gap-2 rounded-full border border-fd-border bg-fd-card px-4 py-2 text-xs text-fd-muted-foreground transition-colors hover:border-fd-primary/40 hover:text-fd-foreground"
      >
        <span>Powered by</span>
        <NotraLogo className="size-4 transition-transform group-hover:scale-110" />
        <span className="font-medium">Notra</span>
      </a>
    </div>
  );
}

function NotraLogo({ className }: { className?: string }) {
  return (
    <svg
      viewBox="0 0 800 800"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
      className={className}
      aria-hidden
    >
      <path
        d="M572.881 462.223c-12.712 43.22-290.678 105.932-394.068 83.898l-48.305-10.169 48.305-78.814 68.644-104.237 73.729-106.78 251.695-127.119 78.814-22.881 17.796 17.796h10.17c17.796 35.593 3.945 147.458-12.712 195.763-25.424 73.729-124.576 96.61-177.966 114.407-4.064 1.355 96.61-5.085 83.898 38.136Z"
        fill="#c8b2ee"
        stroke="#1e1e1e"
        strokeWidth="35"
        strokeLinecap="round"
      />
      <path
        d="M700 96.111c-162.712-4.237-510.508 111.356-600 607.627"
        stroke="#1e1e1e"
        strokeWidth="75"
        strokeLinecap="round"
      />
    </svg>
  );
}
