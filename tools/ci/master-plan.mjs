// Decides whether a push to master needs the heavy jobs again.
//
//   node tools/ci/master-plan.mjs <ci|click>
//
// Writes `run=true|false` and `reason=...` to $GITHUB_OUTPUT. A merge commit
// whose tree is the PR head's tree adds nothing the PR run did not already
// test, so the heavy jobs are skipped when that PR head is green. They still
// run when a lockfile, a Cargo.toml or a build/cache-key input changed
// (that master run is the one that saves the Actions caches), on tags, and
// whenever anything cannot be read (fail open).
import { execFileSync } from "node:child_process";
import { appendFileSync } from "node:fs";
import { pathToFileURL } from "node:url";

// Statuses the PR head must carry, as "<workflow> / <job> (pull_request)".
export const REQUIRED = {
  ci: ["CI / rust (pull_request)", "CI / app (pull_request)", "CI / audit (pull_request)"],
  click: ["Click / click (pull_request)"],
};

const KEY_FILES = new Set([
  "CMakeLists.txt",
  "qt/build.rs",
  "qt/stamp/build.rs",
  "tools/ci/click.sh",
  ".forgejo/workflows/ci.yml",
  ".forgejo/workflows/click.yml",
]);

// Files a cache key hashes: they decide whether master must save a new cache.
export function isKeyInput(path) {
  const base = path.split("/").pop();
  return base === "Cargo.toml" || base === "Cargo.lock" || KEY_FILES.has(path);
}

// Latest status per context wins (a re-run replaces the earlier result).
export function latestStates(statuses) {
  const byContext = new Map();
  for (const s of statuses ?? []) {
    const old = byContext.get(s.context);
    if (!old || s.id > old.id) byContext.set(s.context, s);
  }
  return byContext;
}

export function decide({ ref, parents, tree, headTree, statuses, changed, required }) {
  if (ref !== "refs/heads/master") return { run: true, reason: "not a push to master" };
  if (parents.length !== 2) return { run: true, reason: "not a merge commit" };
  if (!headTree || tree !== headTree) {
    return { run: true, reason: "master moved: the merge tree differs from the PR head" };
  }
  const states = latestStates(statuses);
  for (const ctx of required) {
    const s = states.get(ctx);
    if (!s || s.status !== "success") {
      return { run: true, reason: `PR head has no green "${ctx}"` };
    }
  }
  const key = changed.find(isKeyInput);
  if (key) return { run: true, reason: `${key} changed: this run saves the caches` };
  return { run: false, reason: "tree already tested green on the PR head" };
}

function git(...args) {
  return execFileSync("git", args, { encoding: "utf8" }).trim();
}

async function gather(env) {
  const parents = git("rev-list", "--parents", "-n", "1", "HEAD").split(" ").slice(1);
  const info = { ref: env.GITHUB_REF, parents, tree: git("rev-parse", "HEAD^{tree}") };
  if (info.ref !== "refs/heads/master" || parents.length !== 2) return info;
  info.headTree = git("rev-parse", `${parents[1]}^{tree}`);
  info.changed = git("diff", "--name-only", "--no-renames", parents[0], "HEAD").split("\n");
  const url = `${env.GITHUB_SERVER_URL}/api/v1/repos/${env.GITHUB_REPOSITORY}/commits/${parents[1]}/status`;
  const res = await fetch(url);
  if (!res.ok) throw new Error(`${url}: ${res.status}`);
  info.statuses = (await res.json()).statuses;
  return info;
}

async function main() {
  const required = REQUIRED[process.argv[2]];
  if (!required) throw new Error("usage: master-plan.mjs <ci|click>");
  let out;
  try {
    out = decide({ changed: [], statuses: [], ...(await gather(process.env)), required });
  } catch (e) {
    out = { run: true, reason: `plan failed (${e.message}); running everything` };
  }
  console.log(`run=${out.run} (${out.reason})`);
  if (process.env.GITHUB_OUTPUT) {
    appendFileSync(process.env.GITHUB_OUTPUT, `run=${out.run}\nreason=${out.reason}\n`);
  }
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) await main();
