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

  /** `.env` holds only TELEGRAM_BOT_TOKEN and TELEGRAM_ADMIN_CHAT_ID. Never log the values. */
  async readDotenv() {
    const out = { TELEGRAM_BOT_TOKEN: "", TELEGRAM_ADMIN_CHAT_ID: "" };
    let text = "";
    try {
      text = await fs.readFile(path.join(this.root, ".env"), "utf8");
    } catch {
      return out;
    }
    for (const raw of text.split(/\r?\n/)) {
      const line = raw.trim();
      if (!line || line.startsWith("#")) continue;
      const eq = line.indexOf("=");
      if (eq < 1) continue;
      const k = line.slice(0, eq).trim();
      if (k !== "TELEGRAM_BOT_TOKEN" && k !== "TELEGRAM_ADMIN_CHAT_ID") continue;
      let v = line.slice(eq + 1).trim();
      if (
        (v.startsWith('"') && v.endsWith('"')) ||
        (v.startsWith("'") && v.endsWith("'"))
      ) {
        v = v.slice(1, -1);
      }
      out[k] = v;
    }
    return out;
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
    const env = await this.readDotenv();
    const botToken = env.TELEGRAM_BOT_TOKEN;
    if (!botToken) {
      throw new Error("TELEGRAM_BOT_TOKEN missing in .env");
    }
    console.log("Telegram bot token: from .env");
    const employeeName = await this.prompt("Employee name: ");
    const dailyHours = await this.prompt("Daily hours required (e.g. 8): ");
    const adminChatId =
      env.TELEGRAM_ADMIN_CHAT_ID || (await this.prompt("Admin Telegram chat id: "));
    if (env.TELEGRAM_ADMIN_CHAT_ID) {
      console.log("Admin Telegram chat id: from .env");
    }
    if (!employeeName || !dailyHours || !botToken || !adminChatId) {
      throw new Error("Employee name and daily hours are required.");
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

  async fileExists(p) {
    try {
      await fs.access(p);
      return true;
    } catch {
      return false;
    }
  }

  async findWixBin() {
    const fromEnv = process.env.WIX ? path.join(process.env.WIX, "bin") : "";
    const candidates = [
      fromEnv,
      "C:\\Program Files (x86)\\WiX Toolset v3.14\\bin",
      "C:\\Program Files (x86)\\WiX Toolset v3.11\\bin",
      "C:\\Program Files\\WiX Toolset v3.14\\bin",
      "C:\\Program Files\\WiX Toolset v3.11\\bin",
    ].filter(Boolean);
    for (const dir of candidates) {
      if (await this.fileExists(path.join(dir, "candle.exe"))) {
        return dir;
      }
    }
    return "";
  }

  async tryWix(preassigned) {
    const wxsDir = path.join(this.root, "packaging", "msi", "wix");
    if (!(await this.fileExists(path.join(wxsDir, "Product.wxs")))) {
      return;
    }
    const wixBin = await this.findWixBin();
    if (!wixBin) {
      console.warn(
        "WiX Toolset 3.11 (candle.exe) not found. x64 exe is in target/x86_64-pc-windows-msvc/release/. Install WiX 3.11 and run pnpm build again for the MSI."
      );
      return;
    }
    const candle = path.join(wixBin, "candle.exe");
    const light = path.join(wixBin, "light.exe");
    const outDir = path.join(this.root, "target", "wix");
    await fs.mkdir(outDir, { recursive: true });
    const wxs = [
      "Product.wxs",
      "Files.wxs",
      "Service.wxs",
      "Tasks.wxs",
      "ExtensionPolicies.wxs",
      "UI.wxs",
    ];
    const candleArgs = [
      "-nologo",
      "-arch",
      "x64",
      "-dPlatform=x64",
      "-ext",
      "WixUtilExtension",
      "-out",
      `${outDir}\\`,
    ];
    if (preassigned) {
      candleArgs.push("-dPreassigned=1");
    }
    for (const f of wxs) {
      candleArgs.push(path.join(wxsDir, f));
    }
    await this.runCommand(`"${candle}"`, candleArgs);
    const msi = path.join(outDir, "Ftracker-1.0.0-x64.msi");
    const lightArgs = [
      "-nologo",
      "-ext",
      "WixUtilExtension",
      "-spdb",
      "-out",
      msi,
      `-b`,
      `bin=${path.join(this.root, "target", "x86_64-pc-windows-msvc", "release")}`,
      `-b`,
      `res=${path.join(this.root, "packaging", "res")}`,
      `-b`,
      `ext=${path.join(this.root, "extension", "ftracker-upload-gate")}`,
      `-b`,
      `nmh=${path.join(this.root, "packaging", "native-messaging")}`,
      `-b`,
      `tasks=${path.join(this.root, "packaging", "tasks")}`,
      `-b`,
      `pre=${path.join(this.root, "packaging", "msi")}`,
    ];
    for (const f of wxs) {
      lightArgs.push(path.join(outDir, f.replace(/\.wxs$/i, ".wixobj")));
    }
    await this.runCommand(`"${light}"`, lightArgs);
    console.log(`MSI: ${msi}`);
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
      console.log("Installer mode:");
      console.log("  1. interactive  (employee fills name, hours, and TOTP on the PC)");
      console.log("  2. preassigned  (you fill those now, then the PC only confirms TOTP)");
      const mode = (await this.prompt("Choice [1]: ")).trim().toLowerCase();
      const preassigned = mode === "2" || mode === "preassigned" || mode === "pre-assigned" || mode === "p";
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
        console.warn("x86 release build failed:", err.message);
      }
      await this.tryWix(preassigned);
      await this.trySign();
      await this.printOutputs();
    } finally {
      if (this.rl) this.rl.close();
    }
  }

  async printOutputs() {
    const artifacts = [
      path.join(this.root, "target", "wix", "Ftracker-1.0.0-x64.msi"),
      path.join(this.root, "target", "x86_64-pc-windows-msvc", "release", "ftracker.exe"),
      path.join(this.root, "target", "x86_64-pc-windows-msvc", "release", "ftracker-svc.exe"),
      path.join(this.root, "target", "i686-pc-windows-msvc", "release", "ftracker.exe"),
      path.join(this.root, "target", "i686-pc-windows-msvc", "release", "ftracker-svc.exe"),
    ];
    const found = [];
    for (const p of artifacts) {
      if (await this.fileExists(p)) {
        found.push(p);
      }
    }
    console.log("Build finished.");
    if (found.length === 0) {
      return;
    }
    console.log("Output:");
    for (const p of found) {
      console.log(`  ${p}`);
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
