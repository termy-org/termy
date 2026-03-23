import { Link } from "@tanstack/react-router";
import { motion } from "framer-motion";
import {
  Download,
  ExternalLink,
  Heart,
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
import { GitHubStarsButton } from "./animate-ui/components/buttons/github-stars";

function DiscordIcon({ className }: { className?: string }): JSX.Element {
  return (
    <svg
      className={className}
      viewBox="0 0 24 24"
      fill="currentColor"
      xmlns="http://www.w3.org/2000/svg"
    >
      <path d="M20.317 4.37a19.791 19.791 0 0 0-4.885-1.515.074.074 0 0 0-.079.037c-.21.375-.444.864-.608 1.25a18.27 18.27 0 0 0-5.487 0 12.64 12.64 0 0 0-.617-1.25.077.077 0 0 0-.079-.037A19.736 19.736 0 0 0 3.677 4.37a.07.07 0 0 0-.032.027C.533 9.046-.32 13.58.099 18.057a.082.082 0 0 0 .031.057 19.9 19.9 0 0 0 5.993 3.03.078.078 0 0 0 .084-.028 14.09 14.09 0 0 0 1.226-1.994.076.076 0 0 0-.041-.106 13.107 13.107 0 0 1-1.872-.892.077.077 0 0 1-.008-.128 10.2 10.2 0 0 0 .372-.292.074.074 0 0 1 .077-.01c3.928 1.793 8.18 1.793 12.062 0a.074.074 0 0 1 .078.01c.12.098.246.198.373.292a.077.077 0 0 1-.006.127 12.299 12.299 0 0 1-1.873.892.077.077 0 0 0-.041.107c.36.698.772 1.362 1.225 1.993a.076.076 0 0 0 .084.028 19.839 19.839 0 0 0 6.002-3.03.077.077 0 0 0 .032-.054c.5-5.177-.838-9.674-3.549-13.66a.061.061 0 0 0-.031-.03zM8.02 15.33c-1.183 0-2.157-1.085-2.157-2.419 0-1.333.956-2.419 2.157-2.419 1.21 0 2.176 1.096 2.157 2.42 0 1.333-.956 2.418-2.157 2.418zm7.975 0c-1.183 0-2.157-1.085-2.157-2.419 0-1.333.955-2.419 2.157-2.419 1.21 0 2.176 1.096 2.157 2.42 0 1.333-.946 2.418-2.157 2.418z" />
    </svg>
  );
}

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
    label: "Discord",
    href: "https://discord.gg/4VDBFD7vAp",
    external: true,
  },
  {
    label: "GitHub",
    href: "https://github.com/lassejlv/termy",
    external: true,
  },
  {
    label: "Sponsor",
    href: "https://github.com/sponsors/lassejlv",
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
                        href="https://discord.gg/4VDBFD7vAp"
                        external
                        icon={DiscordIcon}
                        label="Discord"
                        description="Join our community server"
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
                <NavigationMenuItem>
                  <motion.a
                    href="https://github.com/sponsors/lassejlv"
                    target="_blank"
                    rel="noreferrer"
                    className="inline-flex h-8 shrink-0 items-center justify-center gap-1.5 whitespace-nowrap rounded-md border bg-background px-3 text-sm font-medium shadow-xs outline-none transition-[box-shadow,_color,_background-color,_border-color,_outline-color,_text-decoration-color,_fill,_stroke] hover:bg-accent hover:text-accent-foreground focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/50 disabled:pointer-events-none disabled:opacity-50 dark:border-input dark:bg-input/30 dark:hover:bg-input/50 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4"
                    initial="rest"
                    whileHover="hover"
                    whileTap="hover"
                  >
                    <span className="relative inline-flex items-center justify-center">
                      <motion.span
                        animate={{ scale: [1, 1.16, 1], opacity: [1, 0.82, 1] }}
                        transition={{
                          duration: 1.4,
                          repeat: Number.POSITIVE_INFINITY,
                          ease: "easeInOut",
                        }}
                        className="inline-flex"
                      >
                        <Heart className="size-4 text-[#db61a2]" />
                      </motion.span>
                      <motion.span
                        variants={{
                          rest: { opacity: 0, scale: 0.7, x: 0, y: 0 },
                          hover: {
                            x: [0, 8, 12],
                            y: [0, -7, -10],
                            opacity: [0, 1, 0],
                            scale: [0.7, 1, 0.8],
                            transition: {
                              duration: 0.9,
                              ease: "easeOut",
                              repeat: Number.POSITIVE_INFINITY,
                              repeatDelay: 0.15,
                            },
                          },
                        }}
                        className="absolute -right-1 -top-1 pointer-events-none"
                      >
                        <Sparkles className="size-2.5 text-[#db61a2]" />
                      </motion.span>
                      <motion.span
                        variants={{
                          rest: { opacity: 0, scale: 0.7, x: 0, y: 0 },
                          hover: {
                            x: [0, -7, -10],
                            y: [0, -6, -9],
                            opacity: [0, 1, 0],
                            scale: [0.7, 1, 0.8],
                            transition: {
                              duration: 0.85,
                              ease: "easeOut",
                              repeat: Number.POSITIVE_INFINITY,
                              repeatDelay: 0.2,
                              delay: 0.08,
                            },
                          },
                        }}
                        className="absolute -left-1 -top-1 pointer-events-none"
                      >
                        <Sparkles className="size-2 text-[#db61a2]" />
                      </motion.span>
                    </span>
                    Sponsor
                  </motion.a>
                </NavigationMenuItem>
                <NavigationMenuItem className="ml-1">
                  <GitHubStarsButton
                    variant="outline"
                    size="sm"
                    username="lassejlv"
                    repo="termy"
                    onClick={() =>
                      window.location.replace(
                        "https://github.com/lassejlv/termy",
                      )
                    }
                  />
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
