import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";
import fs from "node:fs";
import path from "node:path";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

function explainExit(code) {
  if (code === 0 || code === null) {
    return null;
  }
  const u = code >>> 0;
  const hex = "0x" + u.toString(16).toUpperCase();
  if (u === 0xc0000005) {
    return `${hex} STATUS_ACCESS_VIOLATION — the process crashed. Lines starting with "Ftracker:" above are the last steps that ran.`;
  }
  if (u === 0xc0000135) {
    return `${hex} missing DLL`;
  }
  if (u === 0xc0000409) {
    return `${hex} stack buffer overrun`;
  }
  if (code === 1) {
    return "exit 1 — see \"Ftracker error:\" above (config, enroll, or a Rust Err).";
  }
  return `exit ${code} (${hex})`;
}

export class Dev {
  constructor() {
    this.root = root;
  }

  /** pnpm warns if this folder is missing. We do not install npm packages. */
  ensurePnpmStub() {
    const dir = path.join(this.root, "node_modules");
    try {
      fs.mkdirSync(dir, { recursive: true });
    } catch {
      /* ignore */
    }
  }

  runCommand(command, args, extraEnv = {}) {
    return new Promise((resolve, reject) => {
      const child = spawn(command, args, {
        cwd: this.root,
        stdio: "inherit",
        shell: true,
        env: { ...process.env, RUST_BACKTRACE: "1", ...extraEnv },
      });
      child.on("exit", (code) => {
        if (code === 0 || code === null) resolve();
        else reject(Object.assign(new Error(`${command} ${args.join(" ")} exited ${code}`), { code }));
      });
    });
  }

  async run() {
    this.ensurePnpmStub();
    // Unlock ftracker.exe before cargo so this build actually replaces the binary.
    if (process.platform === "win32") {
      await new Promise((resolve) => {
        const child = spawn("taskkill", ["/IM", "ftracker.exe", "/F"], {
          cwd: this.root,
          stdio: "ignore",
          shell: true,
        });
        child.on("exit", () => resolve());
        child.on("error", () => resolve());
      });
    }
    await this.runCommand("cargo", [
      "build",
      "-p",
      "ftracker-session",
      "-p",
      "ftracker-service",
    ]);
    const exe = path.join(
      this.root,
      "target",
      "debug",
      process.platform === "win32" ? "ftracker.exe" : "ftracker"
    );
    try {
      await this.runCommand(exe, []);
    } catch (err) {
      const why = explainExit(err.code);
      console.error(err.message || err);
      if (why) {
        console.error(why);
      }
      if (err.code === 1) {
        console.error(
          "If this was the first run: add TELEGRAM_BOT_TOKEN and TELEGRAM_ADMIN_CHAT_ID to .env, scan the TOTP QR, then pnpm dev again."
        );
      }
    }
  }
}

const isMain = process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url);
if (isMain) {
  new Dev().run().catch((err) => {
    console.error(err.message || err);
    process.exit(1);
  });
}
