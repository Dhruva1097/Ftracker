import { spawn } from "node:child_process";
import { createInterface } from "node:readline/promises";
import { stdin as input, stdout as output } from "node:process";
import { fileURLToPath } from "node:url";
import fs from "node:fs/promises";
import path from "node:path";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

export class Build {
  constructor() {
    this.root = root;
    this.rl = null;
  }

  async prompt(question) {
    if (!this.rl) this.rl = createInterface({ input, output });
    const answer = await this.rl.question(question);
    return answer.trim();
  }

  runCommand(command, args, extraEnv = {}) {
    return new Promise((resolve, reject) => {
      const child = spawn(command, args, {
        cwd: this.root,
        stdio: "inherit",
        shell: true,
        env: { ...process.env, ...extraEnv },
      });
      child.on("exit", (code) => {
        if (code === 0) resolve();
        else reject(new Error(`${command} ${args.join(" ")} exited ${code}`));
      });
    });
  }

  rustflags() {
    return "-C target-feature=+crt-static";
  }

  async cargoRelease(target) {
    await this.runCommand(
      "cargo",
      ["build", "--release", "--target", target, "-p", "ftracker-session", "-p", "ftracker-service"],
      { RUSTFLAGS: this.rustflags() }
    );
  }

  async collectPreassign() {
    const employeeName = await this.prompt("Employee name: ");
    const dailyHours = await this.prompt("Daily hours required (e.g. 8): ");
    const botToken = await this.prompt("Telegram bot token: ");
    const adminChatId = await this.prompt("Admin Telegram chat id: ");
    if (!employeeName || !dailyHours || !botToken || !adminChatId) {
      throw new Error("All pre-assigned fields are required.");
    }
    const payload = {
      employee_name: employeeName,
      daily_hours_required: Number(dailyHours),
      telegram_bot_token: botToken,
      telegram_admin_chat_id: adminChatId,
    };
    const out = path.join(this.root, "packaging", "msi", "preassign.json");
    await fs.mkdir(path.dirname(out), { recursive: true });
    await fs.writeFile(out, JSON.stringify(payload, null, 2), "utf8");
    console.log("Wrote packaging/msi/preassign.json (gitignored). First run on the PC will DPAPI-wrap and delete it.");
  }

  async tryWix() {
    const wixDir = path.join(this.root, "packaging", "msi");
    try {
      await fs.access(path.join(wixDir, "wix", "Product.wxs"));
    } catch {
      return;
    }
    try {
      await this.runCommand("cargo", ["wix", "--package", "ftracker-session", "--nocapture"]);
    } catch {
      console.warn("WiX / cargo-wix not available. Release binaries are in target/. Run WiX later with packaging/msi/wix.");
    }
  }

  async trySign() {
    const pfx = process.env.SIGN_PFX;
    if (!pfx) {
      console.warn("SIGN_PFX not set; skipping Authenticode. Set SIGN_PFX and SIGN_PASSWORD to sign.");
      return;
    }
    const script = path.join(this.root, "packaging", "msi", "sign.ps1");
    await this.runCommand("powershell", ["-File", script]);
  }

  async run() {
    try {
      const mode = (await this.prompt("Installer mode [interactive/preassigned] (default interactive): ")).toLowerCase();
      const preassigned = mode === "preassigned" || mode === "pre-assigned" || mode === "p";
      if (preassigned) {
        await this.collectPreassign();
      } else {
        console.log("Interactive MSI: employee name, hours, token, chat id, and TOTP are collected at install time.");
        const payload = path.join(this.root, "packaging", "msi", "preassign.json");
        await fs.rm(payload, { force: true });
      }
      await this.cargoRelease("x86_64-pc-windows-msvc");
      try {
        await this.cargoRelease("i686-pc-windows-msvc");
      } catch (err) {
        console.warn("x86 build skipped or failed (install i686-pc-windows-msvc target):", err.message);
      }
      await this.tryWix();
      await this.trySign();
      console.log("Build finished.");
    } finally {
      if (this.rl) this.rl.close();
    }
  }
}

const isMain = process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url);
if (isMain) {
  new Build().run().catch((err) => {
    console.error(err.message || err);
    process.exit(1);
  });
}
