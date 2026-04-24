#!/usr/bin/env node
const { execFileSync } = require("child_process");
const path = require("path");
const bin = path.join(
  __dirname,
  "bin",
  process.platform === "win32" ? "agentbridge.exe" : "agentbridge"
);
try {
  execFileSync(bin, process.argv.slice(2), { stdio: "inherit" });
} catch (e) {
  process.exit(e.status || 1);
}
