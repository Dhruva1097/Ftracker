import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";
import fs from "node:fs";
import path from "node:path";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

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

  runCommand(command, args) {
    return new Promise((resolve, reject) => {
      const child = spawn(command, args, {
        cwd: this.root,
        stdio: "inherit",
        shell: true,
      });
      child.on("exit", (code) => {
        if (code === 0 || code === null) resolve();
        else reject(new Error(`${command} ${args.join(" ")} exited ${code}`));
      });
    });
  }

  async run() {
    this.ensurePnpmStub();
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
      console.error(err.message || err);
      console.error(
        "If this was the first run: add TELEGRAM_BOT_TOKEN and TELEGRAM_ADMIN_CHAT_ID to .env, scan the TOTP QR, then pnpm dev again."
      );
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
