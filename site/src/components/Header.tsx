import { Link } from "@tanstack/react-router";
import { Menu, Moon, Sun, X } from "lucide-react";
import { type JSX, useEffect, useState } from "react";
import { Button } from "@/components/ui/button";
import { useTheme } from "@/hooks/useTheme";

type NavLink =
  | { label: string; href: string; to?: never; external?: boolean }
  | { label: string; to: string; href?: never; external?: never };

const navLinks: NavLink[] = [
  { label: "Features", href: "/#features" },
  { label: "Download", href: "/#download" },
  { label: "Themes", to: "/themes" },
  { label: "Releases", to: "/releases" },
  { label: "Docs", to: "/docs" },
  {
    label: "GitHub",
    href: "https://github.com/lassejlv/termy",
    external: true,
  },
];

const linkClass =
  "px-3 py-1.5 text-sm text-muted-foreground/70 transition-colors hover:text-foreground";
const mobileLinkClass =
  "px-3 py-2 text-sm text-muted-foreground/70 transition-colors hover:text-foreground";

interface NavItemProps {
  link: NavLink;
  className: string;
  onClick?: () => void;
}

function getMobileOverlayClassName(isMobileMenuOpen: boolean): string {
  const baseClassName =
    "fixed inset-0 top-14 z-40 bg-black/20 transition-opacity duration-200 md:hidden";

  if (isMobileMenuOpen) {
    return `${baseClassName} opacity-100`;
  }

  return `${baseClassName} pointer-events-none opacity-0`;
}

function getMobileMenuClassName(isMobileMenuOpen: boolean): string {
  const baseClassName =
    "absolute left-0 right-0 top-14 z-50 border-t border-border/30 bg-background/95 px-6 py-4 backdrop-blur-xl transition-all duration-200 md:hidden";

  if (isMobileMenuOpen) {
    return `${baseClassName} translate-y-0 opacity-100`;
  }

  return `${baseClassName} pointer-events-none -translate-y-2 opacity-0`;
}

function NavItem({ link, className, onClick }: NavItemProps): JSX.Element {
  if (link.to) {
    return (
      <Link to={link.to} className={className} onClick={onClick}>
        {link.label}
      </Link>
    );
  }

  return (
    <a
      href={link.href}
      className={className}
      onClick={onClick}
      {...(link.external ? { target: "_blank", rel: "noreferrer" } : {})}
    >
      {link.label}
    </a>
  );
}

export function Header(): JSX.Element {
  const { theme, toggleTheme } = useTheme();
  const [isMobileMenuOpen, setIsMobileMenuOpen] = useState(false);

  function closeMobileMenu(): void {
    setIsMobileMenuOpen(false);
  }

  function toggleMobileMenu(): void {
    setIsMobileMenuOpen((open) => !open);
  }

  useEffect(() => {
    if (!isMobileMenuOpen) {
      return;
    }

    function onKeyDown(event: KeyboardEvent): void {
      if (event.key === "Escape") {
        closeMobileMenu();
      }
    }

    window.addEventListener("keydown", onKeyDown);

    return () => {
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [isMobileMenuOpen]);

  return (
    <header className="fixed top-0 left-0 right-0 z-50 backdrop-blur-xl bg-background/80 border-b border-border/30">
      <nav className="mx-auto flex h-14 max-w-6xl items-center justify-between px-6">
        <Link
          to="/"
          onClick={closeMobileMenu}
          className="flex items-center gap-2.5 text-foreground transition-colors hover:text-primary"
        >
          <img
            src="/termy_icon.png"
            alt="Termy"
            width={32}
            height={32}
            className="rounded-lg"
          />
          <span className="text-sm font-medium tracking-tight">Termy</span>
        </Link>

        <div className="hidden items-center gap-0.5 md:flex">
          {navLinks.map((link) => (
            <NavItem key={link.label} link={link} className={linkClass} />
          ))}
          <div className="w-px h-4 bg-border/50 mx-2" />
          <Button
            variant="ghost"
            size="sm"
            onClick={toggleTheme}
            className="text-muted-foreground/60 hover:text-foreground"
          >
            {theme === "light" ? (
              <Moon className="w-4 h-4" />
            ) : (
              <Sun className="w-4 h-4" />
            )}
          </Button>
        </div>

        <Button
          type="button"
          variant="ghost"
          size="icon"
          onClick={toggleMobileMenu}
          aria-label={isMobileMenuOpen ? "Close menu" : "Open menu"}
          aria-expanded={isMobileMenuOpen}
          aria-controls="mobile-menu"
          className="text-muted-foreground/60 hover:text-foreground md:hidden"
        >
          {isMobileMenuOpen ? (
            <X className="h-5 w-5" />
          ) : (
            <Menu className="h-5 w-5" />
          )}
        </Button>
      </nav>

      <button
        type="button"
        aria-label="Close menu"
        onClick={closeMobileMenu}
        className={getMobileOverlayClassName(isMobileMenuOpen)}
      />

      <div
        id="mobile-menu"
        aria-hidden={!isMobileMenuOpen}
        className={getMobileMenuClassName(isMobileMenuOpen)}
      >
        <div className="flex flex-col gap-1">
          {navLinks.map((link) => (
            <NavItem
              key={link.label}
              link={link}
              className={mobileLinkClass}
              onClick={closeMobileMenu}
            />
          ))}
          <div className="my-2 h-px bg-border/30" />
          <Button
            type="button"
            variant="ghost"
            size="sm"
            onClick={toggleTheme}
            className="w-fit text-muted-foreground/60 hover:text-foreground"
          >
            {theme === "light" ? "Dark mode" : "Light mode"}
          </Button>
        </div>
      </div>
    </header>
  );
}
