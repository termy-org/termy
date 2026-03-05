import { Link } from "@tanstack/react-router";
import {
  Download,
  ExternalLink,
  Menu,
  Moon,
  Palette,
  Sparkles,
  Sun,
  Tag,
  Users,
  X,
} from "lucide-react";
import { type JSX, useEffect, useState } from "react";
import { Button } from "@/components/ui/button";
import {
  NavigationMenu,
  NavigationMenuContent,
  NavigationMenuItem,
  NavigationMenuLink,
  NavigationMenuList,
  NavigationMenuTrigger,
} from "@/components/ui/navigation-menu";
import { useTheme } from "@/hooks/useTheme";

type NavLink =
  | { label: string; href: string; to?: never; external?: boolean }
  | { label: string; to: string; href?: never; external?: never };

const navLinks: NavLink[] = [
  { label: "Features", href: "/#features" },
  { label: "Download", href: "/#download" },
  { label: "Themes", to: "/themes" },
  { label: "Releases", to: "/releases" },
  { label: "Contributors", to: "/contributors" },
  { label: "Docs", to: "/docs" },
  {
    label: "GitHub",
    href: "https://github.com/lassejlv/termy",
    external: true,
  },
];

const mobileLinkClass =
  "flex items-center gap-3 rounded-lg px-3 py-2.5 text-sm font-medium text-muted-foreground transition-colors hover:bg-muted/50 hover:text-foreground";

const dropdownLinkClass =
  "flex flex-row select-none items-start gap-2.5 rounded-md px-3 py-2.5 no-underline outline-none transition-colors hover:bg-accent hover:text-accent-foreground focus:bg-accent focus:text-accent-foreground";

interface NavItemProps {
  link: NavLink;
  className: string;
  onClick?: () => void;
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

function DropdownLink({
  href,
  to,
  external,
  icon: Icon,
  label,
  description,
}: {
  href?: string;
  to?: string;
  external?: boolean;
  icon: React.ComponentType<{ className?: string }>;
  label: string;
  description: string;
}): JSX.Element {
  const content = (
    <>
      <Icon className="mt-0.5 h-4 w-4 shrink-0 text-muted-foreground" />
      <div className="flex flex-col gap-0.5">
        <span className="text-sm font-medium leading-none text-foreground">
          {label}
        </span>
        <span className="text-xs leading-snug text-muted-foreground">
          {description}
        </span>
      </div>
    </>
  );

  if (to) {
    return (
      <Link to={to} className={dropdownLinkClass}>
        {content}
      </Link>
    );
  }

  return (
    <a
      href={href}
      className={dropdownLinkClass}
      {...(external ? { target: "_blank", rel: "noreferrer" } : {})}
    >
      {content}
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
    if (!isMobileMenuOpen) return;

    function onKeyDown(event: KeyboardEvent): void {
      if (event.key === "Escape") closeMobileMenu();
    }

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [isMobileMenuOpen]);

  return (
    <>
      <header className="fixed top-0 left-0 right-0 z-50 backdrop-blur-xl bg-background/80">
        <nav className="mx-auto flex h-14 max-w-6xl items-center justify-between px-5">
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
            <NavigationMenu>
              <NavigationMenuList>
                <NavigationMenuItem>
                  <NavigationMenuTrigger className="bg-transparent text-sm text-muted-foreground/70 hover:bg-transparent hover:text-foreground data-[state=open]:bg-transparent">
                    Product
                  </NavigationMenuTrigger>
                  <NavigationMenuContent>
                    <div className="grid w-[280px] gap-0.5 p-2">
                      <DropdownLink
                        href="/#features"
                        icon={Sparkles}
                        label="Features"
                        description="What makes Termy different"
                      />
                      <DropdownLink
                        href="/#download"
                        icon={Download}
                        label="Download"
                        description="Get Termy for your platform"
                      />
                      <DropdownLink
                        to="/releases"
                        icon={Tag}
                        label="Releases"
                        description="Changelog and version history"
                      />
                    </div>
                  </NavigationMenuContent>
                </NavigationMenuItem>

                <NavigationMenuItem>
                  <NavigationMenuTrigger className="bg-transparent text-sm text-muted-foreground/70 hover:bg-transparent hover:text-foreground data-[state=open]:bg-transparent">
                    Community
                  </NavigationMenuTrigger>
                  <NavigationMenuContent>
                    <div className="grid w-[280px] gap-0.5 p-2">
                      <DropdownLink
                        to="/themes"
                        icon={Palette}
                        label="Themes"
                        description="Browse community themes"
                      />
                      <DropdownLink
                        to="/contributors"
                        icon={Users}
                        label="Contributors"
                        description="People behind Termy"
                      />
                      <DropdownLink
                        href="https://github.com/lassejlv/termy"
                        external
                        icon={ExternalLink}
                        label="GitHub"
                        description="Source code and issues"
                      />
                    </div>
                  </NavigationMenuContent>
                </NavigationMenuItem>

                <NavigationMenuItem>
                  <NavigationMenuLink asChild>
                    <Link
                      to="/docs"
                      className="inline-flex h-9 w-max items-center justify-center rounded-md px-4 py-2 text-sm text-muted-foreground/70 transition-colors hover:text-foreground"
                    >
                      Docs
                    </Link>
                  </NavigationMenuLink>
                </NavigationMenuItem>
              </NavigationMenuList>
            </NavigationMenu>
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
      </header>

      {/* Mobile dropdown menu */}
      {isMobileMenuOpen && (
        <>
          <button
            type="button"
            aria-label="Close menu"
            onClick={closeMobileMenu}
            className="fixed inset-0 z-40 md:hidden"
          />
          <div
            id="mobile-menu"
            className="fixed top-14 left-0 right-0 z-50 bg-background/95 backdrop-blur-xl px-5 py-3 shadow-lg md:hidden"
          >
            <nav className="mx-auto flex max-w-6xl flex-col gap-0.5">
              {navLinks.map((link) => (
                <NavItem
                  key={link.label}
                  link={link}
                  className={mobileLinkClass}
                  onClick={closeMobileMenu}
                />
              ))}
              <div className="my-1.5 h-px bg-border/30" />
              <button
                type="button"
                onClick={() => {
                  toggleTheme();
                  closeMobileMenu();
                }}
                className={mobileLinkClass}
              >
                {theme === "light" ? (
                  <Moon className="h-4 w-4" />
                ) : (
                  <Sun className="h-4 w-4" />
                )}
                {theme === "light" ? "Dark mode" : "Light mode"}
              </button>
            </nav>
          </div>
        </>
      )}
    </>
  );
}
