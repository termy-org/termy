import { type JSX, useEffect, useRef, useState } from "react";

const ASCII_ART = [
  "                  ..'",
  "               ,xNMM.",
  "             .OMMMMo",
  '             lMM"',
  "   .;loddo:. .olloddol;.",
  " cKMMMMMMMMMMNWMMMMMMMMMM0:",
  ".KMMMMMMMMMMMMMMMMMMMMMMMWd.",
  "XMMMMMMMMMMMMMMMMMMMMMMMX.",
  ";MMMMMMMMMMMMMMMMMMMMMMMM:",
  ":MMMMMMMMMMMMMMMMMMMMMMMM:",
  ".MMMMMMMMMMMMMMMMMMMMMMMMX.",
  " kMMMMMMMMMMMMMMMMMMMMMMMMWd.",
  " 'XMMMMMMMMMMMMMMMMMMMMMMMMMMk",
  "  'XMMMMMMMMMMMMMMMMMMMMMMMMK.",
  "    kMMMMMMMMMMMMMMMMMMMMMMd",
  '     ;KMMMMMMMWXXWMMMMMMMk.',
  '       "cooc*"    "*coo\'"',
];

const SYSTEM_INFO = [
  ["", "lassevestergaard@Mac"],
  ["", "--------------------"],
  ["OS:", "macOS Tahoe 26.2 (25C56) arm64"],
  ["Host:", "MacBook Pro (14-inch, 2024, Three Thunderbolt 4 ports)"],
  ["Kernel:", "Darwin 25.2.0"],
  ["Uptime:", "20 hours, 31 mins"],
  ["Packages:", "283 (brew), 49 (brew-cask)"],
  ["Shell:", "zsh 5.9"],
  ["Display (Color LCD):", "3024x1964 @ 2x in 14\", 120 Hz [Built-in]"],
  ["WM:", "Quartz Compositor 1.600.0"],
  ["WM Theme:", "Multicolor (Dark)"],
  ["Theme:", "Liquid Glass"],
  ["Font:", ".AppleSystemUIFont [System], Helvetica [User]"],
  ["Cursor:", "Fill - Black, Outline - White (32px)"],
  ["Terminal:", "termy"],
  ["CPU:", "Apple M4 (10) @ 4.46 GHz"],
  ["GPU:", "Apple M4 (10) @ 1.58 GHz [Integrated]"],
  ["Memory:", "16.01 GB / 24.00 GB (67%)"],
  ["Swap:", "Disabled"],
  ["Disk (/):", "166.67 GB / 460.43 GB (36%) - apfs [Read-only]"],
  ["Local IP (en0):", "192.168.8.8/24"],
  ["Battery (bq40z651):", "51% (5 hours, 6 mins remaining) [Discharging]"],
  ["Locale:", "C.UTF-8"],
];

const COLOR_BLOCKS = [
  [
    "#2e3436",
    "#cc0000",
    "#4e9a06",
    "#c4a000",
    "#3465a4",
    "#75507b",
    "#06989a",
    "#d3d7cf",
  ],
  [
    "#555753",
    "#ef2929",
    "#8ae234",
    "#fce94f",
    "#729fcf",
    "#ad7fa8",
    "#34e2e2",
    "#eeeeec",
  ],
];

interface TerminalLine {
  id: number;
  type: "prompt" | "output" | "fastfetch";
  content?: string;
  command?: string;
}

let nextLineId = 0;

const KNOWN_COMMANDS: Record<string, string[]> = {
  help: [
    "Available commands:",
    "  fastfetch  - Display system information",
    "  help       - Show this help message",
    "  clear      - Clear the terminal",
    "  whoami     - Display current user",
    "  echo       - Echo a message",
    "  neofetch   - Alias for fastfetch",
    "  ls         - List files",
    "  termy      - Show termy info",
    "  cargo      - Simulate cargo commands",
    "",
    "This is a demo terminal. Commands are simulated.",
  ],
  whoami: ["dev"],
  ls: [
    "Documents  Downloads  Desktop  Music  Pictures  Videos",
    ".config    .zshrc     .gitconfig",
  ],
  neofetch: ["fastfetch"],
  pwd: ["/home/dev"],
  date: [],
  uname: ["Darwin"],
  "termy --version": ["termy 0.3.0"],
  termy: [
    "termy - A terminal that gets out of your way",
    "",
    "USAGE:",
    "    termy [OPTIONS]",
    "",
    "OPTIONS:",
    "    --version    Print version info",
    "    --config     Path to config file",
    "    --help       Print help information",
  ],
  "cargo build": [
    "   Compiling termy v0.3.0",
    "    Finished dev [unoptimized + debuginfo] target(s) in 0.42s",
  ],
  "cargo test": [
    "   Compiling termy v0.3.0",
    "    Finished test [unoptimized + debuginfo] target(s) in 0.38s",
    "     Running unittests src/main.rs",
    "",
    "running 42 tests",
    "test config::tests::parse_config ... ok",
    "test renderer::tests::gpu_init ... ok",
    "test terminal::tests::pty_spawn ... ok",
    "...",
    "",
    "test result: ok. 42 passed; 0 failed; 0 ignored",
  ],
  cargo: [
    "Rust's package manager",
    "",
    "USAGE:",
    "    cargo [COMMAND]",
    "",
    "Try: cargo build, cargo test",
  ],
};

function getNextLineId(): number {
  const currentId = nextLineId;
  nextLineId += 1;
  return currentId;
}

function createPromptLine(command: string): TerminalLine {
  return {
    id: getNextLineId(),
    type: "prompt",
    command,
  };
}

function createOutputLine(content: string): TerminalLine {
  return {
    id: getNextLineId(),
    type: "output",
    content,
  };
}

function createFastfetchLine(): TerminalLine {
  return {
    id: getNextLineId(),
    type: "fastfetch",
  };
}

function isFastfetchCommand(command: string): boolean {
  return command === "fastfetch" || command === "neofetch";
}

function getPreviousHistoryIndex(
  historyIndex: number,
  historyLength: number,
): number {
  if (historyIndex === -1) {
    return historyLength - 1;
  }

  return Math.max(0, historyIndex - 1);
}

function getNextHistoryIndex(historyIndex: number, historyLength: number): number {
  if (historyIndex === -1) {
    return -1;
  }

  const nextIndex = historyIndex + 1;

  if (nextIndex >= historyLength) {
    return -1;
  }

  return nextIndex;
}

function getCursorClassName(isFocused: boolean): string {
  const baseClassName = "inline-block w-[8px] h-[15px] translate-y-[1px]";

  if (isFocused) {
    return `${baseClassName} bg-[#d1d5db] animate-terminal-blink`;
  }

  return `${baseClassName} bg-[#d1d5db]/40`;
}

function FastfetchOutput(): JSX.Element {
  const maxAsciiWidth = 40;

  return (
    <div className="flex gap-2">
      <div className="hidden sm:block shrink-0 text-[#4ade80]">
        {ASCII_ART.map((line, index) => (
          <div key={index} className="leading-[1.25]">
            <span className="whitespace-pre">{line.padEnd(maxAsciiWidth)}</span>
          </div>
        ))}
      </div>

      <div className="min-w-0">
        {SYSTEM_INFO.map(([label, value], index) => (
          <div key={index} className="leading-[1.25] whitespace-nowrap overflow-hidden text-ellipsis">
            {label ? (
              <>
                <span className="text-[#4ade80] font-bold">{label}</span>{" "}
                <span className="text-[#d1d5db]">{value}</span>
              </>
            ) : (
              <span className="text-[#d1d5db]">{value}</span>
            )}
          </div>
        ))}

        <div className="mt-2 flex flex-col">
          {COLOR_BLOCKS.map((row, rowIndex) => (
            <div key={rowIndex} className="flex">
              {row.map((color, colorIndex) => (
                <span
                  key={colorIndex}
                  className="inline-block w-[30px] h-[18px]"
                  style={{ backgroundColor: color }}
                />
              ))}
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

function PromptPrefix(): JSX.Element {
  return (
    <span className="whitespace-pre shrink-0">
      <span className="text-[#4ade80]">→</span>
      {"  "}
      <span className="text-[#82aaff]">dev</span>
      {" "}
    </span>
  );
}

export function InteractiveTerminal(): JSX.Element {
  const [lines, setLines] = useState<TerminalLine[]>(() => [
    createPromptLine("fastfetch"),
    createFastfetchLine(),
  ]);
  const [currentInput, setCurrentInput] = useState("");
  const [commandHistory, setCommandHistory] = useState<string[]>(["fastfetch"]);
  const [historyIndex, setHistoryIndex] = useState(-1);
  const [isFocused, setIsFocused] = useState(false);

  const inputRef = useRef<HTMLInputElement>(null);
  const terminalRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!terminalRef.current) {
      return;
    }

    terminalRef.current.scrollTop = terminalRef.current.scrollHeight;
  }, [lines]);

  function focusInput(): void {
    inputRef.current?.focus();
  }

  function clearTerminal(): void {
    setLines([]);
  }

  function processCommand(command: string): void {
    const trimmed = command.trim();

    if (trimmed === "clear") {
      clearTerminal();
      return;
    }

    setLines((previousLines) => {
      const updatedLines: TerminalLine[] = [...previousLines, createPromptLine(trimmed)];

      if (!trimmed) {
        return updatedLines;
      }

      if (isFastfetchCommand(trimmed)) {
        updatedLines.push(createFastfetchLine());
        return updatedLines;
      }

      if (trimmed.startsWith("echo ")) {
        updatedLines.push(createOutputLine(trimmed.slice(5)));
        return updatedLines;
      }

      if (trimmed === "date") {
        updatedLines.push(createOutputLine(new Date().toString()));
        return updatedLines;
      }

      const outputLines = KNOWN_COMMANDS[trimmed];
      if (!outputLines) {
        updatedLines.push(createOutputLine(`zsh: command not found: ${trimmed}`));
        return updatedLines;
      }

      if (outputLines[0] === "fastfetch") {
        updatedLines.push(createFastfetchLine());
        return updatedLines;
      }

      for (const outputLine of outputLines) {
        updatedLines.push(createOutputLine(outputLine));
      }

      return updatedLines;
    });
  }

  function submitCurrentCommand(): void {
    const command = currentInput;
    const trimmedCommand = command.trim();

    if (trimmedCommand) {
      setCommandHistory((previousHistory) => [...previousHistory, trimmedCommand]);
    }

    setHistoryIndex(-1);
    processCommand(command);
    setCurrentInput("");
  }

  function showPreviousHistoryEntry(): void {
    if (commandHistory.length === 0) {
      return;
    }

    const nextIndex = getPreviousHistoryIndex(historyIndex, commandHistory.length);
    setHistoryIndex(nextIndex);
    setCurrentInput(commandHistory[nextIndex] ?? "");
  }

  function showNextHistoryEntry(): void {
    const nextIndex = getNextHistoryIndex(historyIndex, commandHistory.length);
    setHistoryIndex(nextIndex);

    if (nextIndex === -1) {
      setCurrentInput("");
      return;
    }

    setCurrentInput(commandHistory[nextIndex] ?? "");
  }

  function handleKeyDown(event: React.KeyboardEvent<HTMLInputElement>): void {
    switch (event.key) {
      case "Enter":
        event.preventDefault();
        submitCurrentCommand();
        return;
      case "ArrowUp":
        event.preventDefault();
        showPreviousHistoryEntry();
        return;
      case "ArrowDown":
        event.preventDefault();
        showNextHistoryEntry();
        return;
      case "l":
        if (event.ctrlKey) {
          event.preventDefault();
          clearTerminal();
        }
        return;
      default:
        return;
    }
  }

  function renderLine(line: TerminalLine): JSX.Element {
    if (line.type === "fastfetch") {
      return (
        <div key={line.id} className="mb-3">
          <FastfetchOutput />
        </div>
      );
    }

    if (line.type === "prompt") {
      return (
        <div key={line.id} className="flex">
          <PromptPrefix />
          <span className="text-[#d1d5db]">{line.command}</span>
        </div>
      );
    }

    return (
      <div key={line.id} className="text-[#d1d5db]">
        {line.content || "\u00A0"}
      </div>
    );
  }

  return (
    <div
      className="terminal-body select-text cursor-text"
      ref={terminalRef}
      onClick={focusInput}
    >
      {lines.map(renderLine)}

      <div className="flex items-center">
        <PromptPrefix />
        <div className="relative flex-1">
          <input
            ref={inputRef}
            type="text"
            value={currentInput}
            onChange={(event) => setCurrentInput(event.target.value)}
            onKeyDown={handleKeyDown}
            onFocus={() => setIsFocused(true)}
            onBlur={() => setIsFocused(false)}
            className="terminal-input"
            spellCheck={false}
            autoComplete="off"
            autoCapitalize="off"
            autoFocus
          />

          <span
            className="absolute top-0 left-0 pointer-events-none text-transparent"
            style={{ fontSize: "inherit", lineHeight: "inherit" }}
          >
            {currentInput}
            <span className={getCursorClassName(isFocused)} />
          </span>
        </div>
      </div>
    </div>
  );
}
