import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";
import path from "node:path";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

export class Dev {
  constructor() {
    this.root = root;
  }

  runCommand(command, args) {
    return new Promise((resolve, reject) => {
      const child = spawn(command, args, {
        cwd: this.root,
        stdio: "inherit",
        shell: true,
      });
      child.on("exit", (code) => {
        if (code === 0) resolve();
        else reject(new Error(`${command} ${args.join(" ")} exited ${code}`));
      });
    });
  }

  async run() {
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
    await this.runCommand(exe, []);
  }
}

const isMain = process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url);
if (isMain) {
  new Dev().run().catch((err) => {
    console.error(err.message || err);
    process.exit(1);
  });
}
