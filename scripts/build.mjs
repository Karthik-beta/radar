import { spawnSync } from "node:child_process";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("..", import.meta.url));

const features = process.env.RADAR_FEATURES;

const steps = [
  // Leptos erases component types under this cfg, which cuts about 10 KB gzipped from the UI.
  { cwd: "crates/web", cmd: "trunk", args: ["build", "--release"], env: { RUSTFLAGS: "--cfg erase_components" } },
  {
    cwd: "crates/worker",
    cmd: "worker-build",
    args: features ? ["--release", "--", "--features", features] : ["--release"],
  },
  { cwd: ".", cmd: process.execPath, args: ["scripts/strip-wasm-names.mjs", "crates/worker/build/index_bg.wasm"] },
];

for (const { cwd, cmd, args, env } of steps) {
  console.log(`> ${cmd} ${args.join(" ")} (in ${cwd})`);
  const result = spawnSync(cmd, args, { cwd: join(root, cwd), stdio: "inherit", env: { ...process.env, ...env } });
  if (result.error) {
    console.error(`${cmd} failed to start: ${result.error.message}`);
    process.exit(1);
  }
  if (result.status !== 0) {
    console.error(`${cmd} exited with ${result.status}`);
    process.exit(result.status ?? 1);
  }
}
